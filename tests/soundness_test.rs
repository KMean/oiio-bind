//! Every input that could once take the process down, or read memory it did
//! not own, from safe Rust.
//!
//! These are not hypotheticals. Each was reproduced against the OpenImageIO
//! this crate links before the guard that now stops it was written: seven
//! entry points segfaulted on a deep `ImageBuf`, `ifft` walked off the end of
//! an ordinary overscan image, `pixel_hash_sha1` divided by zero on an empty
//! one, and `paste` reached `std::terminate` through a negative channel
//! offset. A regression here is a crash in someone's renderer, so each case
//! asserts an `Err` rather than merely surviving.
//!
//! The crate's claim is that safe Rust cannot cause undefined behaviour. This
//! file is that claim, written down.

mod common;

use oiio::algo::{FitMode, Operand};
use oiio::{algo, DeepImage, ImageBuf, ImageInput, ImageSpec, PixelFormat, Roi};

fn flat(width: u32, height: u32, channels: u32) -> ImageBuf {
    let spec = ImageSpec::new(width, height, channels, PixelFormat::F32).unwrap();
    let mut image = ImageBuf::new(&spec).unwrap();
    algo::zero(&mut image, None).unwrap();
    image
}

fn deep(width: u32, height: u32, channels: u32) -> ImageBuf {
    let spec = ImageSpec::new(width, height, channels, PixelFormat::F32)
        .unwrap()
        .as_deep();
    ImageBuf::new(&spec).unwrap()
}

/// A deep image has no contiguous pixels, so a kernel that walks them
/// dereferences null. `IBAprep` refuses deep images, but it is only ever shown
/// the destination and up to three sources — never an image passed separately,
/// and `fft` does not call it at all.
#[test]
fn deep_images_are_refused_by_everything_that_reads_flat_pixels() {
    let deep_source = deep(4, 4, 4);

    // fft skips IBAprep entirely and goes straight to paste.
    let mut dst = ImageBuf::empty().unwrap();
    assert!(algo::fft(&mut dst, &deep_source, None).is_err(), "fft");

    let mut dst = ImageBuf::empty().unwrap();
    assert!(algo::ifft(&mut dst, &deep_source, None).is_err(), "ifft");

    // paste and copy take the deep path only when the destination is deep or
    // unallocated; a flat, allocated one falls through to the flat kernel.
    let mut destination = flat(8, 8, 4);
    assert!(
        algo::paste(&mut destination, [0, 0, 0], 0, &deep_source, None).is_err(),
        "paste"
    );

    let mut destination = flat(8, 8, 4);
    assert!(
        algo::copy(&mut destination, &deep_source, None, None).is_err(),
        "copy"
    );

    // The convolution kernel is a third image, so IBAprep never sees it.
    let mut dst = ImageBuf::empty().unwrap();
    let source = flat(8, 8, 4);
    assert!(
        algo::convolve(&mut dst, &source, &deep_source, true, None).is_err(),
        "convolve kernel"
    );

    // As is st_warp's coordinate map.
    let mut dst = ImageBuf::empty().unwrap();
    assert!(
        algo::st_warp(
            &mut dst,
            &source,
            &deep_source,
            [0, 1],
            [false, false],
            &oiio::algo::WarpOptions::default(),
            None,
        )
        .is_err(),
        "st_warp coordinates"
    );

    let mut dst = ImageBuf::empty().unwrap();
    assert!(
        algo::channel_sum(&mut dst, &deep_source, &[1.0, 1.0, 1.0, 1.0], None).is_err(),
        "channel_sum"
    );

    let mut dst = ImageBuf::empty().unwrap();
    assert!(
        algo::transpose(&mut dst, &deep_source, None).is_err(),
        "transpose"
    );

    // The measurements too.
    assert!(algo::pixel_stats(&deep_source, None).is_err());
    assert!(algo::pixel_hash_sha1(&deep_source, "", None).is_err());
    assert!(algo::histogram(&deep_source, 0, 4, 0.0..1.0, false, None).is_err());
}

/// Deep into deep still works — the guard refuses the mixed case, not deep
/// images as such.
#[test]
fn deep_to_deep_still_copies() {
    let source = deep(4, 4, 4);
    let mut destination = deep(4, 4, 4);
    algo::copy(&mut destination, &source, None, None).unwrap();
    assert!(destination.is_deep());
}

/// `ifft` transforms the union of the source's data and display windows and
/// then reads the *source* at those coordinates. A data window that does not
/// start at the origin, or that is smaller than the display window, is read
/// past the end. The second shape is what every overscan EXR looks like, and
/// what `crop` produces.
#[test]
fn ifft_refuses_a_source_it_would_read_outside_of() {
    // Data window away from the origin.
    let offset = ImageSpec::new(8, 8, 2, PixelFormat::F32)
        .unwrap()
        .with_origin([100_000, 100_000, 0])
        .with_full_window([100_000, 100_000, 0], [8, 8, 1])
        .unwrap();
    let mut source = ImageBuf::new(&offset).unwrap();
    algo::fill(&mut source, &[0.5, 0.0], None).unwrap();
    let mut dst = ImageBuf::empty().unwrap();
    let error = algo::ifft(&mut dst, &source, None).unwrap_err();
    println!("data window off the origin: {error}");

    // Display window larger than the data window: an overscan image.
    let overscan = ImageSpec::new(8, 8, 2, PixelFormat::F32)
        .unwrap()
        .with_full_window([0, 0, 0], [4096, 4096, 1])
        .unwrap();
    let mut source = ImageBuf::new(&overscan).unwrap();
    algo::fill(&mut source, &[0.5, 0.0], None).unwrap();
    let mut dst = ImageBuf::empty().unwrap();
    let error = algo::ifft(&mut dst, &source, None).unwrap_err();
    println!("display window larger than the data window: {error}");

    // A buffer holding nothing at all.
    let nothing = ImageBuf::empty().unwrap();
    let mut dst = ImageBuf::empty().unwrap();
    assert!(algo::ifft(&mut dst, &nothing, None).is_err());
}

/// The ordinary round trip still works, so the guard has not cost anything a
/// caller needs.
#[test]
fn ifft_still_inverts_what_fft_produced() {
    let mut source = flat(16, 16, 1);
    algo::fill(&mut source, &[0.25], None).unwrap();

    let mut frequency = ImageBuf::empty().unwrap();
    algo::fft(&mut frequency, &source, None).unwrap();
    let mut back = ImageBuf::empty().unwrap();
    algo::ifft(&mut back, &frequency, None).unwrap();

    let roi = Roi::new(3..4, 5..6, 0..1, 0..1).unwrap();
    let mut one = [0.0_f32; 1];
    back.get_pixels_into(roi, &mut one).unwrap();
    assert!((one[0] - 0.25).abs() < 1e-3, "got {}", one[0]);
}

/// `simplePixelHashSHA1` divides by `roi.width() * pixel_bytes()` without
/// checking it. `ImageBuf::empty()` is the documented way to make a
/// destination, so handing one over by mistake is an ordinary slip.
#[test]
fn hashing_a_buffer_with_no_pixels_is_an_error() {
    let nothing = ImageBuf::empty().unwrap();
    let error = algo::pixel_hash_sha1(&nothing, "", None).unwrap_err();
    println!("empty buffer: {error}");
    assert!(error.to_string().contains("no pixels"), "{error}");
}

/// Three OpenImageIO fast paths take a raw pointer from the region rather than
/// going through a bounds-checked iterator, so a region larger than the source
/// reads past the allocation. The region is clamped to the source now.
#[test]
fn a_region_larger_than_the_source_does_not_read_past_it() {
    let source = flat(8, 8, 4);
    let huge = Roi::new(0..4096, 0..4096, 0..1, 0..4).unwrap();

    // simplePixelHashSHA1's memcpy over roi.width().
    let digest = algo::pixel_hash_sha1(&source, "", Some(huge)).unwrap();
    assert_eq!(digest.len(), 40);

    // The float-RGBA colour path's memcpy from the source's pixel address.
    let mut dst = ImageBuf::empty().unwrap();
    algo::color_matrix_transform(
        &mut dst,
        &source,
        &[
            1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
        ],
        false,
        Some(huge),
    )
    .unwrap();

    // And the statistics, which share the same clamp.
    let stats = algo::pixel_stats(&source, Some(huge)).unwrap();
    assert_eq!(
        stats.finite_count[0], 64,
        "only the real pixels are counted"
    );
}

/// `paste` offsets the source's channels by `first_channel` without clamping.
/// Far enough negative, the destination's channel count goes negative,
/// `default_channel_names` reserves that many, and throwing out of a `noexcept`
/// function calls `std::terminate`.
#[test]
fn paste_refuses_a_channel_offset_that_would_terminate_the_process() {
    let source = flat(4, 4, 3);

    for offset in [-3, -4, -10, -1000] {
        let mut destination = ImageBuf::empty().unwrap();
        let outcome = algo::paste(&mut destination, [0, 0, 0], offset, &source, None);
        assert!(
            outcome.is_err(),
            "a first channel of {offset} puts every source channel outside the \
             destination and should be refused, got {outcome:?}"
        );
    }

    // A negative offset that still leaves channels inside is legitimate: it
    // lands a later source channel on the destination's first.
    let mut destination = ImageBuf::empty().unwrap();
    algo::paste(&mut destination, [0, 0, 0], -1, &source, None).unwrap();
    assert_eq!(destination.spec().unwrap().channel_count(), 2);
}

/// `channel_sum` pads the weights against the *destination's* channel count,
/// which is one, and then reads them across the source's channels — past the
/// end of the caller's own slice.
#[test]
fn channel_sum_needs_one_weight_per_source_channel() {
    let source = flat(8, 8, 3);

    for weights in [&[][..], &[1.0][..], &[1.0, 1.0][..], &[1.0; 8][..]] {
        let mut dst = ImageBuf::empty().unwrap();
        let outcome = algo::channel_sum(&mut dst, &source, weights, None);
        assert!(
            outcome.is_err(),
            "{} weights for 3 channels should be refused",
            weights.len()
        );
    }

    let mut dst = ImageBuf::empty().unwrap();
    algo::channel_sum(&mut dst, &source, &[1.0, 1.0, 1.0], None).unwrap();
    assert_eq!(dst.spec().unwrap().channel_count(), 1);
}

/// `warp_` sizes its per-pixel scratch buffer from the destination and fills it
/// from the source, so a narrower destination is written past the end of a
/// stack allocation. `rotate` and an exact `fit` reach the same kernel.
#[test]
fn warping_into_a_narrower_destination_is_refused() {
    let source = flat(8, 8, 4);
    let identity = [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0];
    let options = oiio::algo::WarpOptions::default();

    let mut narrow = flat(8, 8, 2);
    assert!(
        algo::warp(&mut narrow, &source, &identity, &options, None).is_err(),
        "warp"
    );

    let mut narrow = flat(8, 8, 2);
    assert!(
        algo::rotate(&mut narrow, &source, 0.5, None, &options, None).is_err(),
        "rotate"
    );

    let mut narrow = flat(8, 8, 2);
    assert!(
        algo::fit(
            &mut narrow,
            &source,
            None,
            None,
            FitMode::default(),
            true,
            None
        )
        .is_err(),
        "fit"
    );

    // An equally wide or wider destination is fine.
    let mut same = flat(8, 8, 4);
    algo::warp(&mut same, &source, &identity, &options, None).unwrap();
}

/// The colour engine transforms the wrong channels for a region above channel
/// zero, and the repair for its scribble cannot reach where it lands.
#[test]
fn colour_operations_refuse_a_region_above_channel_zero() {
    let source = flat(8, 8, 6);
    let upper = Roi::new(0..8, 0..8, 0..1, 3..6).unwrap();
    let identity = [
        1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
    ];

    let mut dst = ImageBuf::empty().unwrap();
    let error =
        algo::color_matrix_transform(&mut dst, &source, &identity, false, Some(upper)).unwrap_err();
    println!("offset colour region: {error}");
    assert!(error.to_string().contains("channel zero"), "{error}");

    let mut dst = ImageBuf::empty().unwrap();
    assert!(algo::color_convert(&mut dst, &source, "linear", "sRGB", false, Some(upper)).is_err());

    // Beginning at zero is fine, and the channels past the fourth survive.
    let lower = Roi::new(0..8, 0..8, 0..1, 0..6).unwrap();
    let mut dst = ImageBuf::empty().unwrap();
    algo::color_matrix_transform(&mut dst, &source, &identity, false, Some(lower)).unwrap();
}

/// `ImageBuf::set_deep_samples` indexes the pixel without checking the range,
/// so an out-of-range coordinate resized a different pixel and reported
/// success — while the reader, which does check, called that pixel empty.
#[test]
fn deep_sample_counts_reject_a_coordinate_outside_the_image() {
    let mut image = deep(4, 4, 2);

    for (x, y) in [(4, 0), (-1, 2), (0, 4), (100, 100)] {
        let outcome = image.set_deep_sample_count(x, y, 3);
        assert!(outcome.is_err(), "{x},{y} should be outside the image");
    }

    // Nothing was written anywhere.
    for y in 0..4 {
        for x in 0..4 {
            assert_eq!(image.deep_sample_count(x, y), 0, "at {x},{y}");
        }
    }

    // A coordinate inside still works.
    image.set_deep_sample_count(3, 3, 2).unwrap();
    assert_eq!(image.deep_sample_count(3, 3), 2);
}

/// `CompareResults` is a plain aggregate. When the images disagree about
/// deepness OpenImageIO sets only its error flag and returns, leaving every
/// measurement whatever the stack held.
#[test]
fn comparing_images_that_cannot_be_compared_is_an_error() {
    let flat_image = flat(4, 4, 4);
    let deep_image = deep(4, 4, 4);

    let error = algo::compare(&flat_image, &deep_image, 0.0, 0.0, None).unwrap_err();
    println!("deep against flat: {error}");

    let nothing = ImageBuf::empty().unwrap();
    assert!(algo::compare(&flat_image, &nothing, 0.0, 0.0, None).is_err());

    // The ordinary comparison still reports.
    let summary = algo::compare(&flat_image, &flat_image, 0.0, 0.0, None).unwrap();
    assert_eq!(summary.max_error, 0.0);
    assert!(!summary.failed);
}

/// A sweep: hand a deep image, an empty one, and an oversized region to every
/// operation that takes them, and require an answer rather than a crash. This
/// is the net that catches the next one of these.
#[test]
fn no_operation_crashes_on_an_empty_or_deep_input() {
    let empty = ImageBuf::empty().unwrap();
    let deep_source = deep(4, 4, 4);
    let source = flat(8, 8, 4);
    let huge = Roi::new(-1000..1000, -1000..1000, 0..1, 0..4).unwrap();

    for probe in [&empty, &deep_source] {
        let mut dst = ImageBuf::empty().unwrap();
        let _ = algo::flip(&mut dst, probe, None);
        let _ = algo::flop(&mut dst, probe, None);
        let _ = algo::crop(&mut dst, probe, None);
        let _ = algo::cut(&mut dst, probe, None);
        let _ = algo::transpose(&mut dst, probe, None);
        let _ = algo::premult(&mut dst, probe, None);
        let _ = algo::unpremult(&mut dst, probe, None);
        let _ = algo::invert(&mut dst, probe, None);
        let _ = algo::laplacian(&mut dst, probe, None);
        let _ = algo::median_filter(&mut dst, probe, 3, None, None);
        let _ = algo::dilate(&mut dst, probe, 3, None, None);
        let _ = algo::erode(&mut dst, probe, 3, None, None);
        let _ = algo::unsharp_mask(&mut dst, probe, "gaussian", 3.0, 1.0, 0.0, None);
        let _ = algo::fft(&mut dst, probe, None);
        let _ = algo::ifft(&mut dst, probe, None);
        let _ = algo::polar_to_complex(&mut dst, probe, None);
        let _ = algo::complex_to_polar(&mut dst, probe, None);
        let _ = algo::min(&mut dst, probe, Operand::Constant(&[0.5]), None);
        let _ = algo::max(&mut dst, probe, Operand::Constant(&[0.5]), None);
        let _ = algo::flatten(&mut dst, probe, None);
        let _ = algo::deepen(&mut dst, probe, 1.0, None);
        let _ = algo::pixel_stats(probe, None);
        let _ = algo::histogram(probe, 0, 4, 0.0..1.0, false, None);
        let _ = algo::constant_color(probe, 0.0, None);
        let _ = algo::is_monochrome(probe, 0.0, None);
        let _ = algo::nonzero_region(probe, None);
        let _ = algo::pixel_hash_sha1(probe, "", None);
        let _ = algo::compare(probe, &source, 0.0, 0.0, None);
    }

    // The same operations with a region far outside the image.
    let mut dst = ImageBuf::empty().unwrap();
    let _ = algo::crop(&mut dst, &source, Some(huge));
    let _ = algo::flip(&mut dst, &source, Some(huge));
    let _ = algo::pixel_stats(&source, Some(huge));
    let _ = algo::pixel_hash_sha1(&source, "", Some(huge));
    let _ = algo::nonzero_region(&source, Some(huge));
    let _ = algo::histogram(&source, 0, 4, 0.0..1.0, true, Some(huge));
}

/// An error message OpenImageIO built out of bytes it read from the file.
///
/// The EXR reader quotes attribute names back at you when a header will not
/// parse, and a name in a corrupt file is whatever bytes happened to be there.
/// Handing those to cxx's `rust::String(std::string)` throws, and it throws
/// inside a shim declared `noexcept`, so the process aborted with
/// `STATUS_STACK_BUFFER_OVERRUN` instead of returning an error. Thirty of the
/// OpenEXR project's own fuzzer fixtures did this. Every shim now goes through
/// `rust::String::lossy`, which substitutes U+FFFD and cannot throw.
#[test]
fn error_message_with_invalid_utf8_is_an_error_not_an_abort() {
    // A 19-byte EXR: the magic number, a version, then an attribute whose name
    // is three bytes that are not valid UTF-8 in any encoding.
    let mut bytes = vec![0x76u8, 0x2f, 0x31, 0x01, 0x02, 0x00, 0x00, 0x00];
    bytes.extend_from_slice(b"\xff\xfe\xfd\0chlist\0");

    let error = ImageInput::from_memory("invalid-utf8.exr", bytes)
        .expect_err("a 19-byte truncated header is not a readable image");

    // The point is that we got here at all. That the replacement character
    // survives into the message confirms the lossy path ran rather than the
    // reader having failed earlier for an unrelated reason.
    let text = error.to_string();
    assert!(
        text.contains('\u{fffd}'),
        "expected the undecodable attribute name to be replaced, got {text:?}"
    );
}

/// A channel range starting at or past the image's channel count.
///
/// `ImageBuf::get_pixels` clamps `chend` to the channel count but never
/// `chbegin`, so `5..6` on a three channel image becomes `5..3`: a channel
/// count of -2. `ROI::contains` still passes, so it takes the fast path into
/// `copy_image`, which divides the pixel size by that count and memcpys with
/// it. `chbegin` exactly equal to the channel count divides by zero instead.
#[test]
fn a_channel_range_past_the_end_is_refused_by_the_buffer() {
    let spec = ImageSpec::new(4, 4, 3, PixelFormat::F32).unwrap();
    let mut buffer = ImageBuf::new(&spec).unwrap();

    // 5..6 was an access violation, 3..4 an integer divide by zero, and 2..3
    // is the last range that genuinely exists.
    for channels in [3..4_u32, 4..5, 5..6, 0..4] {
        let roi = Roi::new(0..4, 0..4, 0..1, channels.clone()).unwrap();
        let mut pixels = vec![-1.0_f32; roi.element_count().unwrap()];
        assert!(
            buffer.get_pixels_into(roi, &mut pixels).is_err(),
            "channels {channels:?} do not exist in a 3 channel image"
        );
        assert!(buffer.set_pixels(roi, &pixels).is_err());
    }

    let roi = Roi::new(0..4, 0..4, 0..1, 2..3).unwrap();
    let mut pixels = vec![-1.0_f32; roi.element_count().unwrap()];
    buffer.get_pixels_into(roi, &mut pixels).unwrap();
    assert_eq!(pixels, vec![0.0; 16]);
}

/// A deep `ImageBuf` reports `Storage::Local` but has no flat storage, and the
/// iterator leaves its proxy pointer null, so the contiguous pixel API read and
/// wrote through null. Reading a deep EXR back with `ImageBuf::read` is enough
/// to reach it, which is exactly what the documentation used to suggest.
#[test]
fn the_contiguous_pixel_api_refuses_a_deep_buffer() {
    let spec = ImageSpec::new(4, 4, 5, PixelFormat::F32)
        .unwrap()
        .with_channel_names(["R", "G", "B", "A", "Z"])
        .unwrap()
        .as_deep();
    let mut deep = ImageBuf::new(&spec).unwrap();
    assert!(deep.is_deep());

    let roi = Roi::new(0..4, 0..4, 0..1, 0..5).unwrap();
    let mut pixels = vec![0.0_f32; roi.element_count().unwrap()];
    let error = deep
        .get_pixels_into(roi, &mut pixels)
        .expect_err("a deep image has no flat pixels to read");
    assert!(error.to_string().contains("deep"), "{error}");
    assert!(deep.set_pixels(roi, &pixels).is_err());
}

/// OpenImageIO does not propagate a failed pixel allocation. It records the
/// error, leaves the buffer with null pixels, and hopes the caller notices; the
/// caller is the zero-fill, which asserts on `localpixels()` and then divides
/// by zero, so the constructor never returns. 416 GB is an ordinary "too big
/// for this machine" request, and the threshold is machine-dependent.
#[test]
fn a_spec_too_large_to_allocate_is_an_error_not_an_abort() {
    // A size that overflows the byte count rather than one that merely exceeds
    // the machine's memory. Both take the same path -- `new_pixels` catches the
    // exception, records the error and leaves the pixels null -- but this one
    // is refused by `new char[]` before a single page is touched. Asking for a
    // realistic-but-too-large 416 GB instead is machine-dependent: it fails
    // cleanly here and on Linux, and on macOS the allocator tries to back it
    // and the process is killed before it can fail.
    let huge = ImageSpec::new(i32::MAX as u32, i32::MAX as u32, 4, PixelFormat::F32).unwrap();
    assert!(
        ImageBuf::new(&huge).is_err(),
        "a spec whose byte count does not fit should not have been allocated"
    );

    // A deep spec is indexed by an int inside DeepData::init, so more pixels
    // than that can hold must be refused rather than allowed to truncate.
    let deep = ImageSpec::new(65_536, 65_536, 1, PixelFormat::F32)
        .unwrap()
        .as_deep();
    assert!(ImageBuf::new(&deep).is_err());

    // DeepImage::new reaches the same DeepData::init and needs the same cap:
    // 65536 x 65536 pixels truncate to a negative int, and the resize that
    // negative reaches throws out of a noexcept shim -- std::terminate.
    assert!(
        DeepImage::new(&deep).is_err(),
        "a deep image past the int pixel cap should be refused, not truncated"
    );
}

/// A buffer whose read failed handed back uninitialised heap and called it Ok.
///
/// OpenImageIO allocates the pixels before it opens the file, and does not zero
/// them. When the decode then fails it records the error and clears the valid
/// flag, but leaves the untouched allocation and the local storage in place;
/// `ImageBuf::get_pixels` gates its fast path on `localpixels()` alone and never
/// consults the flag. A caller who logged the read error and carried on got
/// whatever the heap held, with no way to tell.
#[test]
fn a_buffer_whose_read_failed_does_not_hand_back_the_heap() {
    let scratch = common::ScratchDir::new("failedread");
    let spec = ImageSpec::new(64, 64, 3, PixelFormat::F32).unwrap();
    let whole = scratch.file("whole.exr");
    common::write_image(&whole, &spec, &common::f32_ramp(64 * 64 * 3)).unwrap();

    // Header-valid, truncated part way through the pixel data: it opens, then
    // the read fails.
    let bytes = std::fs::read(&whole).unwrap();
    let cut = scratch.file("cut.exr");
    std::fs::write(&cut, &bytes[..bytes.len() * 2 / 3]).unwrap();

    let mut buffer = ImageBuf::from_path(&cut).unwrap();
    assert!(buffer.read().is_err(), "a truncated EXR should not read");

    let roi = buffer.spec().unwrap().data_window().unwrap();
    let mut pixels = vec![-1.0_f32; roi.element_count().unwrap()];
    assert!(
        buffer.get_pixels_into(roi, &mut pixels).is_err(),
        "these pixels were never read"
    );
    assert!(
        pixels.iter().all(|value| *value == 0.0),
        "the destination still holds whatever the allocation held"
    );

    // The deferred read is the same shape and must keep working: a buffer
    // attached to a good file and never explicitly read still serves pixels.
    let lazy = ImageBuf::from_path(&whole).unwrap();
    let mut through = vec![-1.0_f32; roi.element_count().unwrap()];
    lazy.get_pixels_into(roi, &mut through).unwrap();
    assert!(through.iter().any(|value| *value != -1.0));
}

/// An attribute whose type is an array with no concrete length.
///
/// `TypeDesc::fromstring` presets `arraylen` to -1, so `"float[]"` parses to -1
/// and `"uint8[-3]"` to -3, while `TypeDesc::size()` clamps the element count
/// to at least one -- so `"float[]"` measures four bytes and a four byte
/// payload passed the size check. The clamp is not applied on the way back out:
/// `sprint_type` and `format_type` both size their loop as
/// `arraylen ? arraylen : 1` with the raw value, and `size_t(-1)` is
/// 18446744073709551615, so reading the spec back walked off the end of a four
/// byte inline buffer.
#[test]
fn an_attribute_with_an_unsized_array_type_is_refused() {
    for type_name in ["float[]", "uint8[-3]", "int[-1]", "float[-1000]"] {
        let spec = ImageSpec::new(4, 4, 3, PixelFormat::U8)
            .unwrap()
            .with_attribute(
                "probe",
                oiio::AttributeValue::Other {
                    type_name: type_name.to_owned(),
                    value: String::new(),
                    bytes: vec![0_u8; 4],
                },
            );

        // Either the spec refuses it or the buffer does; what must not happen
        // is that it is stored and then stringified on the way back out.
        if let Ok(buffer) = ImageBuf::new(&spec) {
            let restored = buffer.spec().expect("reading the spec back");
            assert!(
                restored.attribute("probe").is_none(),
                "{type_name} was stored anyway"
            );
        }
    }

    // A concrete array still works.
    let spec = ImageSpec::new(4, 4, 3, PixelFormat::U8)
        .unwrap()
        .with_attribute(
            "good",
            oiio::AttributeValue::Other {
                type_name: "float[2]".to_owned(),
                value: String::new(),
                bytes: 1.0_f32
                    .to_ne_bytes()
                    .into_iter()
                    .chain(2.0_f32.to_ne_bytes())
                    .collect(),
            },
        );
    let buffer = ImageBuf::new(&spec).unwrap();
    assert!(buffer.spec().unwrap().attribute("good").is_some());
}

/// A channel range starting past the destination's last channel killed six
/// `algo` operations outright.
///
/// `IBAprep` begins every operation with
/// `roi = roi_intersection(roi, get_roi(dst->spec()))`, and `roi_intersection`
/// takes the larger begin and the smaller end. So 5..8 against 0..3 comes back
/// inverted, chbegin 5 and chend 3, and `ROI::nchannels()` is -2. The kernels
/// turn that into an unsigned length: `zero` reaches
/// `memcpy(.., nchannels * sizeof(T))` with `(size_t)-8` from an address
/// already five floats into a three float pixel.
#[test]
fn an_algo_region_cannot_start_past_the_destination() {
    let source = flat(4, 4, 3);
    let bad = Roi::new(0..4, 0..4, 0..1, 5..8).unwrap();

    let mut dst = ImageBuf::new(&ImageSpec::new(4, 4, 3, PixelFormat::F32).unwrap()).unwrap();
    assert!(algo::zero(&mut dst, Some(bad)).is_err());
    assert!(algo::fill(&mut dst, &[1.0, 2.0, 3.0], Some(bad)).is_err());
    assert!(algo::copy(&mut dst, &source, None, Some(bad)).is_err());
    assert!(algo::crop(&mut dst, &source, Some(bad)).is_err());
    assert!(algo::premult(&mut dst, &source, Some(bad)).is_err());
    assert!(algo::unpremult(&mut dst, &source, Some(bad)).is_err());

    // The same range against a destination that has those channels is fine.
    let mut wide = ImageBuf::new(&ImageSpec::new(4, 4, 8, PixelFormat::F32).unwrap()).unwrap();
    algo::zero(&mut wide, Some(bad)).unwrap();

    // And the whole-image default is untouched.
    let mut dst = ImageBuf::empty().unwrap();
    algo::copy(&mut dst, &source, None, None).unwrap();
    assert_eq!(dst.channel_count(), 3);
}

/// The quiet half of the region problem: results that were wrong rather than
/// fatal, and reported as Ok.
///
/// A `chend` past the channel count is the worst of them, because the strides
/// were computed from the range the caller asked for: OpenImageIO writes the
/// real channels at the wide stride and leaves every remaining slot holding
/// whatever the caller's buffer already had, which reads as data. A region
/// outside the data window comes back as zeros, because the iterator's default
/// wrap is `WrapBlack`, so an absent region is indistinguishable from a black
/// one. And `set_pixels` outside the window skips every pixel that does not
/// exist and still reports success, so the write goes nowhere.
#[test]
fn a_region_the_image_does_not_have_is_an_error_not_a_wrong_answer() {
    let spec = ImageSpec::new(2, 2, 3, PixelFormat::F32).unwrap();
    let mut buffer = ImageBuf::new(&spec).unwrap();
    let window = spec.data_window().unwrap();
    let values: Vec<f32> = (0..12).map(|value| value as f32).collect();
    buffer.set_pixels(window, &values).unwrap();

    // (a) more channels than exist: used to return Ok with the caller's own
    // stale bytes in the slots that were never written.
    let wide = window.with_channels(0..6).unwrap();
    let mut out = vec![-1.0_f32; wide.element_count().unwrap()];
    assert!(buffer.get_pixels_into(wide, &mut out).is_err());
    assert!(out.iter().all(|value| *value == -1.0));

    // (b) a region nowhere near the image: used to return Ok and all zeros.
    let elsewhere = Roi::new(1000..1002, 1000..1002, 0..1, 0..3).unwrap();
    let mut out = vec![-1.0_f32; elsewhere.element_count().unwrap()];
    assert!(buffer.get_pixels_into(elsewhere, &mut out).is_err());

    // A partial overlap is just as wrong, and just as quiet.
    let straddling = Roi::new(1..3, 0..2, 0..1, 0..3).unwrap();
    let mut out = vec![-1.0_f32; straddling.element_count().unwrap()];
    assert!(buffer.get_pixels_into(straddling, &mut out).is_err());

    // (c) writing outside the window: used to report success and do nothing.
    let payload = vec![9.0_f32; elsewhere.element_count().unwrap()];
    assert!(buffer.set_pixels(elsewhere, &payload).is_err());

    // The region the image does have still round-trips.
    let mut back = vec![0.0_f32; window.element_count().unwrap()];
    buffer.get_pixels_into(window, &mut back).unwrap();
    assert_eq!(back, values);
}

/// An `Other` attribute whose payload does not measure what its type says was
/// dropped by the shim and the refusal discarded, so the attribute vanished
/// between the spec and the file with nothing said. `is_writable` claimed it
/// would be written, too.
#[test]
fn an_attribute_that_cannot_be_written_says_so() {
    let short = oiio::AttributeValue::Other {
        type_name: "float2".to_owned(),
        value: String::new(),
        bytes: vec![0_u8; 4],
    };
    assert!(!short.is_writable(), "four bytes is not a float2");

    let spec = ImageSpec::new(4, 4, 3, PixelFormat::U8)
        .unwrap()
        .with_attribute("short", short);
    assert!(
        ImageBuf::new(&spec).is_err(),
        "the attribute cannot be carried and that has to be said"
    );

    // The right number of bytes for the declared type is fine.
    let exact = oiio::AttributeValue::Other {
        type_name: "float2".to_owned(),
        value: String::new(),
        bytes: vec![0_u8; 8],
    };
    assert!(exact.is_writable());
    let spec = ImageSpec::new(4, 4, 3, PixelFormat::U8)
        .unwrap()
        .with_attribute("exact", exact);
    let buffer = ImageBuf::new(&spec).unwrap();
    assert!(buffer.spec().unwrap().attribute("exact").is_some());
}

/// `mad` with two image sources of different channel counts.
///
/// The destination is sized from the wider source, and `IBAprep` allocates it
/// with `InitializePixels::No` -- nothing in OpenImageIO passes
/// `IBAprep_FILL_ZERO_ALLOC` -- so any channel the kernel does not write is
/// uninitialised heap handed back with a success return. The lines meant to
/// clear the trailing channels hold on the 3.1.12 this was developed against
/// and did not on the 3.1.14 CI builds: property testing found a one-against-two
/// and a six-against-three there, each leaving exactly the channels beyond the
/// narrower source holding heap. The shape is refused rather than made to
/// depend on which OpenImageIO is linked.
#[test]
fn mad_refuses_sources_that_disagree_on_channel_count() {
    let narrow = ImageBuf::new(&ImageSpec::new(5, 10, 3, PixelFormat::F32).unwrap()).unwrap();
    let wide = ImageBuf::new(&ImageSpec::new(5, 10, 6, PixelFormat::F32).unwrap()).unwrap();

    for (a, b) in [(&narrow, &wide), (&wide, &narrow)] {
        let mut dst = ImageBuf::empty().unwrap();
        let error = algo::mad(
            &mut dst,
            a,
            Operand::Image(b),
            Operand::Constant(&[0.5]),
            None,
        )
        .expect_err("three and six channels cannot be lined up");
        assert!(error.to_string().contains("channels"), "{error}");

        let mut dst = ImageBuf::empty().unwrap();
        assert!(algo::mad(&mut dst, a, Operand::Image(b), Operand::Image(b), None).is_err());
    }

    // The third operand counts too. a*b+c reads every image operand to the
    // union channel count, so it is not enough to line up the first two: the
    // iii variant used to check only a and b, and the ici variant (b constant,
    // a and c both images) checked nothing at all.
    let mut dst = ImageBuf::empty().unwrap();
    assert!(
        algo::mad(
            &mut dst,
            &narrow,
            Operand::Image(&narrow),
            Operand::Image(&wide),
            None
        )
        .is_err(),
        "the third image operand is wider and must be refused"
    );
    let mut dst = ImageBuf::empty().unwrap();
    assert!(
        algo::mad(
            &mut dst,
            &narrow,
            Operand::Constant(&[0.5]),
            Operand::Image(&wide),
            None
        )
        .is_err(),
        "b is a constant, a and c are unequal images and must be refused"
    );

    // Matching sources still work, and write every channel.
    let mut dst = ImageBuf::empty().unwrap();
    algo::mad(
        &mut dst,
        &wide,
        Operand::Image(&wide),
        Operand::Constant(&[0.5]),
        None,
    )
    .unwrap();
    let roi = dst.spec().unwrap().data_window().unwrap();
    let mut out = vec![-1.0_f32; roi.element_count().unwrap()];
    dst.get_pixels_into(roi, &mut out).unwrap();
    assert!(out.iter().all(|value| (*value - 0.5).abs() < 1e-6));

    // Three matching images write every channel too (the iii path).
    let mut dst = ImageBuf::empty().unwrap();
    algo::mad(
        &mut dst,
        &wide,
        Operand::Image(&wide),
        Operand::Image(&wide),
        None,
    )
    .unwrap();
    assert_eq!(dst.channel_count(), 6);
}
