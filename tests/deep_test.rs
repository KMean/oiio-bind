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

/// The per-pixel deep operations: sort, merge, opacity, and their stated
/// requirements — a missing role channel is an error here, not OpenImageIO's
/// silent return.
#[test]
fn per_pixel_deep_operations_sort_merge_and_cull() {
    let spec = |names: &[&str]| {
        let mut spec = ImageSpec::new(1, 1, names.len() as u32, PixelFormat::F32)
            .unwrap()
            .with_channel_names(names.to_vec())
            .unwrap()
            .as_deep();
        if let Some(alpha) = names.iter().position(|&n| n == "A") {
            spec = spec.with_alpha_channel(Some(alpha as u32)).unwrap();
        }
        if let Some(z) = names.iter().position(|&n| n == "Z") {
            spec = spec.with_z_channel(Some(z as u32)).unwrap();
        }
        spec
    };

    // Two samples out of depth order.
    let mut deep = DeepImage::new(&spec(&["A", "Z"])).unwrap();
    deep.set_sample_count(0, 0, 2).unwrap();
    deep.set_value(0, 0, 0, 0, 1.0).unwrap(); // opaque...
    deep.set_value(0, 0, 1, 0, 9.0).unwrap(); // ...at depth 9
    deep.set_value(0, 0, 0, 1, 0.5).unwrap(); // half...
    deep.set_value(0, 0, 1, 1, 2.0).unwrap(); // ...at depth 2

    deep.sort_samples(0, 0).unwrap();
    assert_eq!(deep.value(0, 0, 1, 0).unwrap(), 2.0, "front first");

    assert_eq!(
        deep.opaque_depth(0, 0).unwrap(),
        Some(9.0),
        "opacity is reached at the far sample"
    );

    // Culling drops nothing here (opacity is last), then everything behind
    // an opaque front sample once we make one.
    deep.set_value(0, 0, 0, 0, 1.0).unwrap();
    deep.occlusion_cull_samples(0, 0).unwrap();
    assert_eq!(deep.sample_count(0, 0).unwrap(), 1);

    // Merging a pixel from a like image combines and re-sorts.
    let mut other = DeepImage::new(&spec(&["A", "Z"])).unwrap();
    other.set_sample_count(0, 0, 1).unwrap();
    other.set_value(0, 0, 0, 0, 0.25).unwrap();
    other.set_value(0, 0, 1, 0, 1.0).unwrap();
    deep.merge_pixel_from(0, 0, &other, 0, 0).unwrap();
    assert!(deep.sample_count(0, 0).unwrap() >= 2);
    assert_eq!(deep.value(0, 0, 1, 0).unwrap(), 1.0, "nearest after merge");

    // The stated requirements, refused rather than silently skipped.
    let mut no_z = DeepImage::new(&spec(&["A", "B"])).unwrap();
    assert!(no_z.sort_samples(0, 0).is_err());
    assert!(no_z.merge_overlap_samples(0, 0).is_err());
    let mut no_alpha = DeepImage::new(&spec(&["R", "Z"])).unwrap();
    assert!(no_alpha.occlusion_cull_samples(0, 0).is_err());

    // A channel-layout mismatch cannot be merged.
    let mut wide = DeepImage::new(&spec(&["R", "A", "Z"])).unwrap();
    assert!(wide.merge_pixel_from(0, 0, &other, 0, 0).is_err());

    // No samples and no opacity is None, not f32::MAX.
    let empty = DeepImage::new(&spec(&["A", "Z"])).unwrap();
    assert_eq!(empty.opaque_depth(0, 0).unwrap(), None);
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

/// Build a deep image band by band, stream the bands in order, and read the
/// file back both whole and as a band that straddles the two written ones.
#[test]
fn streams_deep_scanline_bands_and_reads_them_back() {
    let scratch = ScratchDir::new("deepstream");
    let path = scratch.file("streamed.exr");

    const WIDTH: u32 = 8;
    const HEIGHT: u32 = 4;
    let spec = ImageSpec::new(WIDTH, HEIGHT, 2, PixelFormat::F32)
        .unwrap()
        .with_channel_names(["A", "Z"])
        .unwrap()
        .as_deep();

    let sample_count = |x: i32, y: i32| ((x + y) % 3) as usize;
    let expected = |x: i32, y: i32, channel: usize, sample: usize| -> f32 {
        (x as f32) + (y as f32) * 0.5 + (channel as f32) * 0.25 + (sample as f32) * 0.125
    };

    // Each band is its own deep image, shaped and placed like the rows it
    // holds.
    let band = |rows: std::ops::Range<i32>| -> oiio::DeepImage {
        let band_spec = ImageSpec::new(WIDTH, (rows.end - rows.start) as u32, 2, PixelFormat::F32)
            .unwrap()
            .with_channel_names(["A", "Z"])
            .unwrap()
            .with_origin([0, rows.start, 0])
            .as_deep();
        let mut deep = DeepImage::new(&band_spec).unwrap();
        for y in rows {
            for x in 0..WIDTH as i32 {
                deep.set_sample_count(x, y, sample_count(x, y)).unwrap();
                for sample in 0..sample_count(x, y) {
                    for channel in 0..2 {
                        deep.set_value(x, y, channel, sample, expected(x, y, channel, sample))
                            .unwrap();
                    }
                }
            }
        }
        deep
    };

    let mut output = oiio::ImageOutput::create(&path, &spec).unwrap();
    output.write_deep_scanlines(0..2, &band(0..2)).unwrap();
    output.write_deep_scanlines(2..4, &band(2..4)).unwrap();
    output.close().unwrap();

    // Whole-image read: every sample of every band survived.
    let mut input = ImageInput::from_path(&path).unwrap();
    let read_back = input.read_deep_image().unwrap();
    let mut checked = 0usize;
    for y in 0..HEIGHT as i32 {
        for x in 0..WIDTH as i32 {
            assert_eq!(read_back.sample_count(x, y).unwrap(), sample_count(x, y));
            for sample in 0..sample_count(x, y) {
                for channel in 0..2 {
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
    assert!(checked > 0, "the fixture wrote no samples");

    // Band read, straddling the two written bands: reads are random access.
    let mut input = ImageInput::from_path(&path).unwrap();
    let middle = input.read_deep_scanlines_at(0, 0, 1..3).unwrap();
    assert_eq!(middle.dimensions(), [WIDTH, 2, 1]);
    assert_eq!(middle.origin(), [0, 1, 0]);
    assert_eq!(middle.channel_count(), 2);
    for y in 1..3 {
        for x in 0..WIDTH as i32 {
            assert_eq!(middle.sample_count(x, y).unwrap(), sample_count(x, y));
            for sample in 0..sample_count(x, y) {
                for channel in 0..2 {
                    let value = middle.value(x, y, channel, sample).unwrap();
                    let wanted = expected(x, y, channel, sample);
                    assert!(
                        (value - wanted).abs() < 1e-5,
                        "band ({x}, {y}) channel {channel} sample {sample}: {value} != {wanted}"
                    );
                }
            }
        }
    }
    input.close().unwrap();
}

/// The tiled counterpart: write two whole-tile blocks, read one back by its
/// aligned range, and refuse a misaligned one.
#[test]
fn streams_deep_tile_blocks_and_reads_them_back() {
    let scratch = ScratchDir::new("deeptiles");
    let path = scratch.file("tiled.exr");

    const WIDTH: u32 = 32;
    const HEIGHT: u32 = 16;
    let spec = ImageSpec::new(WIDTH, HEIGHT, 2, PixelFormat::F32)
        .unwrap()
        .with_channel_names(["A", "Z"])
        .unwrap()
        .with_tile_size([16, 16, 1])
        .unwrap()
        .as_deep();

    let sample_count = |x: i32, y: i32| ((x + y) % 3) as usize;
    let expected = |x: i32, y: i32, channel: usize, sample: usize| -> f32 {
        (x as f32) - (y as f32) * 0.5 + (channel as f32) * 0.25 + (sample as f32) * 0.125
    };

    let block = |xs: std::ops::Range<i32>| -> oiio::DeepImage {
        let block_spec = ImageSpec::new((xs.end - xs.start) as u32, HEIGHT, 2, PixelFormat::F32)
            .unwrap()
            .with_channel_names(["A", "Z"])
            .unwrap()
            .with_origin([xs.start, 0, 0])
            .as_deep();
        let mut deep = DeepImage::new(&block_spec).unwrap();
        for y in 0..HEIGHT as i32 {
            for x in xs.clone() {
                deep.set_sample_count(x, y, sample_count(x, y)).unwrap();
                for sample in 0..sample_count(x, y) {
                    for channel in 0..2 {
                        deep.set_value(x, y, channel, sample, expected(x, y, channel, sample))
                            .unwrap();
                    }
                }
            }
        }
        deep
    };

    let mut output = oiio::ImageOutput::create(&path, &spec).unwrap();
    output
        .write_deep_tiles(0..16, 0..16, 0..1, &block(0..16))
        .unwrap();
    output
        .write_deep_tiles(16..32, 0..16, 0..1, &block(16..32))
        .unwrap();
    output.close().unwrap();

    let mut input = ImageInput::from_path(&path).unwrap();

    // A misaligned block is refused before OpenImageIO can misplace it.
    let error = input
        .read_deep_tiles_at(0, 0, 8..24, 0..16, 0..1)
        .unwrap_err();
    assert!(matches!(error, Error::InvalidRegion { axis: "x", .. }));

    let right = input.read_deep_tiles_at(0, 0, 16..32, 0..16, 0..1).unwrap();
    assert_eq!(right.dimensions(), [16, 16, 1]);
    assert_eq!(right.origin(), [16, 0, 0]);
    let mut checked = 0usize;
    for y in 0..HEIGHT as i32 {
        for x in 16..32 {
            assert_eq!(right.sample_count(x, y).unwrap(), sample_count(x, y));
            for sample in 0..sample_count(x, y) {
                for channel in 0..2 {
                    let value = right.value(x, y, channel, sample).unwrap();
                    let wanted = expected(x, y, channel, sample);
                    assert!(
                        (value - wanted).abs() < 1e-5,
                        "block ({x}, {y}) channel {channel} sample {sample}: {value} != {wanted}"
                    );
                    checked += 1;
                }
            }
        }
    }
    input.close().unwrap();
    assert!(checked > 0, "the block held no samples");
}

/// The streaming guards: bands out of order, mis-shaped, after a whole-image
/// write, or aimed at the wrong storage layout are all refused with this
/// crate's messages, before OpenEXR's pointer arithmetic can walk outside
/// the band's arrays.
#[test]
fn deep_streaming_is_refused_out_of_order_or_mis_shaped() {
    let scratch = ScratchDir::new("deepguards");

    let spec = ImageSpec::new(8, 4, 2, PixelFormat::F32)
        .unwrap()
        .with_channel_names(["A", "Z"])
        .unwrap()
        .as_deep();
    let band_spec = ImageSpec::new(8, 2, 2, PixelFormat::F32)
        .unwrap()
        .with_channel_names(["A", "Z"])
        .unwrap()
        .with_origin([0, 2, 0])
        .as_deep();
    let band = DeepImage::new(&band_spec).unwrap();

    // Out of order: the second band first.
    let mut output = oiio::ImageOutput::create(&scratch.file("order.exr"), &spec).unwrap();
    let error = output.write_deep_scanlines(2..4, &band).unwrap_err();
    assert!(
        error.to_string().contains("in order"),
        "unexpected error: {error}"
    );

    // Mis-shaped: a two-row band for a three-row range.
    let error = output.write_deep_scanlines(0..3, &band).unwrap_err();
    assert!(
        error.to_string().contains("covers"),
        "unexpected error: {error}"
    );

    // Tiles of a scanline file.
    let error = output
        .write_deep_tiles(0..8, 0..4, 0..1, &band)
        .unwrap_err();
    assert!(matches!(error, Error::InvalidImageSpec(_)));

    // After a whole-image write the subimage is spent; a band on top of it
    // would misalign OpenEXR's own scanline cursor against the new band's
    // arrays, so the cursor refuses it here.
    let whole = DeepImage::new(&spec).unwrap();
    let mut output = oiio::ImageOutput::create(&scratch.file("spent.exr"), &spec).unwrap();
    output.write_deep_image(&whole).unwrap();
    let error = output.write_deep_scanlines(0..2, &band).unwrap_err();
    assert!(
        error.to_string().contains("in order"),
        "unexpected error: {error}"
    );

    // A flat writer takes no deep bands.
    let flat_spec = ImageSpec::new(8, 4, 2, PixelFormat::F32).unwrap();
    let mut flat = oiio::ImageOutput::create(&scratch.file("flat.exr"), &flat_spec).unwrap();
    let error = flat.write_deep_scanlines(0..2, &band).unwrap_err();
    assert!(
        error.to_string().contains("flat pixels"),
        "unexpected error: {error}"
    );

    // Scanline bands of a tiled file.
    let tiled_spec = ImageSpec::new(32, 16, 2, PixelFormat::F32)
        .unwrap()
        .with_channel_names(["A", "Z"])
        .unwrap()
        .with_tile_size([16, 16, 1])
        .unwrap()
        .as_deep();
    let mut tiled = oiio::ImageOutput::create(&scratch.file("tiled.exr"), &tiled_spec).unwrap();
    let error = tiled.write_deep_scanlines(0..2, &band).unwrap_err();
    assert!(matches!(error, Error::InvalidImageSpec(_)));

    // Reading: a band outside the data window, tiles of a scanline file,
    // and deep reads of a flat file are each refused.
    let streamed = scratch.file("readable.exr");
    let mut output = oiio::ImageOutput::create(&streamed, &spec).unwrap();
    let whole = DeepImage::new(&spec).unwrap();
    output.write_deep_image(&whole).unwrap();
    output.close().unwrap();

    let mut input = ImageInput::from_path(&streamed).unwrap();
    let error = input.read_deep_scanlines_at(0, 0, 2..9).unwrap_err();
    assert!(matches!(error, Error::InvalidRegion { axis: "y", .. }));
    let error = input
        .read_deep_tiles_at(0, 0, 0..8, 0..4, 0..1)
        .unwrap_err();
    assert!(matches!(error, Error::InvalidImageSpec(_)));
    input.close().unwrap();

    let flat_file = scratch.file("flatfile.exr");
    write_image(&flat_file, &flat_spec, &f32_ramp(8 * 4 * 2)).unwrap();
    let mut input = ImageInput::from_path(&flat_file).unwrap();
    let error = input.read_deep_scanlines_at(0, 0, 0..2).unwrap_err();
    assert!(
        error.to_string().contains("not deep"),
        "unexpected error: {error}"
    );
    input.close().unwrap();
}
