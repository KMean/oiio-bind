//! Deep images, where each pixel holds a list of samples.
//!
//! Real deep files are large and not vendored, so the corpus tests here are
//! opt-in through `OIIO_BIND_TEST_EXR_IMAGES`, as in `corpus_test`.

mod common;

use common::{f32_ramp, write_image, ScratchDir};
use oiio::{DeepImage, Error, ImageInput, ImageSpec, PixelFormat};
use std::path::PathBuf;

fn exr_corpus() -> Option<PathBuf> {
    let path = PathBuf::from(std::env::var_os("OIIO_BIND_TEST_EXR_IMAGES")?);
    path.is_dir().then_some(path)
}

/// Find a deep EXR in the corpus, if one is available.
fn a_deep_file() -> Option<PathBuf> {
    let root = exr_corpus()?;
    for relative in [
        "v2/Stereo/Balls.exr",
        "v2/LeftView/Balls.exr",
        "v2/LowResLeftView/Balls.exr",
        "v2/Stereo/Ground.exr",
    ] {
        let path = root.join(relative);
        if path.is_file() {
            return Some(path);
        }
    }
    None
}

#[test]
fn reads_a_deep_image_and_its_samples() {
    let Some(path) = a_deep_file() else {
        eprintln!("skipping: set OIIO_BIND_TEST_EXR_IMAGES to a corpus with deep files");
        return;
    };

    let mut input = ImageInput::from_path(&path).unwrap();
    let spec = input.image_spec().unwrap();
    assert!(spec.is_deep(), "{} should be deep", path.display());

    // The contiguous API must refuse it rather than return nonsense.
    let mut flat = vec![0.0_f32; spec.element_count().unwrap()];
    assert!(matches!(
        input.read_image_into(&mut flat),
        Err(Error::UnsupportedDeepImage)
    ));

    let deep = input.read_deep_image().unwrap();
    println!(
        "{}: {:?}, {} channels, {} pixels",
        path.file_name().unwrap().to_string_lossy(),
        deep.dimensions(),
        deep.channel_count(),
        deep.pixel_count()
    );
    for channel in deep.channels() {
        println!("  channel {} ({})", channel.name(), channel.format());
    }

    assert_eq!(deep.dimensions(), spec.dimensions());
    assert_eq!(deep.channel_count(), spec.channel_count() as usize);
    assert_eq!(
        deep.pixel_count(),
        u64::from(spec.dimensions()[0]) * u64::from(spec.dimensions()[1])
    );

    // A deep render carries depth, which is the whole point of the format.
    let depth = deep
        .z_channel()
        .expect("a deep image from a renderer should have Z");
    assert_eq!(deep.channels()[depth].name(), "Z");

    // Walk the image for a pixel that actually has samples, then read them.
    let origin = deep.origin();
    let [width, height, _] = deep.dimensions();
    let mut total_samples = 0u64;
    let mut deepest = 0usize;
    let mut sampled: Option<(i32, i32)> = None;

    for y in origin[1]..origin[1] + height as i32 {
        for x in origin[0]..origin[0] + width as i32 {
            let count = deep.sample_count(x, y).unwrap();
            total_samples += count as u64;
            if count > deepest {
                deepest = count;
                sampled = Some((x, y));
            }
        }
    }

    println!("  {total_samples} samples in total, deepest pixel holds {deepest}");
    assert!(
        total_samples > 0,
        "a deep image with no samples is not deep"
    );

    let (x, y) = sampled.expect("some pixel should hold samples");
    let depths = deep.samples(x, y, depth).unwrap();
    assert_eq!(depths.len(), deepest);
    assert!(
        depths.iter().all(|value| value.is_finite()),
        "depths should be finite, got {depths:?}"
    );
    println!(
        "  deepest pixel at ({x}, {y}), first depths {:?}",
        &depths[..depths.len().min(4)]
    );

    input.close().unwrap();
}

#[test]
fn deep_accessors_are_bounds_checked() {
    let Some(path) = a_deep_file() else {
        eprintln!("skipping: set OIIO_BIND_TEST_EXR_IMAGES");
        return;
    };

    let mut input = ImageInput::from_path(&path).unwrap();
    let deep = input.read_deep_image().unwrap();
    let origin = deep.origin();
    let [width, height, _] = deep.dimensions();

    // Outside the data window on either axis.
    assert!(matches!(
        deep.sample_count(origin[0] - 1, origin[1]),
        Err(Error::InvalidRegion { axis: "x", .. })
    ));
    assert!(matches!(
        deep.sample_count(origin[0], origin[1] + height as i32),
        Err(Error::InvalidRegion { axis: "y", .. })
    ));
    assert!(matches!(
        deep.value(origin[0] + width as i32, origin[1], 0, 0),
        Err(Error::InvalidRegion { axis: "x", .. })
    ));

    // A channel the image does not have.
    assert!(matches!(
        deep.value(origin[0], origin[1], deep.channel_count(), 0),
        Err(Error::InvalidRoi(_))
    ));

    // A sample index beyond what the pixel holds must be refused, and the
    // last valid one accepted. Deep images are sparse, so the pixel to test
    // this on has to be searched for rather than assumed to be near a corner.
    let mut populated = None;
    'outer: for y in origin[1]..origin[1] + height as i32 {
        for x in origin[0]..origin[0] + width as i32 {
            if deep.sample_count(x, y).unwrap() > 0 {
                populated = Some((x, y));
                break 'outer;
            }
        }
    }
    let (x, y) = populated.expect("expected some pixel with at least one sample");
    let count = deep.sample_count(x, y).unwrap();

    assert!(deep.value(x, y, 0, count - 1).is_ok(), "last sample");
    assert!(matches!(
        deep.value(x, y, 0, count),
        Err(Error::InvalidRoi(_))
    ));

    // An empty pixel has no valid sample at all.
    let mut empty = None;
    'search: for y in origin[1]..origin[1] + height as i32 {
        for x in origin[0]..origin[0] + width as i32 {
            if deep.sample_count(x, y).unwrap() == 0 {
                empty = Some((x, y));
                break 'search;
            }
        }
    }
    if let Some((x, y)) = empty {
        assert!(matches!(deep.value(x, y, 0, 0), Err(Error::InvalidRoi(_))));
    }
}

/// Build a deep image sample by sample, write it, read it back, and check
/// every sample survived.
#[test]
fn round_trips_a_deep_image_written_from_scratch() {
    let scratch = ScratchDir::new("deepwrite");
    let path = scratch.file("written.exr");

    const WIDTH: u32 = 8;
    const HEIGHT: u32 = 4;
    let spec = ImageSpec::new(WIDTH, HEIGHT, 5, PixelFormat::F32)
        .unwrap()
        .with_channel_names(["R", "G", "B", "A", "Z"])
        .unwrap()
        .as_deep();
    assert!(spec.is_deep());

    // A pixel at (x, y) gets (x % 3) samples, so the image is sparse in the
    // way a real deep render is, with empty pixels among populated ones.
    let sample_count = |x: i32| (x % 3) as usize;
    let expected = |x: i32, y: i32, channel: usize, sample: usize| -> f32 {
        (x as f32) + (y as f32) * 0.5 + (channel as f32) * 0.25 + (sample as f32) * 0.125
    };

    let mut deep = DeepImage::new(&spec).unwrap();
    for y in 0..HEIGHT as i32 {
        for x in 0..WIDTH as i32 {
            let count = sample_count(x);
            deep.set_sample_count(x, y, count).unwrap();
            for sample in 0..count {
                for channel in 0..5 {
                    deep.set_value(x, y, channel, sample, expected(x, y, channel, sample))
                        .unwrap();
                }
            }
        }
    }

    let mut output = oiio::ImageOutput::create(&path, &spec).unwrap();
    output.write_deep_image(&deep).unwrap();
    output.close().unwrap();
    assert!(path.exists());

    // Read it back through a fresh reader.
    let mut input = ImageInput::from_path(&path).unwrap();
    let read_spec = input.image_spec().unwrap();
    assert!(read_spec.is_deep(), "the file should be deep");
    assert_eq!(read_spec.dimensions(), [WIDTH, HEIGHT, 1]);
    assert_eq!(read_spec.channel_names(), ["R", "G", "B", "A", "Z"]);

    let read_back = input.read_deep_image().unwrap();
    let mut checked = 0usize;
    for y in 0..HEIGHT as i32 {
        for x in 0..WIDTH as i32 {
            let count = read_back.sample_count(x, y).unwrap();
            assert_eq!(count, sample_count(x), "sample count at ({x}, {y})");
            for sample in 0..count {
                for channel in 0..5 {
                    let value = read_back.value(x, y, channel, sample).unwrap();
                    let wanted = expected(x, y, channel, sample);
                    assert!(
                        (value - wanted).abs() < 1e-5,
                        "({x}, {y}) channel {channel} sample {sample}: {value} != {wanted}"
                    );
                    checked += 1;
                }
            }
        }
    }
    input.close().unwrap();

    println!("{checked} deep sample values round-tripped");
    assert!(checked > 0, "the fixture wrote no samples");
}

#[test]
fn deep_writing_is_refused_when_the_writer_disagrees() {
    let scratch = ScratchDir::new("deepmismatch");

    let deep_spec = ImageSpec::new(8, 4, 5, PixelFormat::F32)
        .unwrap()
        .with_channel_names(["R", "G", "B", "A", "Z"])
        .unwrap()
        .as_deep();
    let deep = DeepImage::new(&deep_spec).unwrap();

    // A writer opened for flat pixels cannot take a deep image.
    let flat_path = scratch.file("flat.exr");
    let flat_spec = ImageSpec::new(8, 4, 5, PixelFormat::F32).unwrap();
    let mut flat = oiio::ImageOutput::create(&flat_path, &flat_spec).unwrap();
    let error = flat.write_deep_image(&deep).unwrap_err();
    assert!(
        error.to_string().contains("flat pixels"),
        "unexpected error: {error}"
    );

    // Nor can a deep writer take an image of another size.
    let other_path = scratch.file("other.exr");
    let other_spec = ImageSpec::new(16, 4, 5, PixelFormat::F32)
        .unwrap()
        .with_channel_names(["R", "G", "B", "A", "Z"])
        .unwrap()
        .as_deep();
    let mut other = oiio::ImageOutput::create(&other_path, &other_spec).unwrap();
    assert!(matches!(
        other.write_deep_image(&deep),
        Err(Error::InvalidImageSpec(_))
    ));
}

/// What `set_sample_count` documents: shrinking keeps the room the dropped
/// samples occupied, and regrowing within it brings their old values back
/// rather than zeroes. Pinned so the documentation stays honest about
/// OpenImageIO leaving holes for speed.
#[test]
fn regrown_samples_return_their_old_values_not_zeroes() {
    let spec = ImageSpec::new(1, 1, 1, PixelFormat::F32)
        .unwrap()
        .with_channel_names(["Z"])
        .unwrap()
        .as_deep();
    let mut deep = DeepImage::new(&spec).unwrap();

    deep.set_sample_count(0, 0, 2).unwrap();
    deep.set_value(0, 0, 0, 0, 1.0).unwrap();
    deep.set_value(0, 0, 0, 1, 7.0).unwrap();

    deep.set_sample_count(0, 0, 1).unwrap();
    assert_eq!(deep.sample_count(0, 0).unwrap(), 1);

    deep.set_sample_count(0, 0, 2).unwrap();
    assert_eq!(
        deep.value(0, 0, 0, 1).unwrap(),
        7.0,
        "the regrown sample returns its old value, as documented"
    );
    assert_eq!(deep.value(0, 0, 0, 0).unwrap(), 1.0);
}

#[test]
fn a_deep_image_needs_a_deep_specification() {
    let flat = ImageSpec::new(4, 4, 3, PixelFormat::F32).unwrap();
    assert!(matches!(
        DeepImage::new(&flat),
        Err(Error::InvalidImageSpec(_))
    ));
}

#[test]
fn a_flat_image_is_not_read_as_deep() {
    let scratch = ScratchDir::new("notdeep");
    let path = scratch.file("flat.exr");

    let spec = ImageSpec::new(4, 4, 3, PixelFormat::F32).unwrap();
    write_image(&path, &spec, &f32_ramp(spec.element_count().unwrap())).unwrap();

    let mut input = ImageInput::from_path(&path).unwrap();
    assert!(!input.image_spec().unwrap().is_deep());

    // Asking for deep data from a flat image is a mistake worth reporting,
    // not something to answer with an empty result.
    let error = input.read_deep_image().unwrap_err();
    assert!(
        error.to_string().contains("not deep"),
        "unexpected error: {error}"
    );
}
