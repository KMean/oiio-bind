//! Deep compositing, and the `ImageBuf` sample access it needs.

mod common;

use oiio::{algo, ImageBuf, ImageSpec, PixelFormat, Roi};

/// A deep image with `samples` samples in every pixel, each a little further
/// away and a little less opaque than the last.
///
/// Channels are R, G, B, A, Z, which is what the deep operations look for by
/// name.
fn deep_image(width: u32, height: u32, samples: u32) -> ImageBuf {
    let spec = ImageSpec::new(width, height, 5, PixelFormat::F32)
        .unwrap()
        .with_channel_names(["R", "G", "B", "A", "Z"])
        .unwrap()
        .as_deep();
    let mut image = ImageBuf::new(&spec).unwrap();

    for y in 0..height as i32 {
        for x in 0..width as i32 {
            image.set_deep_sample_count(x, y, samples).unwrap();
            for s in 0..samples {
                let depth = 1.0 + s as f32;
                image.set_deep_value(x, y, 0, s, 0.5).unwrap();
                image.set_deep_value(x, y, 1, s, 0.25).unwrap();
                image.set_deep_value(x, y, 2, s, 0.125).unwrap();
                image.set_deep_value(x, y, 3, s, 0.5).unwrap();
                image.set_deep_value(x, y, 4, s, depth).unwrap();
            }
        }
    }
    image
}

fn flat_pixel(image: &ImageBuf, x: i32, y: i32) -> Vec<f32> {
    let channels = image.spec().unwrap().channel_count();
    let roi = Roi::new(x..x + 1, y..y + 1, 0..1, 0..channels).unwrap();
    let mut values = vec![0.0_f32; channels as usize];
    image.get_pixels_into(roi, &mut values).unwrap();
    values
}

#[test]
fn an_image_buffer_can_hold_and_report_deep_samples() {
    let image = deep_image(4, 4, 3);

    assert!(image.is_deep());
    assert_eq!(image.deep_sample_count(0, 0), 3);
    assert_eq!(image.deep_value(0, 0, 4, 0).unwrap(), 1.0);
    assert_eq!(image.deep_value(0, 0, 4, 2).unwrap(), 3.0);
    assert_eq!(image.deep_value(2, 3, 3, 1).unwrap(), 0.5);

    // A pixel nobody wrote to holds nothing.
    let empty =
        ImageBuf::new(&ImageSpec::new(4, 4, 5, PixelFormat::F32).unwrap().as_deep()).unwrap();
    assert_eq!(empty.deep_sample_count(0, 0), 0);
}

/// OpenImageIO answers an out-of-range index with a null pointer and then
/// reads zero or drops the write, saying nothing either way.
#[test]
fn deep_indices_are_checked() {
    let mut image = deep_image(4, 4, 2);

    assert!(
        image.deep_value(0, 0, 9, 0).is_err(),
        "channel out of range"
    );
    assert!(image.deep_value(0, 0, 0, 5).is_err(), "sample out of range");
    assert!(image.set_deep_value(0, 0, 0, 5, 1.0).is_err());

    let error = image.deep_value(0, 0, 0, 5).unwrap_err();
    println!("out-of-range sample reported as: {error}");
}

#[test]
fn a_flat_image_refuses_deep_access() {
    let mut flat = ImageBuf::new(&ImageSpec::new(4, 4, 3, PixelFormat::F32).unwrap()).unwrap();
    assert!(!flat.is_deep());
    assert!(flat.set_deep_sample_count(0, 0, 1).is_err());
    assert!(flat.deep_value(0, 0, 0, 0).is_err());
}

#[test]
fn flatten_composites_samples_into_one_pixel() {
    let deep = deep_image(4, 4, 3);
    let mut flat = ImageBuf::empty().unwrap();
    algo::flatten(&mut flat, &deep, None).unwrap();

    assert!(!flat.is_deep(), "the result of flattening is a flat image");
    let pixel = flat_pixel(&flat, 0, 0);
    println!("three samples at alpha 0.5 flatten to: {pixel:?}");

    // Front to back with alpha 0.5 each: the first contributes fully, the
    // second at half, the third at a quarter. Alpha accumulates towards one.
    assert!(
        pixel[3] > 0.5 && pixel[3] < 1.0,
        "alpha should accumulate above one sample's but stay under one, got {}",
        pixel[3]
    );
    assert!(
        pixel[0] > 0.5,
        "the colour should accumulate past a single sample's, got {}",
        pixel[0]
    );
}

/// The accumulator is sized from the source but indexed up to the wider of the
/// two buffers, so a wider destination reads past the end of it.
#[test]
fn flatten_refuses_a_wider_destination() {
    let deep = deep_image(4, 4, 2);
    let mut wide = ImageBuf::new(&ImageSpec::new(4, 4, 8, PixelFormat::F32).unwrap()).unwrap();

    let error = algo::flatten(&mut wide, &deep, None).unwrap_err();
    println!("wider destination reported as: {error}");
    assert!(error.to_string().contains("channels"), "{error}");
}

#[test]
fn flatten_needs_an_alpha_channel() {
    // R, G, B, Z — no alpha for the compositing to use.
    let spec = ImageSpec::new(4, 4, 4, PixelFormat::F32)
        .unwrap()
        .with_channel_names(["R", "G", "B", "Z"])
        .unwrap()
        .as_deep();
    let mut deep = ImageBuf::new(&spec).unwrap();
    deep.set_deep_sample_count(0, 0, 1).unwrap();

    let mut flat = ImageBuf::empty().unwrap();
    let error = algo::flatten(&mut flat, &deep, None).unwrap_err();
    println!("no alpha reported as: {error}");
}

#[test]
fn deepen_turns_a_flat_image_into_one_sample_per_pixel() {
    let spec = ImageSpec::new(4, 4, 3, PixelFormat::F32)
        .unwrap()
        .with_channel_names(["R", "G", "B"])
        .unwrap();
    let mut flat = ImageBuf::new(&spec).unwrap();
    algo::fill(&mut flat, &[0.5, 0.5, 0.5], None).unwrap();

    let mut deep = ImageBuf::empty().unwrap();
    algo::deepen(&mut deep, &flat, 7.0, None).unwrap();

    assert!(deep.is_deep());
    assert_eq!(deep.deep_sample_count(0, 0), 1);
    assert_eq!(
        deep.spec().unwrap().channel_count(),
        4,
        "a Z channel is appended when the source has none"
    );
    assert_eq!(
        deep.deep_value(0, 0, 3, 0).unwrap(),
        7.0,
        "the depth should be the value asked for"
    );
    assert_eq!(deep.deep_value(0, 0, 0, 0).unwrap(), 0.5);
}

/// A pixel that is zero everywhere gets no sample, which is what makes an
/// empty background stay empty.
#[test]
fn deepen_leaves_black_pixels_empty() {
    let spec = ImageSpec::new(4, 4, 3, PixelFormat::F32)
        .unwrap()
        .with_channel_names(["R", "G", "B"])
        .unwrap();
    let mut flat = ImageBuf::new(&spec).unwrap();
    algo::zero(&mut flat, None).unwrap();
    // One non-black pixel.
    let one = Roi::new(1..2, 1..2, 0..1, 0..3).unwrap();
    algo::fill(&mut flat, &[1.0, 1.0, 1.0], Some(one)).unwrap();

    let mut deep = ImageBuf::empty().unwrap();
    algo::deepen(&mut deep, &flat, 1.0, None).unwrap();

    assert_eq!(
        deep.deep_sample_count(1, 1),
        1,
        "the lit pixel gets a sample"
    );
    assert_eq!(deep.deep_sample_count(0, 0), 0, "the black one does not");
}

/// A pre-allocated destination would keep its own shape and drop the writes
/// that did not fit, without a word.
#[test]
fn deepen_needs_an_empty_destination() {
    let flat = ImageBuf::new(&ImageSpec::new(4, 4, 3, PixelFormat::F32).unwrap()).unwrap();
    let mut already = ImageBuf::new(&ImageSpec::new(4, 4, 3, PixelFormat::F32).unwrap()).unwrap();

    let error = algo::deepen(&mut already, &flat, 1.0, None).unwrap_err();
    println!("pre-allocated destination reported as: {error}");
    assert!(error.to_string().contains("empty"), "{error}");
}

#[test]
fn flatten_and_deepen_round_trip_a_single_sample() {
    let spec = ImageSpec::new(4, 4, 4, PixelFormat::F32)
        .unwrap()
        .with_channel_names(["R", "G", "B", "A"])
        .unwrap();
    let mut flat = ImageBuf::new(&spec).unwrap();
    algo::fill(&mut flat, &[0.4, 0.5, 0.6, 1.0], None).unwrap();

    let mut deep = ImageBuf::empty().unwrap();
    algo::deepen(&mut deep, &flat, 1.0, None).unwrap();

    let mut back = ImageBuf::empty().unwrap();
    algo::flatten(&mut back, &deep, None).unwrap();

    let original = flat_pixel(&flat, 0, 0);
    let returned = flat_pixel(&back, 0, 0);
    println!("{original:?} -> deep -> {returned:?}");
    for channel in 0..4 {
        assert!(
            (original[channel] - returned[channel]).abs() < 1e-4,
            "a single opaque sample should survive the round trip: {original:?} \
             then {returned:?}"
        );
    }
}

#[test]
fn deep_merge_interleaves_two_images_samples() {
    let near = deep_image(2, 2, 1);
    let mut far = deep_image(2, 2, 1);
    // Push the second image's sample behind the first's.
    for y in 0..2 {
        for x in 0..2 {
            far.set_deep_value(x, y, 4, 0, 10.0).unwrap();
        }
    }

    let mut merged = ImageBuf::empty().unwrap();
    algo::deep_merge(&mut merged, &near, &far, false, None).unwrap();

    assert!(merged.is_deep());
    assert_eq!(
        merged.deep_sample_count(0, 0),
        2,
        "one sample from each should survive without culling"
    );

    // Sorted by depth: the nearer one first.
    let first = merged.deep_value(0, 0, 4, 0).unwrap();
    let second = merged.deep_value(0, 0, 4, 1).unwrap();
    println!("merged depths: {first} then {second}");
    assert!(first <= second, "samples should come back sorted by depth");
}

#[test]
fn deep_merge_needs_matching_channels() {
    let five = deep_image(2, 2, 1);
    let spec = ImageSpec::new(2, 2, 4, PixelFormat::F32)
        .unwrap()
        .with_channel_names(["R", "G", "B", "A"])
        .unwrap()
        .as_deep();
    let four = ImageBuf::new(&spec).unwrap();

    let mut merged = ImageBuf::empty().unwrap();
    let error = algo::deep_merge(&mut merged, &five, &four, true, None).unwrap_err();
    println!("mismatched channels reported as: {error}");
}

#[test]
fn deep_holdout_removes_what_is_hidden() {
    // A source with a sample far away.
    let mut source = deep_image(2, 2, 1);
    for y in 0..2 {
        for x in 0..2 {
            source.set_deep_value(x, y, 4, 0, 100.0).unwrap();
        }
    }

    // A holdout that is opaque close to the camera.
    let mut holdout = deep_image(2, 2, 1);
    for y in 0..2 {
        for x in 0..2 {
            holdout.set_deep_value(x, y, 3, 0, 1.0).unwrap(); // fully opaque
            holdout.set_deep_value(x, y, 4, 0, 1.0).unwrap(); // right in front
        }
    }

    let mut held = ImageBuf::empty().unwrap();
    algo::deep_holdout(&mut held, &source, &holdout, None).unwrap();

    assert_eq!(
        held.deep_sample_count(0, 0),
        0,
        "a sample behind an opaque holdout should be culled"
    );

    // With the holdout further away than the source, nothing is hidden.
    let mut behind = deep_image(2, 2, 1);
    for y in 0..2 {
        for x in 0..2 {
            behind.set_deep_value(x, y, 3, 0, 1.0).unwrap();
            behind.set_deep_value(x, y, 4, 0, 1000.0).unwrap();
        }
    }
    let mut kept = ImageBuf::empty().unwrap();
    algo::deep_holdout(&mut kept, &source, &behind, None).unwrap();
    assert_eq!(
        kept.deep_sample_count(0, 0),
        1,
        "a sample in front of the holdout should survive"
    );
}

#[test]
fn the_deep_operations_refuse_flat_images() {
    let flat = ImageBuf::new(&ImageSpec::new(4, 4, 4, PixelFormat::F32).unwrap()).unwrap();
    let deep = deep_image(4, 4, 1);

    let mut result = ImageBuf::empty().unwrap();
    assert!(algo::deep_merge(&mut result, &flat, &deep, true, None).is_err());

    let mut result = ImageBuf::empty().unwrap();
    assert!(algo::deep_holdout(&mut result, &flat, &deep, None).is_err());
}
