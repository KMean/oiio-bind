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
use oiio::{algo, ImageBuf, ImageInput, ImageSpec, PixelFormat, Roi};

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
        .err()
        .expect("a 19-byte truncated header is not a readable image");

    // The point is that we got here at all. That the replacement character
    // survives into the message confirms the lossy path ran rather than the
    // reader having failed earlier for an unrelated reason.
    let text = error.to_string();
    assert!(
        text.contains('\u{fffd}'),
        "expected the undecodable attribute name to be replaced, got {text:?}"
    );
}
