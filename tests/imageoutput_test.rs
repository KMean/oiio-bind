use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};

use oiio::{AttributeValue, Error, ImageInput, ImageOutput, ImageSpec, Pixel, PixelFormat, Result};

/// A scratch directory that removes itself when the test ends.
struct ScratchDir(PathBuf);

impl ScratchDir {
    fn new(name: &str) -> Self {
        static COUNTER: AtomicU32 = AtomicU32::new(0);
        let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
        let path =
            std::env::temp_dir().join(format!("oiio-bind-{name}-{}-{unique}", std::process::id()));
        std::fs::create_dir_all(&path).expect("could not create the scratch directory");
        Self(path)
    }

    fn file(&self, name: &str) -> PathBuf {
        self.0.join(name)
    }
}

impl Drop for ScratchDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// A deterministic ramp, distinct per channel and per pixel.
fn ramp<T: Pixel + From<u8>>(count: usize) -> Vec<T> {
    (0..count)
        .map(|index| T::from((index % 251) as u8))
        .collect()
}

fn f32_ramp(count: usize) -> Vec<f32> {
    (0..count).map(|index| index as f32 * 0.125 - 4.0).collect()
}

fn f16_ramp(count: usize) -> Vec<half::f16> {
    (0..count)
        .map(|index| half::f16::from_f32(index as f32 * 0.5 - 2.0))
        .collect()
}

fn read_back<T: Pixel>(path: &Path) -> Result<(ImageSpec, Vec<T>)> {
    let mut input = ImageInput::from_path(path)?;
    let spec = input.image_spec()?;
    let mut pixels = vec![T::default(); spec.element_count()?];
    input.read_image_into(&mut pixels)?;
    input.close()?;
    Ok((spec, pixels))
}

#[test]
fn round_trips_an_eight_bit_png() {
    let scratch = ScratchDir::new("png8");
    let path = scratch.file("ramp.png");

    let spec = ImageSpec::new(7, 5, 3, PixelFormat::U8).unwrap();
    let written: Vec<u8> = ramp(spec.element_count().unwrap());

    let mut output = ImageOutput::create(&path, &spec).unwrap();
    assert_eq!(output.format_name(), "png");
    output.write_image(&written).unwrap();
    output.close().unwrap();

    let (read_spec, read): (ImageSpec, Vec<u8>) = read_back(&path).unwrap();
    assert_eq!(read_spec.dimensions(), [7, 5, 1]);
    assert_eq!(read_spec.channel_count(), 3);
    assert_eq!(read_spec.format(), PixelFormat::U8);
    assert_eq!(read, written);
}

#[test]
fn round_trips_a_sixteen_bit_png() {
    let scratch = ScratchDir::new("png16");
    let path = scratch.file("ramp.png");

    let spec = ImageSpec::new(4, 4, 1, PixelFormat::U16).unwrap();
    let written: Vec<u16> = (0..spec.element_count().unwrap())
        .map(|index| (index as u16) * 4096)
        .collect();

    let mut output = ImageOutput::create(&path, &spec).unwrap();
    output.write_image(&written).unwrap();
    output.close().unwrap();

    let (read_spec, read): (ImageSpec, Vec<u16>) = read_back(&path).unwrap();
    assert_eq!(read_spec.format(), PixelFormat::U16);
    assert_eq!(read, written);
}

#[test]
fn round_trips_a_float_exr_without_loss() {
    let scratch = ScratchDir::new("exr32");
    let path = scratch.file("ramp.exr");

    let spec = ImageSpec::new(9, 6, 4, PixelFormat::F32).unwrap();
    let written = f32_ramp(spec.element_count().unwrap());

    let mut output = ImageOutput::create(&path, &spec).unwrap();
    output.write_image(&written).unwrap();
    output.close().unwrap();

    let (read_spec, read): (ImageSpec, Vec<f32>) = read_back(&path).unwrap();
    assert_eq!(read_spec.format(), PixelFormat::F32);
    assert_eq!(read_spec.channel_names(), ["R", "G", "B", "A"]);
    assert_eq!(read, written);
}

#[test]
fn round_trips_a_half_exr_without_loss() {
    let scratch = ScratchDir::new("exr16");
    let path = scratch.file("ramp.exr");

    let spec = ImageSpec::new(5, 3, 3, PixelFormat::F16).unwrap();
    let written = f16_ramp(spec.element_count().unwrap());

    let mut output = ImageOutput::create(&path, &spec).unwrap();
    output.write_image(&written).unwrap();
    output.close().unwrap();

    let (read_spec, read): (ImageSpec, Vec<half::f16>) = read_back(&path).unwrap();
    assert_eq!(read_spec.format(), PixelFormat::F16);
    assert_eq!(read, written);
}

#[test]
fn preserves_channel_names_and_metadata() {
    let scratch = ScratchDir::new("metadata");
    let path = scratch.file("named.exr");

    let spec = ImageSpec::new(4, 4, 3, PixelFormat::F16)
        .unwrap()
        .with_channel_names(["depth.Z", "motion.u", "motion.v"])
        .unwrap()
        .with_alpha_channel(None)
        .unwrap()
        .with_attribute("Artist", "oiio-bind")
        .with_attribute("Orientation", 1)
        .with_attribute("PixelAspectRatio", 2.0_f32);

    let written = f16_ramp(spec.element_count().unwrap());
    let mut output = ImageOutput::create(&path, &spec).unwrap();
    output.write_image(&written).unwrap();
    output.close().unwrap();

    let (read_spec, _): (ImageSpec, Vec<half::f16>) = read_back(&path).unwrap();
    assert_eq!(
        read_spec.channel_names(),
        ["depth.Z", "motion.u", "motion.v"]
    );
    assert_eq!(
        read_spec
            .attribute("Artist")
            .and_then(AttributeValue::as_str),
        Some("oiio-bind")
    );
    assert_eq!(
        read_spec
            .attribute("Orientation")
            .and_then(AttributeValue::as_int),
        Some(1)
    );
    assert_eq!(
        read_spec
            .attribute("PixelAspectRatio")
            .and_then(AttributeValue::as_float),
        Some(2.0)
    );
}

#[test]
fn reports_unmodelled_metadata_types_without_losing_them() {
    let scratch = ScratchDir::new("othermeta");
    let path = scratch.file("other.exr");

    let spec = ImageSpec::new(4, 4, 3, PixelFormat::F16).unwrap();
    let written = f16_ramp(spec.element_count().unwrap());
    let mut output = ImageOutput::create(&path, &spec).unwrap();
    output.write_image(&written).unwrap();
    output.close().unwrap();

    // OpenEXR always records a screen window centre, which is a float pair and
    // therefore not one of the three directly modelled attribute types.
    let (read_spec, _): (ImageSpec, Vec<half::f16>) = read_back(&path).unwrap();
    let centre = read_spec
        .attribute("screenWindowCenter")
        .expect("OpenEXR records a screen window centre");
    match centre {
        AttributeValue::Other { type_name, value } => {
            assert!(type_name.contains("float"), "unexpected type {type_name}");
            assert!(!value.is_empty());
            assert!(!centre.is_writable());
        }
        other => panic!("expected an unmodelled attribute, got {other:?}"),
    }
}

#[test]
fn writes_an_image_in_scanline_batches() {
    let scratch = ScratchDir::new("scanlines");
    let path = scratch.file("batched.exr");

    let spec = ImageSpec::new(8, 6, 3, PixelFormat::F32).unwrap();
    let [width, height, _] = spec.dimensions();
    let channels = spec.channel_count();
    let written = f32_ramp(spec.element_count().unwrap());
    let row = (width * channels) as usize;

    let mut output = ImageOutput::create(&path, &spec).unwrap();
    for begin in (0..height).step_by(2) {
        let end = (begin + 2).min(height);
        let start_value = begin as usize * row;
        let end_value = end as usize * row;
        output
            .write_scanlines(begin as i32..end as i32, &written[start_value..end_value])
            .unwrap();
    }
    output.close().unwrap();

    let (_, read): (ImageSpec, Vec<f32>) = read_back(&path).unwrap();
    assert_eq!(read, written);
}

#[test]
fn writes_a_tiled_exr_tile_by_tile() {
    let scratch = ScratchDir::new("tiled");
    let path = scratch.file("tiled.exr");

    assert!(ImageOutput::plugin_supports(&path, "tiles").unwrap());

    // 40x24 with 16x16 tiles leaves partial tiles on both edges.
    let spec = ImageSpec::new(40, 24, 3, PixelFormat::F16)
        .unwrap()
        .with_tile_size([16, 16, 1])
        .unwrap();
    let [width, height, _] = spec.dimensions();
    let [tile_width, tile_height, _] = spec.tile_dimensions();
    let channels = spec.channel_count() as usize;
    let written = f16_ramp(spec.element_count().unwrap());

    let mut output = ImageOutput::create(&path, &spec).unwrap();
    for y in (0..height).step_by(tile_height as usize) {
        for x in (0..width).step_by(tile_width as usize) {
            let x_end = (x + tile_width).min(width);
            let y_end = (y + tile_height).min(height);

            // Gather this tile's pixels out of the whole-image buffer.
            let mut tile =
                Vec::with_capacity((x_end - x) as usize * (y_end - y) as usize * channels);
            for row in y..y_end {
                let start = (row as usize * width as usize + x as usize) * channels;
                let end = start + (x_end - x) as usize * channels;
                tile.extend_from_slice(&written[start..end]);
            }

            output
                .write_tiles(x as i32..x_end as i32, y as i32..y_end as i32, 0..1, &tile)
                .unwrap();
        }
    }
    output.close().unwrap();

    let (read_spec, read): (ImageSpec, Vec<half::f16>) = read_back(&path).unwrap();
    assert!(read_spec.is_tiled());
    assert_eq!(read_spec.tile_dimensions(), [16, 16, 1]);
    assert_eq!(read, written);
}

#[test]
fn writes_and_reads_back_mip_levels() {
    let scratch = ScratchDir::new("mips");
    let path = scratch.file("mipped.exr");

    assert!(ImageOutput::plugin_supports(&path, "mipmap").unwrap());

    // OpenImageIO's OpenEXR writer only enables mip levels for tiled files
    // that declare a texture format; `openexr:levelmode` is consulted only
    // inside that branch.
    // A mipmapped OpenEXR declares the whole pyramid, so every level down to
    // 1x1 must be written.
    let levels: Vec<ImageSpec> = [(16_u32, 16_u32), (8, 8), (4, 4), (2, 2), (1, 1)]
        .into_iter()
        .map(|(width, height)| {
            ImageSpec::new(width, height, 3, PixelFormat::F16)
                .unwrap()
                .with_tile_size([4, 4, 1])
                .unwrap()
                .with_attribute("textureformat", "Plain Texture")
        })
        .collect();

    let mut output = ImageOutput::create(&path, &levels[0]).unwrap();
    for (index, level) in levels.iter().enumerate() {
        if index > 0 {
            output.append_mip_level(level).unwrap();
        }
        let pixels = f16_ramp(level.element_count().unwrap());
        output.write_image(&pixels).unwrap();
    }
    output.close().unwrap();

    let mut input = ImageInput::from_path(&path).unwrap();
    for (index, level) in levels.iter().enumerate() {
        let spec = input.image_spec_at(0, index as u32).unwrap();
        assert_eq!(spec.dimensions(), level.dimensions());

        let expected = f16_ramp(level.element_count().unwrap());
        let mut pixels = vec![half::f16::default(); expected.len()];
        input
            .read_image_into_at(0, index as u32, &mut pixels)
            .unwrap();
        assert_eq!(pixels, expected, "mip level {index} did not round-trip");
    }
    assert!(input.image_spec_at(0, levels.len() as u32).is_err());
    input.close().unwrap();
}

#[test]
fn writes_and_reads_back_subimages() {
    let scratch = ScratchDir::new("subimages");
    let path = scratch.file("parts.exr");

    assert!(ImageOutput::plugin_supports(&path, "multiimage").unwrap());

    // Every part of a multi-part OpenEXR file shares one display window.
    let parts: Vec<ImageSpec> = [(8_u32, 4_u32, 3_u32), (6, 6, 1)]
        .into_iter()
        .map(|(width, height, channels)| {
            ImageSpec::new(width, height, channels, PixelFormat::F32)
                .unwrap()
                .with_full_window([0, 0, 0], [8, 6, 1])
                .unwrap()
        })
        .collect();

    let mut output = ImageOutput::create_multi_subimage(&path, &parts).unwrap();
    for (index, part) in parts.iter().enumerate() {
        if index > 0 {
            output.append_subimage(part).unwrap();
        }
        assert_eq!(output.spec().dimensions(), part.dimensions());
        output
            .write_image(&f32_ramp(part.element_count().unwrap()))
            .unwrap();
    }
    output.close().unwrap();

    let mut input = ImageInput::from_path(&path).unwrap();
    for (index, part) in parts.iter().enumerate() {
        let spec = input.image_spec_at(index as u32, 0).unwrap();
        assert_eq!(spec.dimensions(), part.dimensions());
        assert_eq!(spec.channel_count(), part.channel_count());

        let expected = f32_ramp(part.element_count().unwrap());
        let mut pixels = vec![0.0_f32; expected.len()];
        input
            .read_image_into_at(index as u32, 0, &mut pixels)
            .unwrap();
        assert_eq!(pixels, expected, "subimage {index} did not round-trip");
    }
    input.close().unwrap();
}

#[test]
fn honours_a_non_zero_data_window_origin() {
    let scratch = ScratchDir::new("origin");
    let path = scratch.file("offset.exr");

    let spec = ImageSpec::new(6, 4, 3, PixelFormat::F16)
        .unwrap()
        .with_origin([12, -7, 0])
        .with_full_window([0, 0, 0], [32, 32, 1])
        .unwrap();
    let written = f16_ramp(spec.element_count().unwrap());

    let mut output = ImageOutput::create(&path, &spec).unwrap();
    // Scanline ranges are in image coordinates, not buffer rows.
    output.write_scanlines(-7..-3, &written).unwrap();
    output.close().unwrap();

    let (read_spec, read): (ImageSpec, Vec<half::f16>) = read_back(&path).unwrap();
    assert_eq!(read_spec.origin(), [12, -7, 0]);
    assert_eq!(read_spec.full_dimensions(), [32, 32, 1]);
    assert_eq!(read, written);
}

#[test]
fn rejects_a_buffer_whose_length_does_not_match_the_specification() {
    let scratch = ScratchDir::new("badlen");
    let path = scratch.file("short.exr");

    let spec = ImageSpec::new(4, 4, 3, PixelFormat::F32).unwrap();
    let mut output = ImageOutput::create(&path, &spec).unwrap();

    let short = vec![0.0_f32; spec.element_count().unwrap() - 1];
    assert!(matches!(
        output.write_image(&short),
        Err(Error::BufferLength {
            expected: 48,
            actual: 47
        })
    ));

    let long = vec![0.0_f32; spec.element_count().unwrap() + 1];
    assert!(matches!(
        output.write_image(&long),
        Err(Error::BufferLength { .. })
    ));

    // The failed writes did not consume the file: a correct write still works.
    output
        .write_image(&f32_ramp(spec.element_count().unwrap()))
        .unwrap();
    output.close().unwrap();
}

#[test]
fn rejects_write_regions_outside_the_data_window() {
    let scratch = ScratchDir::new("badregion");
    let path = scratch.file("region.exr");

    let spec = ImageSpec::new(8, 8, 1, PixelFormat::F32).unwrap();
    let mut output = ImageOutput::create(&path, &spec).unwrap();

    let row = vec![0.0_f32; 8];
    assert!(matches!(
        output.write_scanlines(7..9, &[row.clone(), row.clone()].concat()),
        Err(Error::InvalidWriteRegion { axis: "y", .. })
    ));
    assert!(matches!(
        output.write_scanlines(4..4, &row),
        Err(Error::InvalidWriteRegion { axis: "y", .. })
    ));
    assert!(matches!(
        output.write_scanlines(-1..1, &row),
        Err(Error::InvalidWriteRegion { axis: "y", .. })
    ));
}

#[test]
fn rejects_tile_regions_that_are_not_on_the_tile_grid() {
    let scratch = ScratchDir::new("badtiles");
    let path = scratch.file("grid.exr");

    let spec = ImageSpec::new(32, 32, 1, PixelFormat::F16)
        .unwrap()
        .with_tile_size([16, 16, 1])
        .unwrap();
    let mut output = ImageOutput::create(&path, &spec).unwrap();

    let tile = vec![half::f16::ZERO; 16 * 16];
    assert!(matches!(
        output.write_tiles(8..24, 0..16, 0..1, &tile),
        Err(Error::InvalidWriteRegion { axis: "x", .. })
    ));
    assert!(matches!(
        output.write_tiles(0..16, 4..20, 0..1, &tile),
        Err(Error::InvalidWriteRegion { axis: "y", .. })
    ));
    // Aligned, but the buffer describes a different region.
    assert!(matches!(
        output.write_tiles(0..16, 0..16, 0..1, &tile[..8]),
        Err(Error::BufferLength { .. })
    ));
}

#[test]
fn rejects_tile_writes_to_a_scanline_image() {
    let scratch = ScratchDir::new("notiles");
    let path = scratch.file("scanline.exr");

    let spec = ImageSpec::new(8, 8, 1, PixelFormat::F16).unwrap();
    assert!(!spec.is_tiled());

    let mut output = ImageOutput::create(&path, &spec).unwrap();
    let pixels = vec![half::f16::ZERO; 64];
    assert!(matches!(
        output.write_tiles(0..8, 0..8, 0..1, &pixels),
        Err(Error::InvalidImageSpec(_))
    ));
}

#[test]
fn reports_a_file_name_no_plugin_can_write() {
    let scratch = ScratchDir::new("noplugin");
    let path = scratch.file("image.not-an-image-format");

    let spec = ImageSpec::new(4, 4, 3, PixelFormat::U8).unwrap();
    let error = ImageOutput::create(&path, &spec).unwrap_err();
    assert!(matches!(error, Error::CreateImage { .. }));
    assert!(ImageOutput::plugin_supports(&path, "tiles").is_err());
}

#[test]
fn reports_which_features_a_format_supports() {
    let scratch = ScratchDir::new("features");
    let png = scratch.file("image.png");
    let exr = scratch.file("image.exr");

    assert!(!ImageOutput::plugin_supports(&png, "tiles").unwrap());
    assert!(!ImageOutput::plugin_supports(&png, "mipmap").unwrap());
    assert!(ImageOutput::plugin_supports(&exr, "tiles").unwrap());
    assert!(ImageOutput::plugin_supports(&exr, "multiimage").unwrap());
}

#[test]
fn refuses_to_write_a_deep_specification_through_the_contiguous_api() {
    let scratch = ScratchDir::new("deep");
    let path = scratch.file("deep.exr");

    let spec = ImageSpec::new(4, 4, 1, PixelFormat::F32).unwrap();
    let mut output = ImageOutput::create(&path, &spec).unwrap();

    // A writer opened for flat pixels stays flat; the guard is exercised
    // through the specification the writer reports.
    assert!(!output.spec().is_deep());
    output.write_image(&f32_ramp(16)).unwrap();
    output.close().unwrap();
}
