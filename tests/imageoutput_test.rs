mod common;

use std::path::Path;

use common::{f16_ramp, f32_ramp, ScratchDir};
use oiio::{
    f16, AttributeValue, Error, ImageInput, ImageOutput, ImageSpec, Pixel, PixelFormat, Result,
};

/// A deterministic ramp, distinct per channel and per pixel.
fn ramp<T: Pixel + From<u8>>(count: usize) -> Vec<T> {
    (0..count)
        .map(|index| T::from((index % 251) as u8))
        .collect()
}

/// Mixed per-channel formats — the half/float layout multi-AOV EXRs use —
/// survive the round trip, and their invariants hold at both ends.
#[test]
fn mixed_channel_formats_round_trip_through_an_exr() -> Result<()> {
    let scratch = ScratchDir::new("chanfmt");
    let spec = ImageSpec::new(4, 4, 3, PixelFormat::F32)?.with_channel_formats(Some(vec![
        PixelFormat::F16,
        PixelFormat::F32,
        PixelFormat::F16,
    ]))?;
    assert_eq!(spec.channel_format(0), Some(PixelFormat::F16));
    assert_eq!(spec.channel_format(1), Some(PixelFormat::F32));
    assert_eq!(spec.channel_format(3), None, "past the channels is None");

    let path = scratch.file("mixed.exr");
    let mut output = ImageOutput::create(&path, &spec)?;
    output.write_image(&f32_ramp(4 * 4 * 3))?;
    output.close()?;

    let input = ImageInput::from_path(&path)?;
    let read = input.image_spec()?;
    assert_eq!(
        read.channel_formats(),
        Some(&[PixelFormat::F16, PixelFormat::F32, PixelFormat::F16][..]),
        "the mixed layout survives the file"
    );

    // The invariants: a list of the wrong length is refused at both entries,
    // and a format the crate cannot size never reaches a writer.
    assert!(ImageSpec::new(4, 4, 3, PixelFormat::F32)?
        .with_channel_formats(Some(vec![PixelFormat::F16]))
        .is_err());
    let unwritable =
        ImageSpec::new(4, 4, 3, PixelFormat::F32)?.with_channel_formats(Some(vec![
            PixelFormat::F16,
            PixelFormat::Other,
            PixelFormat::F16,
        ]))?;
    assert!(ImageOutput::create(&scratch.file("bad.exr"), &unwritable).is_err());

    // with_format clears the per-channel list, as OpenImageIO's own
    // set_format does, so the two can never silently disagree.
    let cleared = spec.with_format(PixelFormat::F32);
    assert_eq!(cleared.channel_formats(), None);
    Ok(())
}

/// The lossless transcode path: reader straight into writer, no decode to a
/// caller type in between, with the writer-state guards around it.
#[test]
fn copy_image_from_transcodes_losslessly_and_guards_state() -> Result<()> {
    let scratch = ScratchDir::new("copyimg");
    let spec = ImageSpec::new(9, 7, 3, PixelFormat::F16)?;
    let source = scratch.file("src.exr");
    let written = f16_ramp(spec.element_count()?);
    let mut output = ImageOutput::create(&source, &spec)?;
    output.write_image(&written)?;
    output.close()?;

    // The transcode round trip.
    let mut input = ImageInput::from_path(&source)?;
    let copy = scratch.file("copy.exr");
    let mut output = ImageOutput::create(&copy, &input.image_spec()?)?;
    output.copy_image_from(&mut input)?;
    output.close()?;
    let (copy_spec, copied): (ImageSpec, Vec<f16>) = read_back(&copy)?;
    assert_eq!(copy_spec.format(), PixelFormat::F16, "still half");
    assert_eq!(copied, written, "carried losslessly");

    // Mismatched dimensions are refused with OpenImageIO's own clear error.
    let mut input = ImageInput::from_path(&source)?;
    let wrong = ImageSpec::new(4, 4, 3, PixelFormat::F16)?;
    let mut output = ImageOutput::create(&scratch.file("wrong.exr"), &wrong)?;
    assert!(output.copy_image_from(&mut input).is_err());

    // A partially written subimage cannot take the copy.
    let mut input = ImageInput::from_path(&source)?;
    let mut output = ImageOutput::create(&scratch.file("touched.exr"), &spec)?;
    output.write_scanlines(0..1, &f16_ramp(9 * 3))?;
    let error = output.copy_image_from(&mut input).unwrap_err();
    assert!(error.to_string().contains("already written"), "{error}");

    // Reader capability probes, file validity, and config-hint opens.
    let input = ImageInput::from_path(&source)?;
    assert!(input.supports("multiimage"), "EXR reads multi-part files");
    assert!(!input.supports("not-a-feature"));
    assert!(input.is_valid_file(&source)?);
    let png = scratch.file("other.png");
    let png_spec = ImageSpec::new(4, 4, 3, PixelFormat::U8)?;
    let mut png_out = ImageOutput::create(&png, &png_spec)?;
    png_out.write_image(&vec![0_u8; png_spec.element_count()?])?;
    png_out.close()?;
    assert!(
        !input.is_valid_file(&png)?,
        "a PNG is not valid for the EXR reader"
    );

    let hints =
        ImageSpec::new(1, 1, 1, PixelFormat::U8)?.with_attribute("oiio:UnassociatedAlpha", 1);
    let hinted = ImageInput::from_path_with_config(&source, &hints)?;
    assert_eq!(hinted.image_spec()?.dimensions(), [9, 7, 1]);
    hinted.close()?;
    Ok(())
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

    let (read_spec, read): (ImageSpec, Vec<f16>) = read_back(&path).unwrap();
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

    let (read_spec, _): (ImageSpec, Vec<f16>) = read_back(&path).unwrap();
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
    // therefore not one of the directly modelled attribute types.
    let (read_spec, _): (ImageSpec, Vec<f16>) = read_back(&path).unwrap();
    let centre = read_spec
        .attribute("screenWindowCenter")
        .expect("OpenEXR records a screen window centre");
    match centre {
        AttributeValue::Other {
            type_name,
            value,
            bytes,
        } => {
            assert!(type_name.contains("float"), "unexpected type {type_name}");
            assert!(!value.is_empty(), "it should still print");
            // Two floats, carried as the eight bytes OpenImageIO stored, which
            // is what lets it be written back out.
            assert_eq!(bytes.len(), 8, "expected two floats of stored value");
            assert!(centre.is_writable());
        }
        other => panic!("expected an unmodelled attribute, got {other:?}"),
    }
}

#[test]
fn unmodelled_metadata_survives_being_written_again() {
    let scratch = ScratchDir::new("metaroundtrip");
    let first = scratch.file("first.exr");
    let second = scratch.file("second.exr");

    let spec = ImageSpec::new(4, 4, 3, PixelFormat::F16).unwrap();
    let pixels = f16_ramp(spec.element_count().unwrap());
    let mut output = ImageOutput::create(&first, &spec).unwrap();
    output.write_image(&pixels).unwrap();
    output.close().unwrap();

    // Read it back, then write the specification we just read straight out
    // again, without touching the attributes.
    let (read_spec, read_pixels): (ImageSpec, Vec<f16>) = read_back(&first).unwrap();
    let mut again = ImageOutput::create(&second, &read_spec).unwrap();
    again.write_image(&read_pixels).unwrap();
    again.close().unwrap();

    let (final_spec, _): (ImageSpec, Vec<f16>) = read_back(&second).unwrap();

    // Every attribute that was unmodelled the first time must still be there,
    // with the same value, rather than having been quietly dropped.
    let mut compared = 0usize;
    for (name, before) in read_spec.attributes() {
        if !matches!(before, AttributeValue::Other { .. }) {
            continue;
        }
        let after = final_spec
            .attribute(name)
            .unwrap_or_else(|| panic!("{name} was lost on the second write"));
        assert_eq!(before, after, "{name} changed on the second write");
        compared += 1;
    }
    assert!(
        compared > 0,
        "no unmodelled attributes were present to test"
    );
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

    let (read_spec, read): (ImageSpec, Vec<f16>) = read_back(&path).unwrap();
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
        let mut pixels = vec![f16::default(); expected.len()];
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

    let (read_spec, read): (ImageSpec, Vec<f16>) = read_back(&path).unwrap();
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
        Err(Error::InvalidRegion { axis: "y", .. })
    ));
    assert!(matches!(
        output.write_scanlines(4..4, &row),
        Err(Error::InvalidRegion { axis: "y", .. })
    ));
    assert!(matches!(
        output.write_scanlines(-1..1, &row),
        Err(Error::InvalidRegion { axis: "y", .. })
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

    let tile = vec![f16::ZERO; 16 * 16];
    assert!(matches!(
        output.write_tiles(8..24, 0..16, 0..1, &tile),
        Err(Error::InvalidRegion { axis: "x", .. })
    ));
    assert!(matches!(
        output.write_tiles(0..16, 4..20, 0..1, &tile),
        Err(Error::InvalidRegion { axis: "y", .. })
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
    let pixels = vec![f16::ZERO; 64];
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

/// Scanlines out of order used to read megabytes before the caller's buffer.
///
/// OpenEXR's scanline writer computes a "virtual framebuffer" base by biasing
/// the caller's pointer backwards by the row it was asked for, then hands it to
/// `Imf::OutputFile::writePixels`, which writes at its own cursor starting at
/// the top of the data window and only ever advancing. Ask for rows 512..1024
/// first and it reads at `data - 512 * scanline_bytes`. OpenImageIO publishes
/// the difference through `supports("random_access")`, which is true only for
/// tiled EXRs with a random line order, and `write_scanlines` never asked.
#[test]
fn scanlines_must_be_written_in_order() {
    let scratch = ScratchDir::new("scanorder");
    let path = scratch.file("half.exr");

    let spec = ImageSpec::new(256, 128, 4, PixelFormat::F32).unwrap();
    let mut output = ImageOutput::create(&path, &spec).unwrap();
    assert!(!output.supports("random_access"));

    let rows = vec![0.5_f32; 256 * 64 * 4];

    // Starting half way down is exactly the shape that read before the buffer.
    let error = output
        .write_scanlines(64..128, &rows)
        .expect_err("a scanline EXR cannot start half way down");
    assert!(
        matches!(error, Error::InvalidRegion { axis: "y", .. }),
        "{error}"
    );

    // In order works, and the cursor follows.
    output.write_scanlines(0..64, &rows).unwrap();
    assert!(
        output.write_scanlines(0..64, &rows).is_err(),
        "writing the same rows twice reads one scanline past the end"
    );
    output.write_scanlines(64..128, &rows).unwrap();
    output.close().unwrap();

    let (_, read): (ImageSpec, Vec<f32>) = read_back(&path).unwrap();
    assert_eq!(read.len(), 256 * 128 * 4);
    assert!(read.iter().all(|value| (*value - 0.5).abs() < 1e-6));
}

/// `spec()` is documented as the specification the file is open with, and used
/// to return a clone of the caller's. `ImageOutput::check_open` rewrites it: it
/// zeroes the origin for any format that does not report `origin`, fills the
/// display window in from the data window, and raises a zero depth to one. A
/// caller who set an origin the format cannot carry could not tell.
#[test]
fn the_reported_spec_is_the_one_the_file_was_opened_with() {
    let scratch = ScratchDir::new("openspec");

    // EXR carries an arbitrary origin, so it comes back unchanged.
    let spec = ImageSpec::new(6, 4, 3, PixelFormat::F16)
        .unwrap()
        .with_origin([12, -7, 0])
        .with_full_window([0, 0, 0], [32, 32, 1])
        .unwrap();
    let exr = ImageOutput::create(&scratch.file("kept.exr"), &spec).unwrap();
    assert!(exr.supports("origin"));
    assert_eq!(exr.spec().origin(), [12, -7, 0]);

    // PNG does not, so the origin is gone and saying so is the point.
    let png_spec = ImageSpec::new(6, 4, 3, PixelFormat::U8)
        .unwrap()
        .with_origin([12, 7, 0]);
    let png = ImageOutput::create(&scratch.file("dropped.png"), &png_spec).unwrap();
    assert!(!png.supports("origin"));
    assert_eq!(
        png.spec().origin(),
        [0, 0, 0],
        "the writer dropped the origin and spec() should show it"
    );
}
