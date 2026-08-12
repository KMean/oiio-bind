use std::{path::PathBuf, sync::Arc};

use anyhow::Result;
use oiio::{f16, Error, ImageCache, Roi};

fn fixture_path() -> PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/images/test16.png")
}

/// The OIIO 3.x bridge must keep the cache alive through ImageBuf's
/// std::shared_ptr even after Rust drops its original cache handle.
#[test]
fn imagebuf_retains_its_imagecache() {
    let cache = oiio_sys::imagecache::imagecache_create(false);
    assert!(!cache.is_null());

    let image_path =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/images/test16.png");
    let imagebuf = unsafe {
        oiio_sys::imagebuf::imagebuf_new_from_file(
            image_path.to_str().expect("test path must be UTF-8"),
            0,
            0,
            cache.clone(),
            std::ptr::null(),
            std::ptr::null_mut(),
        )
    };
    assert!(!imagebuf.is_null());

    drop(cache);

    let retained_cache = oiio_sys::imagebuf::imagebuf_imagecache(
        imagebuf.as_ref().expect("ImageBuf must remain alive"),
    );
    assert!(!retained_cache.is_null());
}

#[test]
fn safe_imagecache_reads_full_image_and_regions() -> Result<()> {
    let cache = ImageCache::builder()
        .max_memory_mb(64.0)
        .max_open_files(8)
        .autotile(8)
        .unassociated_alpha(true)
        .build()?;
    let path = fixture_path();
    let spec = cache.image_spec(&path)?;
    assert_eq!(spec.dimensions(), [16, 16, 1]);
    assert_eq!(spec.channel_count(), 4);
    let cached_spec = cache.image_spec_at(&path, 0, 0)?;
    assert_eq!(cached_spec.tile_dimensions()[0], 8);
    assert_eq!(cached_spec.tile_dimensions()[1], 8);

    let full_roi = spec.data_window()?;
    let mut pixels = vec![0_u16; full_roi.element_count()?];
    cache.get_pixels_into(&path, full_roi, &mut pixels)?;
    assert_pixel(&pixels, 0, 0, [65_535, 0, 0, 65_535]);
    assert_pixel(&pixels, 15, 15, [65_535, 65_535, 65_535, 65_535]);

    let first_two = Roi::new(0..2, 0..1, 0..1, 0..4)?;
    let mut subset = vec![0_u16; first_two.element_count()?];
    cache.get_pixels_into(&path, first_two, &mut subset)?;
    assert_eq!(subset, [65_535, 0, 0, 65_535, 61_166, 4_369, 0, 65_535]);

    let green_blue = Roi::new(0..2, 0..1, 0..1, 1..3)?;
    let mut channels = vec![0_u16; green_blue.element_count()?];
    cache.get_pixels_into(&path, green_blue, &mut channels)?;
    assert_eq!(channels, [0, 0, 4_369, 0]);

    cache.invalidate(&path, true)?;
    assert!(!cache.stats().is_empty());
    cache.reset_stats();
    Ok(())
}

#[test]
fn safe_imagecache_zero_fills_outside_data_window() -> Result<()> {
    let cache = ImageCache::new()?;
    let path = fixture_path();
    let roi = Roi::new(-1..1, -1..1, 0..1, 0..4)?;
    let mut pixels = vec![u16::MAX; roi.element_count()?];

    cache.get_pixels_into(&path, roi, &mut pixels)?;

    assert_eq!(&pixels[..12], &[0; 12]);
    assert_eq!(&pixels[12..], &[65_535, 0, 0, 65_535]);
    Ok(())
}

#[test]
fn safe_imagecache_supports_all_sealed_pixel_types() -> Result<()> {
    let cache = ImageCache::new()?;
    let path = fixture_path();
    let corner = Roi::new(0..1, 0..1, 0..1, 0..4)?;

    let mut u8_pixels = [0_u8; 4];
    cache.get_pixels_into(&path, corner, &mut u8_pixels)?;
    assert_eq!(u8_pixels, [255, 0, 0, 255]);

    let mut half_pixels = [f16::ZERO; 4];
    cache.get_pixels_into(&path, corner, &mut half_pixels)?;
    assert_eq!(half_pixels, [f16::ONE, f16::ZERO, f16::ZERO, f16::ONE]);

    let mut float_pixels = [0.0_f32; 4];
    cache.get_pixels_into(&path, corner, &mut float_pixels)?;
    assert_eq!(float_pixels, [1.0, 0.0, 0.0, 1.0]);
    Ok(())
}

#[test]
fn safe_imagecache_rejects_buffers_channels_and_indices_before_reading() -> Result<()> {
    let cache = ImageCache::new()?;
    let path = fixture_path();
    let roi = Roi::new(0..16, 0..16, 0..1, 0..4)?;
    let sentinel = 0xA55A_u16;
    let mut storage = vec![sentinel; 1_040];

    let error = cache
        .get_pixels_into(&path, roi, &mut storage[..1_023])
        .expect_err("short cache buffer must fail");
    assert!(matches!(
        error,
        Error::BufferLength {
            expected: 1_024,
            actual: 1_023
        }
    ));
    assert!(storage.iter().all(|&value| value == sentinel));

    let invalid_channels = Roi::new(0..1, 0..1, 0..1, 3..5)?;
    let error = cache
        .get_pixels_into(&path, invalid_channels, &mut [0_u16; 2])
        .expect_err("out-of-range channels must fail");
    assert!(matches!(error, Error::InvalidRoi(_)));

    let error = cache
        .image_spec_at(&path, u32::MAX, 0)
        .expect_err("indices larger than i32 must fail");
    assert!(matches!(error, Error::InvalidImageSpec(_)));
    Ok(())
}

#[test]
fn sys_bounded_cache_read_defensively_rejects_short_byte_slice() -> Result<()> {
    let path = fixture_path();
    let mut cache = oiio_sys::imagecache::imagecache_create(false);
    let roi = oiio_sys::imageio::ROI {
        xbegin: 0,
        xend: 16,
        ybegin: 0,
        yend: 16,
        zbegin: 0,
        zend: 1,
        chbegin: 0,
        chend: 4,
    };
    let format = oiio_sys::typedesc::typedesc_from_basetype_arraylen(
        oiio_sys::typedesc::BaseType::Uint16,
        0,
    );
    let sentinel = 0xA5A5_u16;
    let mut storage = vec![sentinel; 1_024];
    let bytes = unsafe { std::slice::from_raw_parts_mut(storage.as_mut_ptr().cast::<u8>(), 2_048) };
    let mut error = String::from("stale error");

    let succeeded = unsafe {
        oiio_sys::imagecache::imagecache_get_pixels_span_with_error(
            cache.pin_mut_unchecked(),
            path.to_str().expect("test path must be UTF-8"),
            0,
            0,
            &roi,
            format,
            &mut bytes[..2_046],
            &mut error,
        )
    };

    assert!(!succeeded);
    assert!(error.contains("destination buffer"));
    assert!(storage.iter().all(|&value| value == sentinel));
    Ok(())
}

#[test]
fn sys_cache_dimensions_copy_retains_a_valid_format() {
    let path = fixture_path();
    let mut cache = oiio_sys::imagecache::imagecache_create(false);
    let spec = unsafe {
        oiio_sys::imagecache::imagecache_get_cache_dimensions_copy(
            cache.pin_mut_unchecked(),
            path.to_str().expect("test path must be UTF-8"),
            0,
            0,
        )
    };

    assert!(!spec.is_null());
    assert!(oiio_sys::imageio::imagespec_valid(
        spec.as_ref().expect("cache dimension spec must exist")
    ));
}

#[test]
fn safe_imagecache_reports_missing_files_and_invalid_settings() {
    assert!(matches!(
        ImageCache::builder().max_memory_mb(f32::NAN).build(),
        Err(Error::InvalidCacheSetting { .. })
    ));
    assert!(matches!(
        ImageCache::builder().max_open_files(0).build(),
        Err(Error::InvalidCacheSetting { .. })
    ));

    let cache = ImageCache::new().expect("cache creation must succeed");
    let path = fixture_path().with_file_name("does-not-exist.png");
    let error = cache
        .image_spec(&path)
        .expect_err("missing image must return an error");
    match error {
        Error::Operation { message, .. } => assert!(!message.is_empty()),
        other => panic!("unexpected error: {other}"),
    }
}

#[test]
fn imagecache_can_be_shared_across_threads() -> Result<()> {
    let cache = Arc::new(ImageCache::new()?);
    let barrier = Arc::new(std::sync::Barrier::new(4));
    let path = fixture_path();
    let roi = Roi::new(0..16, 0..16, 0..1, 0..4)?;
    let mut threads = Vec::new();

    for _ in 0..4 {
        let cache = Arc::clone(&cache);
        let barrier = Arc::clone(&barrier);
        let path = path.clone();
        threads.push(std::thread::spawn(move || {
            barrier.wait();
            for _ in 0..32 {
                let mut pixels = vec![0_u16; 1_024];
                cache.get_pixels_into(&path, roi, &mut pixels).unwrap();
                assert_pixel(&pixels, 15, 15, [65_535, 65_535, 65_535, 65_535]);
            }
        }));
    }

    for thread in threads {
        thread.join().expect("cache worker must not panic");
    }
    Ok(())
}

fn assert_pixel(pixels: &[u16], x: usize, y: usize, expected: [u16; 4]) {
    let offset = (y * 16 + x) * 4;
    assert_eq!(pixels[offset..offset + 4], expected);
}
