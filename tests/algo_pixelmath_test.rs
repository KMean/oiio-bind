//! The per-pixel arithmetic that was missing: mad, pow, clamp, min, max,
//! contrast_remap, saturate, invert, paste and cut.

mod common;

use oiio::algo::{ContrastRemap, Operand};
use oiio::{algo, ImageBuf, ImageSpec, PixelFormat, Roi};

/// An image whose every pixel holds the same value per channel.
fn flat(width: u32, height: u32, values: &[f32]) -> ImageBuf {
    let spec = ImageSpec::new(width, height, values.len() as u32, PixelFormat::F32).unwrap();
    let mut image = ImageBuf::new(&spec).unwrap();
    algo::fill(&mut image, values, None).unwrap();
    image
}

/// Every channel of one pixel.
fn pixel(image: &ImageBuf, x: i32, y: i32) -> Vec<f32> {
    let channels = image.spec().unwrap().channel_count();
    let roi = Roi::new(x..x + 1, y..y + 1, 0..1, 0..channels).unwrap();
    let mut values = vec![0.0_f32; channels as usize];
    image.get_pixels_into(roi, &mut values).unwrap();
    values
}

fn close(actual: &[f32], expected: &[f32]) {
    assert_eq!(actual.len(), expected.len(), "{actual:?} vs {expected:?}");
    for (a, e) in actual.iter().zip(expected) {
        assert!(
            (a - e).abs() < 1e-5,
            "expected {expected:?}, got {actual:?}"
        );
    }
}

#[test]
fn mad_covers_every_operand_combination() {
    let a = flat(4, 4, &[2.0, 3.0, 4.0]);
    let b = flat(4, 4, &[5.0, 5.0, 5.0]);
    let c = flat(4, 4, &[1.0, 1.0, 1.0]);
    let spec = ImageSpec::new(4, 4, 3, PixelFormat::F32).unwrap();

    // image * image + image
    let mut result = ImageBuf::new(&spec).unwrap();
    algo::mad(
        &mut result,
        &a,
        Operand::Image(&b),
        Operand::Image(&c),
        None,
    )
    .unwrap();
    close(&pixel(&result, 0, 0), &[11.0, 16.0, 21.0]);

    // image * image + constant
    let mut result = ImageBuf::new(&spec).unwrap();
    algo::mad(
        &mut result,
        &a,
        Operand::Image(&b),
        Operand::Constant(&[10.0]),
        None,
    )
    .unwrap();
    close(&pixel(&result, 0, 0), &[20.0, 25.0, 30.0]);

    // image * constant + image
    let mut result = ImageBuf::new(&spec).unwrap();
    algo::mad(
        &mut result,
        &a,
        Operand::Constant(&[10.0]),
        Operand::Image(&c),
        None,
    )
    .unwrap();
    close(&pixel(&result, 0, 0), &[21.0, 31.0, 41.0]);

    // image * constant + constant, with one value per channel
    let mut result = ImageBuf::new(&spec).unwrap();
    algo::mad(
        &mut result,
        &a,
        Operand::Constant(&[2.0, 3.0, 4.0]),
        Operand::Constant(&[0.5, 0.5, 0.5]),
        None,
    )
    .unwrap();
    close(&pixel(&result, 0, 0), &[4.5, 9.5, 16.5]);
}

/// Too few constants repeat the last one, which is how one value covers every
/// channel.
#[test]
fn a_short_constant_slice_repeats_its_last_value() {
    let a = flat(2, 2, &[1.0, 1.0, 1.0]);
    let spec = ImageSpec::new(2, 2, 3, PixelFormat::F32).unwrap();

    let mut result = ImageBuf::new(&spec).unwrap();
    algo::mad(
        &mut result,
        &a,
        Operand::Constant(&[2.0, 7.0]),
        Operand::Constant(&[0.0]),
        None,
    )
    .unwrap();
    // The second value covers the third channel too.
    close(&pixel(&result, 0, 0), &[2.0, 7.0, 7.0]);
}

#[test]
fn invert_subtracts_from_one() {
    let source = flat(4, 4, &[0.25, 0.5, 1.0]);
    let mut result = ImageBuf::new(&ImageSpec::new(4, 4, 3, PixelFormat::F32).unwrap()).unwrap();
    algo::invert(&mut result, &source, None).unwrap();
    close(&pixel(&result, 0, 0), &[0.75, 0.5, 0.0]);
}

#[test]
fn pow_raises_each_channel() {
    let source = flat(4, 4, &[2.0, 3.0, 4.0]);
    let spec = ImageSpec::new(4, 4, 3, PixelFormat::F32).unwrap();

    let mut result = ImageBuf::new(&spec).unwrap();
    algo::pow(&mut result, &source, &[2.0], None).unwrap();
    close(&pixel(&result, 0, 0), &[4.0, 9.0, 16.0]);

    // An empty exponent slice is not "leave it alone": it is an exponent of
    // zero, so every channel becomes one.
    let mut result = ImageBuf::new(&spec).unwrap();
    algo::pow(&mut result, &source, &[], None).unwrap();
    close(&pixel(&result, 0, 0), &[1.0, 1.0, 1.0]);
}

#[test]
fn clamp_holds_each_channel_in_range() {
    let source = flat(4, 4, &[-1.0, 0.5, 2.0]);
    let spec = ImageSpec::new(4, 4, 3, PixelFormat::F32).unwrap();

    let mut result = ImageBuf::new(&spec).unwrap();
    algo::clamp(&mut result, &source, &[0.0], &[1.0], false, None).unwrap();
    close(&pixel(&result, 0, 0), &[0.0, 0.5, 1.0]);

    // Here an empty slice means "no bound on this side", the opposite of the
    // rule pow follows.
    let mut result = ImageBuf::new(&spec).unwrap();
    algo::clamp(&mut result, &source, &[0.0], &[], false, None).unwrap();
    close(&pixel(&result, 0, 0), &[0.0, 0.5, 2.0]);

    let mut result = ImageBuf::new(&spec).unwrap();
    algo::clamp(&mut result, &source, &[], &[1.0], false, None).unwrap();
    close(&pixel(&result, 0, 0), &[-1.0, 0.5, 1.0]);
}

#[test]
fn min_and_max_against_an_image_and_a_constant() {
    let a = flat(4, 4, &[1.0, 5.0, 3.0]);
    let b = flat(4, 4, &[4.0, 2.0, 3.0]);
    let spec = ImageSpec::new(4, 4, 3, PixelFormat::F32).unwrap();

    let mut result = ImageBuf::new(&spec).unwrap();
    algo::min(&mut result, &a, Operand::Image(&b), None).unwrap();
    close(&pixel(&result, 0, 0), &[1.0, 2.0, 3.0]);

    let mut result = ImageBuf::new(&spec).unwrap();
    algo::max(&mut result, &a, Operand::Image(&b), None).unwrap();
    close(&pixel(&result, 0, 0), &[4.0, 5.0, 3.0]);

    let mut result = ImageBuf::new(&spec).unwrap();
    algo::min(&mut result, &a, Operand::Constant(&[2.5]), None).unwrap();
    close(&pixel(&result, 0, 0), &[1.0, 2.5, 2.5]);

    let mut result = ImageBuf::new(&spec).unwrap();
    algo::max(&mut result, &a, Operand::Constant(&[2.5]), None).unwrap();
    close(&pixel(&result, 0, 0), &[2.5, 5.0, 3.0]);
}

/// `max` refuses two shapes that OpenImageIO would read or write out of
/// bounds for, and `min` — whose implementation is the correct mirror —
/// accepts the first of them.
///
/// OpenImageIO's image-against-image `max` widens its channel range to the
/// larger input's channel count, after the range has already been clamped to
/// what the buffers hold, where `min` narrows it to the smaller. The kernel
/// then indexes past the shorter input, and past the destination when the
/// destination is narrower still. Nothing bounds-checks it.
#[test]
fn max_refuses_the_channel_counts_openimageio_would_run_off_the_end_of() {
    let three = flat(4, 4, &[1.0, 2.0, 3.0]);
    let four = flat(4, 4, &[9.0, 9.0, 9.0, 9.0]);

    // Unequal inputs: max would read past the three-channel image.
    let mut result = ImageBuf::empty().unwrap();
    let error = algo::max(&mut result, &three, Operand::Image(&four), None).unwrap_err();
    println!("mismatched channels rejected as: {error}");
    assert!(
        error.to_string().contains("same number of channels"),
        "unexpected error {error}"
    );

    // min handles exactly this case correctly, and copies the surplus channel
    // through, so the restriction is max's alone.
    let mut result = ImageBuf::empty().unwrap();
    algo::min(&mut result, &three, Operand::Image(&four), None).unwrap();
    close(&pixel(&result, 0, 0), &[1.0, 2.0, 3.0, 9.0]);

    // A destination narrower than the inputs: max would write past it.
    let narrow = ImageSpec::new(4, 4, 2, PixelFormat::F32).unwrap();
    let mut result = ImageBuf::new(&narrow).unwrap();
    let other = flat(4, 4, &[0.0, 0.0, 0.0]);
    let error = algo::max(&mut result, &three, Operand::Image(&other), None).unwrap_err();
    println!("narrow destination rejected as: {error}");
    assert!(
        error.to_string().contains("destination"),
        "unexpected error {error}"
    );
}

#[test]
fn contrast_remap_rescales_levels() {
    let source = flat(4, 4, &[0.0, 0.5, 1.0]);
    let spec = ImageSpec::new(4, 4, 3, PixelFormat::F32).unwrap();

    // Defaults are the identity: black 0, white 1, min 0, max 1, no sigmoid.
    let mut result = ImageBuf::new(&spec).unwrap();
    algo::contrast_remap(&mut result, &source, &ContrastRemap::default(), None).unwrap();
    close(&pixel(&result, 0, 0), &[0.0, 0.5, 1.0]);

    // Map the input range 0..1 onto the output range 0.25..0.75.
    let mut result = ImageBuf::new(&spec).unwrap();
    algo::contrast_remap(
        &mut result,
        &source,
        &ContrastRemap {
            min: &[0.25],
            max: &[0.75],
            ..ContrastRemap::default()
        },
        None,
    )
    .unwrap();
    close(&pixel(&result, 0, 0), &[0.25, 0.5, 0.75]);

    // Pulling black and white inward stretches what is between them, and the
    // result is deliberately not clamped.
    let mut result = ImageBuf::new(&spec).unwrap();
    algo::contrast_remap(
        &mut result,
        &source,
        &ContrastRemap {
            black: &[0.25],
            white: &[0.75],
            ..ContrastRemap::default()
        },
        None,
    )
    .unwrap();
    let values = pixel(&result, 0, 0);
    close(&values, &[-0.5, 0.5, 1.5]);
}

#[test]
fn saturate_moves_colour_toward_luminance() {
    let source = flat(4, 4, &[1.0, 0.0, 0.0]);
    let spec = ImageSpec::new(4, 4, 3, PixelFormat::F32).unwrap();

    // Fully desaturated: every channel becomes the luminance, which for pure
    // red under linear sRGB weights is 0.2126.
    let mut result = ImageBuf::new(&spec).unwrap();
    algo::saturate(&mut result, &source, 0.0, 0, None).unwrap();
    close(&pixel(&result, 0, 0), &[0.2126, 0.2126, 0.2126]);

    // A scale of one leaves the image alone.
    let mut result = ImageBuf::new(&spec).unwrap();
    algo::saturate(&mut result, &source, 1.0, 0, None).unwrap();
    close(&pixel(&result, 0, 0), &[1.0, 0.0, 0.0]);
}

/// Fewer than three channels at `first_channel` is an error rather than a
/// partial result.
#[test]
fn saturate_needs_three_channels() {
    let source = flat(4, 4, &[1.0, 0.0]);
    let mut result = ImageBuf::empty().unwrap();
    let error = algo::saturate(&mut result, &source, 0.5, 0, None).unwrap_err();
    println!("two channels rejected as: {error}");
}

#[test]
fn paste_places_a_source_region_by_offset() {
    let mut destination = flat(8, 8, &[0.0, 0.0, 0.0]);
    let patch = flat(2, 2, &[1.0, 1.0, 1.0]);

    algo::paste(&mut destination, [3, 4, 0], 0, &patch, None).unwrap();

    close(&pixel(&destination, 3, 4), &[1.0, 1.0, 1.0]);
    close(&pixel(&destination, 4, 5), &[1.0, 1.0, 1.0]);
    // Just outside the pasted rectangle.
    close(&pixel(&destination, 5, 5), &[0.0, 0.0, 0.0]);
    close(&pixel(&destination, 2, 4), &[0.0, 0.0, 0.0]);
}

/// The region selects part of the *source*, which is the opposite of every
/// other region argument in this module.
#[test]
fn pastes_region_is_a_source_region() {
    let mut destination = flat(8, 8, &[0.0, 0.0, 0.0]);

    // A patch whose left half is 1 and right half is 2.
    let mut patch = flat(4, 2, &[1.0, 1.0, 1.0]);
    let right = Roi::new(2..4, 0..2, 0..1, 0..3).unwrap();
    algo::fill(&mut patch, &[2.0, 2.0, 2.0], Some(right)).unwrap();

    // Take only the right half of the source.
    let source_region = Roi::new(2..4, 0..2, 0..1, 0..3).unwrap();
    algo::paste(&mut destination, [0, 0, 0], 0, &patch, Some(source_region)).unwrap();

    // The offset is applied to the source's own coordinates, so source x=2
    // lands at destination x=2, not at x=0.
    close(&pixel(&destination, 2, 0), &[2.0, 2.0, 2.0]);
    close(&pixel(&destination, 3, 0), &[2.0, 2.0, 2.0]);
    // Nothing was taken from the left half.
    close(&pixel(&destination, 0, 0), &[0.0, 0.0, 0.0]);
    close(&pixel(&destination, 1, 0), &[0.0, 0.0, 0.0]);
}

#[test]
fn cut_moves_the_region_to_the_origin() {
    let mut source = flat(8, 8, &[0.0, 0.0, 0.0]);
    let marked = Roi::new(4..6, 4..6, 0..1, 0..3).unwrap();
    algo::fill(&mut source, &[1.0, 1.0, 1.0], Some(marked)).unwrap();

    let mut result = ImageBuf::empty().unwrap();
    algo::cut(&mut result, &source, Some(marked)).unwrap();

    let spec = result.spec().unwrap();
    assert_eq!(spec.dimensions(), [2, 2, 1]);
    assert_eq!(
        spec.origin(),
        [0, 0, 0],
        "cut should move the region to the origin"
    );
    assert_eq!(
        spec.full_origin(),
        [0, 0, 0],
        "and the display window should cover exactly it"
    );
    assert_eq!(spec.full_dimensions(), [2, 2, 1]);

    close(&pixel(&result, 0, 0), &[1.0, 1.0, 1.0]);

    // crop keeps the coordinates where they were; that is the difference.
    let mut cropped = ImageBuf::empty().unwrap();
    algo::crop(&mut cropped, &source, Some(marked)).unwrap();
    assert_eq!(cropped.spec().unwrap().origin(), [4, 4, 0]);
}
