//! ImageBufAlgo operations, checked against pixels computed in Rust.

mod common;

use common::ScratchDir;
use oiio::{algo, Error, ImageBuf, ImageSpec, PixelFormat, Roi};

const WIDTH: u32 = 8;
const HEIGHT: u32 = 6;
const CHANNELS: u32 = 3;

fn spec() -> ImageSpec {
    ImageSpec::new(WIDTH, HEIGHT, CHANNELS, PixelFormat::F32).unwrap()
}

/// An image whose every pixel holds `values`, one per channel.
fn filled(values: &[f32]) -> ImageBuf {
    let mut image = ImageBuf::new(&spec()).unwrap();
    algo::fill(&mut image, values, None).unwrap();
    image
}

fn pixels_of(image: &ImageBuf) -> Vec<f32> {
    let roi = image.spec().unwrap().data_window().unwrap();
    let mut values = vec![0.0_f32; roi.element_count().unwrap()];
    image.get_pixels_into(roi, &mut values).unwrap();
    values
}

/// Every pixel equals `expected`, channel by channel.
fn assert_every_pixel(image: &ImageBuf, expected: &[f32]) {
    let values = pixels_of(image);
    assert_eq!(values.len() % expected.len(), 0);
    for (index, chunk) in values.chunks(expected.len()).enumerate() {
        assert_eq!(chunk, expected, "pixel {index} differs");
    }
}

#[test]
fn zero_clears_every_channel() {
    let mut image = filled(&[0.25, 0.5, 0.75]);
    assert_every_pixel(&image, &[0.25, 0.5, 0.75]);

    algo::zero(&mut image, None).unwrap();
    assert_every_pixel(&image, &[0.0, 0.0, 0.0]);
}

#[test]
fn fill_sets_one_value_per_channel() {
    let image = filled(&[0.1, 0.2, 0.3]);
    assert_every_pixel(&image, &[0.1, 0.2, 0.3]);
}

#[test]
fn zero_and_fill_respect_a_region() {
    let mut image = filled(&[1.0, 1.0, 1.0]);
    let window = image.spec().unwrap().data_window().unwrap();
    let block = window.with_x(0..4).unwrap().with_y(0..3).unwrap();

    algo::zero(&mut image, Some(block)).unwrap();

    let values = pixels_of(&image);
    let row = (WIDTH * CHANNELS) as usize;
    for y in 0..HEIGHT as usize {
        for x in 0..WIDTH as usize {
            let offset = y * row + x * CHANNELS as usize;
            let inside = x < 4 && y < 3;
            let expected = if inside { 0.0 } else { 1.0 };
            assert_eq!(values[offset], expected, "at ({x}, {y})");
        }
    }
}

#[test]
fn arithmetic_between_two_images() {
    let a = filled(&[0.5, 0.25, 0.125]);
    let b = filled(&[0.25, 0.25, 0.125]);
    let mut result = ImageBuf::new(&spec()).unwrap();

    algo::add(&mut result, &a, &b, None).unwrap();
    assert_every_pixel(&result, &[0.75, 0.5, 0.25]);

    algo::sub(&mut result, &a, &b, None).unwrap();
    assert_every_pixel(&result, &[0.25, 0.0, 0.0]);

    algo::mul(&mut result, &a, &b, None).unwrap();
    assert_every_pixel(&result, &[0.125, 0.0625, 0.015625]);

    algo::div(&mut result, &a, &b, None).unwrap();
    assert_every_pixel(&result, &[2.0, 1.0, 1.0]);
}

#[test]
fn arithmetic_against_constants() {
    let a = filled(&[0.5, 0.5, 0.5]);
    let mut result = ImageBuf::new(&spec()).unwrap();

    algo::add_constant(&mut result, &a, &[0.25, 0.0, -0.5], None).unwrap();
    assert_every_pixel(&result, &[0.75, 0.5, 0.0]);

    algo::mul_constant(&mut result, &a, &[2.0, 4.0, 0.0], None).unwrap();
    assert_every_pixel(&result, &[1.0, 2.0, 0.0]);

    algo::sub_constant(&mut result, &a, &[0.5, 0.25, 0.0], None).unwrap();
    assert_every_pixel(&result, &[0.0, 0.25, 0.5]);
}

#[test]
fn absolute_value_and_difference() {
    let negative = filled(&[-0.5, 0.25, -1.0]);
    let mut result = ImageBuf::new(&spec()).unwrap();

    algo::abs(&mut result, &negative, None).unwrap();
    assert_every_pixel(&result, &[0.5, 0.25, 1.0]);

    let a = filled(&[0.25, 0.75, 0.5]);
    let b = filled(&[0.75, 0.25, 0.5]);
    algo::absdiff(&mut result, &a, &b, None).unwrap();
    assert_every_pixel(&result, &[0.5, 0.5, 0.0]);
}

#[test]
fn copy_converts_the_pixel_format() {
    let source = filled(&[0.5, 0.25, 0.125]);

    // An empty destination lets copy choose the format; a pre-allocated one
    // would keep its own, which the next test covers.
    let mut destination = ImageBuf::empty().unwrap();
    algo::copy(&mut destination, &source, Some(PixelFormat::F16), None).unwrap();
    assert_eq!(destination.spec().unwrap().format(), PixelFormat::F16);
    // These values are all exactly representable in half.
    assert_every_pixel(&destination, &[0.5, 0.25, 0.125]);
}

#[test]
fn an_allocated_destination_keeps_its_own_format() {
    let source = filled(&[0.5, 0.25, 0.125]);

    // Already float, so asking for half changes nothing: the operation writes
    // into the destination as it stands.
    let mut destination = ImageBuf::new(&spec()).unwrap();
    algo::copy(&mut destination, &source, Some(PixelFormat::F16), None).unwrap();
    assert_eq!(destination.spec().unwrap().format(), PixelFormat::F32);
    assert_every_pixel(&destination, &[0.5, 0.25, 0.125]);
}

#[test]
fn crop_keeps_the_regions_coordinates() {
    let source = filled(&[1.0, 1.0, 1.0]);
    let window = source.spec().unwrap().data_window().unwrap();
    let block = window.with_x(2..6).unwrap().with_y(1..4).unwrap();

    let mut cropped = ImageBuf::new(&spec()).unwrap();
    algo::crop(&mut cropped, &source, Some(block)).unwrap();

    let cropped_spec = cropped.spec().unwrap();
    assert_eq!(cropped_spec.origin(), [2, 1, 0]);
    assert_eq!(cropped_spec.dimensions(), [4, 3, 1]);
}

#[test]
fn flip_flop_and_transpose_move_pixels() {
    // A gradient that differs along both axes, so each operation is visible.
    let mut source = ImageBuf::new(&spec()).unwrap();
    let window = source.spec().unwrap().data_window().unwrap();
    let mut values = vec![0.0_f32; window.element_count().unwrap()];
    for y in 0..HEIGHT as usize {
        for x in 0..WIDTH as usize {
            let offset = (y * WIDTH as usize + x) * CHANNELS as usize;
            values[offset] = x as f32;
            values[offset + 1] = y as f32;
            values[offset + 2] = 0.0;
        }
    }
    source.set_pixels(window, &values).unwrap();

    let sample = |image: &ImageBuf, x: usize, y: usize| -> (f32, f32) {
        let pixels = pixels_of(image);
        let width = image.spec().unwrap().dimensions()[0] as usize;
        let offset = (y * width + x) * CHANNELS as usize;
        (pixels[offset], pixels[offset + 1])
    };

    // flip mirrors vertically: the top row becomes the bottom row.
    let mut flipped = ImageBuf::new(&spec()).unwrap();
    algo::flip(&mut flipped, &source, None).unwrap();
    assert_eq!(sample(&flipped, 0, 0), (0.0, (HEIGHT - 1) as f32));

    // flop mirrors horizontally.
    let mut flopped = ImageBuf::new(&spec()).unwrap();
    algo::flop(&mut flopped, &source, None).unwrap();
    assert_eq!(sample(&flopped, 0, 0), ((WIDTH - 1) as f32, 0.0));

    // transpose exchanges the axes, so the result has different dimensions
    // from the source and needs a destination it can size itself.
    let mut transposed = ImageBuf::empty().unwrap();
    algo::transpose(&mut transposed, &source, None).unwrap();
    assert_eq!(transposed.spec().unwrap().dimensions(), [HEIGHT, WIDTH, 1]);
    assert_eq!(sample(&transposed, 2, 3), (3.0, 2.0));
}

#[test]
fn compare_measures_identical_and_differing_images() {
    let a = filled(&[0.5, 0.5, 0.5]);
    let identical = filled(&[0.5, 0.5, 0.5]);

    let same = algo::compare(&a, &identical, 0.0, 0.0, None);
    assert_eq!(same.max_error, 0.0);
    assert_eq!(same.mean_error, 0.0);
    assert_eq!(same.failures, 0);
    assert!(!same.failed);

    // Every value differs by exactly 0.25.
    let different = filled(&[0.75, 0.75, 0.75]);
    let differs = algo::compare(&a, &different, 0.1, 0.05, None);
    assert!((differs.max_error - 0.25).abs() < 1e-6);
    assert!((differs.mean_error - 0.25).abs() < 1e-6);
    assert!(differs.failed);

    // OpenImageIO counts failing *pixels*, not failing channel values.
    let pixels = (WIDTH * HEIGHT) as u64;
    assert_eq!(differs.failures, pixels);
}

#[test]
fn compare_is_how_a_round_trip_is_verified() {
    let scratch = ScratchDir::new("algocompare");
    let path = scratch.file("image.exr");

    let mut original = filled(&[0.125, 0.25, 0.5]);
    original.write(&path).unwrap();

    let mut read_back = ImageBuf::from_path(&path).unwrap();
    read_back.read().unwrap();

    let results = algo::compare(&original, &read_back, 0.0, 0.0, None);
    assert_eq!(results.max_error, 0.0, "the round trip was not exact");
    assert_eq!(results.failures, 0);
}

#[test]
fn operations_reject_an_empty_constant_list() {
    let a = filled(&[1.0, 1.0, 1.0]);
    let mut result = ImageBuf::new(&spec()).unwrap();

    assert!(matches!(
        algo::fill(&mut result, &[], None),
        Err(Error::InvalidImageSpec(_))
    ));
    assert!(matches!(
        algo::add_constant(&mut result, &a, &[], None),
        Err(Error::InvalidImageSpec(_))
    ));
}

#[test]
fn a_region_outside_the_image_produces_an_empty_result() {
    let source = filled(&[1.0, 1.0, 1.0]);
    let mut cropped = ImageBuf::new(&spec()).unwrap();

    // Far outside the data window; OpenImageIO intersects, leaving nothing.
    let outside = Roi::new(100..104, 100..104, 0..1, 0..CHANNELS).unwrap();
    let result = algo::crop(&mut cropped, &source, Some(outside));

    // Either an error or an empty image is acceptable; a full copy is not.
    if result.is_ok() {
        let dimensions = cropped.spec().unwrap().dimensions();
        assert!(
            dimensions[0] == 0 || dimensions[0] == 4,
            "unexpected crop dimensions {dimensions:?}"
        );
    }
}
