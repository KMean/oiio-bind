//! Handles, per-thread state, and tile guards.

mod common;

use std::sync::Arc;

use common::{f16_ramp, f32_ramp, write_image, ScratchDir};
use oiio::{f16, Error, ImageCache, ImageSpec, PixelFormat, Roi};

/// 32x32, 3 channels, float, 16x16 tiles.
fn tiled_fixture(scratch: &ScratchDir, name: &str) -> (std::path::PathBuf, ImageSpec, Vec<f32>) {
    let path = scratch.file(name);
    let spec = ImageSpec::new(32, 32, 3, PixelFormat::F32)
        .unwrap()
        .with_tile_size([16, 16, 1])
        .unwrap();
    let pixels = f32_ramp(spec.element_count().unwrap());
    write_image(&path, &spec, &pixels).unwrap();
    (path, spec, pixels)
}

#[test]
fn a_handle_reads_the_same_pixels_as_the_file_name() {
    let scratch = ScratchDir::new("handle");
    let (path, spec, whole) = tiled_fixture(&scratch, "tiled.exr");
    let cache = ImageCache::new().unwrap();

    let roi = spec.data_window().unwrap();
    let mut by_name = vec![0.0_f32; roi.element_count().unwrap()];
    cache.get_pixels_into(&path, roi, &mut by_name).unwrap();

    let handle = cache.handle(&path).unwrap();
    let mut by_handle = vec![0.0_f32; roi.element_count().unwrap()];
    handle.get_pixels_into(roi, &mut by_handle).unwrap();

    assert_eq!(by_name, whole);
    assert_eq!(by_handle, by_name);
    assert!(handle.is_good());
    assert!(handle.filename().contains("tiled.exr"));
}

#[test]
fn a_handle_reads_regions_and_channel_subsets() {
    let scratch = ScratchDir::new("handleregion");
    let (path, spec, _) = tiled_fixture(&scratch, "tiled.exr");
    let cache = ImageCache::new().unwrap();
    let handle = cache.handle(&path).unwrap();

    let roi = spec
        .data_window()
        .unwrap()
        .with_x(0..16)
        .unwrap()
        .with_y(0..16)
        .unwrap()
        .with_channels(1..3)
        .unwrap();

    let mut by_handle = vec![0.0_f32; roi.element_count().unwrap()];
    handle.get_pixels_into(roi, &mut by_handle).unwrap();

    let mut by_name = vec![0.0_f32; roi.element_count().unwrap()];
    cache.get_pixels_into(&path, roi, &mut by_name).unwrap();

    assert_eq!(by_handle.len(), 16 * 16 * 2);
    assert_eq!(by_handle, by_name);
}

#[test]
fn a_handle_rejects_a_buffer_that_does_not_match_the_region() {
    let scratch = ScratchDir::new("handlebuffer");
    let (path, spec, _) = tiled_fixture(&scratch, "tiled.exr");
    let cache = ImageCache::new().unwrap();
    let handle = cache.handle(&path).unwrap();

    let roi = spec.data_window().unwrap();
    let mut short = vec![0.0_f32; roi.element_count().unwrap() - 1];
    assert!(matches!(
        handle.get_pixels_into(roi, &mut short),
        Err(Error::BufferLength { .. })
    ));
}

#[test]
fn resolving_a_missing_file_fails() {
    let scratch = ScratchDir::new("handlemissing");
    let cache = ImageCache::new().unwrap();
    let missing = scratch.file("does-not-exist.exr");
    assert!(matches!(
        cache.handle(&missing),
        Err(Error::OpenImage { .. })
    ));
}

#[test]
fn per_thread_state_accelerates_reads_without_changing_them() {
    let scratch = ScratchDir::new("perthread");
    let (path, spec, whole) = tiled_fixture(&scratch, "tiled.exr");
    let cache = ImageCache::new().unwrap();
    let handle = cache.handle(&path).unwrap();
    let thread_state = cache.thread_state().unwrap();

    let roi = spec.data_window().unwrap();
    let mut pixels = vec![0.0_f32; roi.element_count().unwrap()];
    handle
        .get_pixels_into_with(&thread_state, 0, 0, roi, &mut pixels)
        .unwrap();

    assert_eq!(pixels, whole);
}

#[test]
fn a_handle_can_be_shared_across_threads() {
    let scratch = ScratchDir::new("handlethreads");
    let (path, spec, whole) = tiled_fixture(&scratch, "tiled.exr");
    let cache = Arc::new(ImageCache::new().unwrap());

    // The handle borrows the cache, so build it inside the scope that shares
    // it. Each thread makes its own per-thread state, which is what
    // OpenImageIO requires.
    let handle = cache.handle(&path).unwrap();
    let handle = &handle;
    let roi = spec.data_window().unwrap();
    let expected = &whole;

    std::thread::scope(|scope| {
        for _ in 0..4 {
            let cache = Arc::clone(&cache);
            scope.spawn(move || {
                let thread_state = cache.thread_state().unwrap();
                let mut pixels = vec![0.0_f32; roi.element_count().unwrap()];
                handle
                    .get_pixels_into_with(&thread_state, 0, 0, roi, &mut pixels)
                    .unwrap();
                assert_eq!(&pixels, expected);
            });
        }
    });
}

#[test]
fn a_tile_exposes_its_own_region_and_pixels() {
    let scratch = ScratchDir::new("tile");
    let (path, _, whole) = tiled_fixture(&scratch, "tiled.exr");
    let cache = ImageCache::new().unwrap();

    // Any coordinate inside the tile resolves to that tile.
    let tile = cache.tile(&path, 0, 0, [20, 4, 0], 0..3).unwrap();
    let roi = tile.roi();
    assert_eq!(roi.x(), 16..32);
    assert_eq!(roi.y(), 0..16);
    assert_eq!(tile.format(), PixelFormat::F32);

    let pixels = tile.pixels::<f32>().unwrap();
    assert_eq!(pixels.len(), roi.element_count().unwrap());

    // Spot-check against the whole image, using the tile's own origin.
    let width = 32usize;
    let channels = 3usize;
    let first = (roi.y().start as usize * width + roi.x().start as usize) * channels;
    assert_eq!(&pixels[0..channels], &whole[first..first + channels]);
}

#[test]
fn a_tile_refuses_a_pixel_type_it_does_not_hold() {
    let scratch = ScratchDir::new("tileformat");
    let (path, _, _) = tiled_fixture(&scratch, "tiled.exr");
    let cache = ImageCache::new().unwrap();

    let tile = cache.tile(&path, 0, 0, [0, 0, 0], 0..3).unwrap();
    assert_eq!(tile.format(), PixelFormat::F32);

    // The tile holds float, so asking for half must fail rather than
    // reinterpret the bytes.
    let error = tile.pixels::<f16>().unwrap_err();
    assert!(matches!(
        error,
        Error::TilePixelFormat {
            requested: PixelFormat::F16,
            actual: PixelFormat::F32
        }
    ));
    assert!(tile.pixels::<u8>().is_err());
    assert!(tile.pixels::<f32>().is_ok());
}

#[test]
fn a_half_tile_reports_half() {
    let scratch = ScratchDir::new("tilehalf");
    let path = scratch.file("half.exr");
    let spec = ImageSpec::new(16, 16, 3, PixelFormat::F16)
        .unwrap()
        .with_tile_size([16, 16, 1])
        .unwrap();
    let written = f16_ramp(spec.element_count().unwrap());
    write_image(&path, &spec, &written).unwrap();

    let cache = ImageCache::new().unwrap();
    let tile = cache.tile(&path, 0, 0, [0, 0, 0], 0..3).unwrap();
    assert_eq!(tile.format(), PixelFormat::F16);
    assert_eq!(tile.pixels::<f16>().unwrap(), &written[..]);
    assert!(tile.pixels::<f32>().is_err());
}

#[test]
fn releasing_many_tiles_does_not_exhaust_the_cache() {
    let scratch = ScratchDir::new("tilerelease");
    let (path, _, _) = tiled_fixture(&scratch, "tiled.exr");
    // A cache small enough that leaked tiles would be noticed.
    let cache = ImageCache::builder().max_memory_mb(1.0).build().unwrap();

    // Every guard is dropped at the end of each iteration.
    for _ in 0..200 {
        for y in [0, 16] {
            for x in [0, 16] {
                let tile = cache.tile(&path, 0, 0, [x, y, 0], 0..3).unwrap();
                assert_eq!(tile.pixels::<f32>().unwrap().len(), 16 * 16 * 3);
            }
        }
    }
}

#[test]
fn a_tile_outside_the_image_is_reported() {
    let scratch = ScratchDir::new("tileoutside");
    let (path, _, _) = tiled_fixture(&scratch, "tiled.exr");
    let cache = ImageCache::new().unwrap();

    // OpenImageIO would return a tile covering 992..1008 on both axes here.
    assert!(matches!(
        cache.tile(&path, 0, 0, [1_000, 1_000, 0], 0..3),
        Err(Error::InvalidRegion { axis: "x", .. })
    ));
    assert!(matches!(
        cache.tile(&path, 0, 0, [0, 40, 0], 0..3),
        Err(Error::InvalidRegion { axis: "y", .. })
    ));
    assert!(matches!(
        cache.tile(&path, 0, 0, [-1, 0, 0], 0..3),
        Err(Error::InvalidRegion { axis: "x", .. })
    ));
    // Empty and over-wide channel ranges.
    assert!(cache.tile(&path, 0, 0, [0, 0, 0], 2..2).is_err());
    assert!(matches!(
        cache.tile(&path, 0, 0, [0, 0, 0], 0..5),
        Err(Error::InvalidRoi(_))
    ));
    // The last valid coordinate still works.
    assert!(cache.tile(&path, 0, 0, [31, 31, 0], 0..3).is_ok());
}

/// Per-thread state must not be sendable between threads, and handles must be.
/// These are compile-time facts, asserted here so a future change that breaks
/// them fails the build.
#[test]
fn thread_safety_is_modelled_as_openimageio_documents_it() {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<ImageCache>();
    assert_send_sync::<oiio::ImageHandle<'static>>();

    // Perthread and TileGuard are deliberately not Send or Sync; there is no
    // stable way to assert a negative bound, so this is documented by the
    // types holding raw pointers plus a PhantomData marker.
}

/// Reads through a handle used to skip validation the by-name read performs.
///
/// `ImageCache::get_pixels_into_at` fetches the spec, refuses deep images and
/// checks the channel range against it. `ImageHandle::read` only checked that
/// the destination was the right length, so a channel range past the end of the
/// image went straight through. OpenImageIO does not clamp it: `get_pixels`
/// takes `result_nchans = chend - chbegin` and hands that to
/// `convert_pixel_values` against a tile that holds `spec.nchannels`, so asking
/// eight channels of a three channel image reads 8/3 past every tile row and
/// reports success. The validation now lives in the shim, where the handle is
/// already resolved and costs no second name lookup.
#[test]
fn a_handle_refuses_a_channel_range_the_image_does_not_have() {
    let scratch = ScratchDir::new("handlechan");
    let (path, spec, _) = tiled_fixture(&scratch, "tiled.exr");
    let cache = ImageCache::new().unwrap();
    let handle = cache.handle(&path).unwrap();

    let window = spec.data_window().unwrap();
    // Three channels exist. Ask for eight.
    let beyond = Roi::new(window.x(), window.y(), window.z(), 0..8).unwrap();

    let mut pixels = vec![0.0_f32; beyond.element_count().unwrap()];
    let error = handle
        .get_pixels_into(beyond, &mut pixels)
        .expect_err("channels 0..8 do not exist in a 3 channel image");
    let text = error.to_string();
    assert!(
        text.contains("channel range"),
        "expected the channel range to be named, got {text:?}"
    );

    // And the by-name path, which already rejected it, still does.
    let mut same = vec![0.0_f32; beyond.element_count().unwrap()];
    assert!(cache.get_pixels_into(&path, beyond, &mut same).is_err());
}

/// A tile's region and format are recorded when it is borrowed, not asked for
/// afterwards. OpenImageIO derives both from the *file's* current spec, which
/// invalidation frees, so `TileGuard::pixels` would otherwise build a slice
/// whose length came out of freed memory.
///
/// Invalidating while a guard is alive is now a compile error rather than a use
/// after free:
///
/// ```compile_fail
/// # use oiio::ImageCache;
/// # fn main() -> oiio::Result<()> {
/// let mut cache = ImageCache::new()?;
/// let tile = cache.tile(std::path::Path::new("x.exr"), 0, 0, [0, 0, 0], 0..3)?;
/// cache.invalidate_all(true);   // needs &mut, and `tile` holds a &
/// let _ = tile.pixels::<f32>()?;
/// # Ok(())
/// # }
/// ```
#[test]
fn a_tile_reports_the_geometry_it_was_borrowed_with() {
    let scratch = ScratchDir::new("tilesnapshot");
    let (path, _, _) = tiled_fixture(&scratch, "tiled.exr");
    let cache = ImageCache::new().unwrap();

    let tile = cache.tile(&path, 0, 0, [4, 4, 0], 0..3).unwrap();
    let roi = tile.roi();
    assert_eq!(tile.format(), PixelFormat::F32);
    // The fixture is 16x16 tiled, so the tile containing (4,4) is 0..16.
    assert_eq!(roi.x(), 0..16);
    assert_eq!(roi.y(), 0..16);
    assert_eq!(
        tile.pixels::<f32>().unwrap().len(),
        roi.element_count().unwrap()
    );
}

/// A caller-managed `Perthread` must be passed back through
/// `get_perthread_info` before each use. That call is where OpenImageIO acts on
/// the purge flag an invalidation sets; without it the record's two-tile
/// microcache keeps handing back pre-invalidation tiles, so a read after an
/// invalidate returns the old file's pixels. The shim now routes every
/// caller-supplied record through it.
#[test]
fn per_thread_state_sees_an_invalidated_file() {
    let scratch = ScratchDir::new("perthreadinval");
    let path = scratch.file("changing.exr");
    let spec = ImageSpec::new(32, 32, 3, PixelFormat::F32)
        .unwrap()
        .with_tile_size([16, 16, 1])
        .unwrap();
    let count = spec.element_count().unwrap();

    write_image(&path, &spec, &vec![0.25_f32; count]).unwrap();

    let mut cache = ImageCache::new().unwrap();
    let window = spec.data_window().unwrap();
    let mut pixels = vec![0.0_f32; count];

    {
        let state = cache.thread_state().unwrap();
        let handle = cache.handle(&path).unwrap();
        handle
            .get_pixels_into_with(&state, 0, 0, window, &mut pixels)
            .unwrap();
        assert!(pixels.iter().all(|value| *value == 0.25));
    }

    // Replace the file on disk with different pixels, then invalidate. The
    // borrow checker requires the handle and the state to be gone by here,
    // which is the other half of the fix.
    write_image(&path, &spec, &vec![0.75_f32; count]).unwrap();
    cache.invalidate(&path, true).unwrap();

    let state = cache.thread_state().unwrap();
    let handle = cache.handle(&path).unwrap();
    handle
        .get_pixels_into_with(&state, 0, 0, window, &mut pixels)
        .unwrap();
    assert!(
        pixels.iter().all(|value| *value == 0.75),
        "the read after invalidate returned the pre-invalidation pixels"
    );
}

/// A shared lifetime is not a shared identity.
///
/// `Perthread<'cache>` and `ImageHandle<'cache>` are both covariant in
/// `'cache`, so two caches alive in the same scope can produce a record and a
/// handle that share a lifetime parameter, and the call type-checks. It must
/// still be refused: a record is registered only with the cache that created
/// it, so the other cache's invalidation can never purge it and the microcache
/// keeps serving tiles from a file that has since been re-read. At teardown the
/// record also still holds a tile reference into a cache that is gone.
#[test]
fn per_thread_state_from_another_cache_is_refused() {
    let scratch = ScratchDir::new("crosscache");
    let (path, spec, whole) = tiled_fixture(&scratch, "tiled.exr");

    let cache_a = ImageCache::new().unwrap();
    let cache_b = ImageCache::new().unwrap();

    let state_of_a = cache_a.thread_state().unwrap();
    let handle_of_b = cache_b.handle(&path).unwrap();

    let window = spec.data_window().unwrap();
    let mut pixels = vec![0.0_f32; window.element_count().unwrap()];

    let error = handle_of_b
        .get_pixels_into_with(&state_of_a, 0, 0, window, &mut pixels)
        .expect_err("the record belongs to a different cache");
    assert!(
        error.to_string().contains("different image cache"),
        "{error}"
    );

    // The matching pair still works.
    let state_of_b = cache_b.thread_state().unwrap();
    handle_of_b
        .get_pixels_into_with(&state_of_b, 0, 0, window, &mut pixels)
        .unwrap();
    assert_eq!(pixels, whole);
}

/// A deep file read through a handle used to report success and return zeros,
/// where the same read by file name returned `Error::UnsupportedDeepImage`.
/// Both paths refuse it, and both say the same thing.
#[test]
fn a_deep_file_is_refused_the_same_way_through_both_cache_paths() {
    let scratch = ScratchDir::new("deephandle");
    let path = scratch.file("deep.exr");

    let spec = ImageSpec::new(4, 4, 5, PixelFormat::F32)
        .unwrap()
        .with_channel_names(["R", "G", "B", "A", "Z"])
        .unwrap()
        .as_deep();
    let mut deep = oiio::DeepImage::new(&spec).unwrap();
    for y in 0..4 {
        for x in 0..4 {
            deep.set_sample_count(x, y, 1).unwrap();
            deep.set_value(x, y, 4, 0, 10.0).unwrap();
        }
    }
    let mut output = oiio::ImageOutput::create(&path, &spec).unwrap();
    output.write_deep_image(&deep).unwrap();
    output.close().unwrap();

    let cache = ImageCache::new().unwrap();
    let window = Roi::new(0..4, 0..4, 0..1, 0..5).unwrap();
    let mut pixels = vec![-1.0_f32; window.element_count().unwrap()];

    assert!(matches!(
        cache.get_pixels_into(&path, window, &mut pixels),
        Err(Error::UnsupportedDeepImage)
    ));

    let handle = cache.handle(&path).unwrap();
    assert!(matches!(
        handle.get_pixels_into(window, &mut pixels),
        Err(Error::UnsupportedDeepImage)
    ));
    assert!(pixels.iter().all(|value| *value == -1.0));
}
