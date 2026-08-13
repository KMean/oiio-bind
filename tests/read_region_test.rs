//! Partial reads: scanline ranges, tile blocks, channel subsets, and mip
//! levels, each checked against the whole image read the ordinary way.

mod common;

use common::{crop, f16_ramp, f32_ramp, write_image, ScratchDir};
use oiio::{f16, Error, ImageInput, ImageOutput, ImageSpec, PixelFormat};

/// 24x16, 4 channels, stored as scanlines.
fn scanline_fixture(scratch: &ScratchDir) -> (std::path::PathBuf, ImageSpec, Vec<f32>) {
    let path = scratch.file("scanline.exr");
    let spec = ImageSpec::new(24, 16, 4, PixelFormat::F32).unwrap();
    let pixels = f32_ramp(spec.element_count().unwrap());
    write_image(&path, &spec, &pixels).unwrap();
    (path, spec, pixels)
}

/// 40x24, 3 channels, 16x16 tiles, so both edges hold partial tiles.
fn tiled_fixture(scratch: &ScratchDir) -> (std::path::PathBuf, ImageSpec, Vec<f16>) {
    let path = scratch.file("tiled.exr");
    let spec = ImageSpec::new(40, 24, 3, PixelFormat::F16)
        .unwrap()
        .with_tile_size([16, 16, 1])
        .unwrap();
    let pixels = f16_ramp(spec.element_count().unwrap());
    write_image(&path, &spec, &pixels).unwrap();
    (path, spec, pixels)
}

#[test]
fn reads_a_scanline_range() {
    let scratch = ScratchDir::new("scanlines");
    let (path, spec, whole) = scanline_fixture(&scratch);

    let mut input = ImageInput::from_path(&path).unwrap();
    let roi = spec.data_window().unwrap().with_y(4..9).unwrap();
    let mut read = vec![0.0_f32; roi.element_count().unwrap()];
    input.read_region_into(roi, &mut read).unwrap();
    input.close().unwrap();

    assert_eq!(read, crop(&whole, 24, 4, 0..24, 4..9, 0..4));
}

#[test]
fn reads_a_channel_subset_of_a_scanline_image() {
    let scratch = ScratchDir::new("channels");
    let (path, spec, whole) = scanline_fixture(&scratch);

    let mut input = ImageInput::from_path(&path).unwrap();
    let roi = spec.data_window().unwrap().with_channels(1..3).unwrap();
    let mut read = vec![0.0_f32; roi.element_count().unwrap()];
    input.read_region_into(roi, &mut read).unwrap();
    input.close().unwrap();

    assert_eq!(read.len(), 24 * 16 * 2);
    assert_eq!(read, crop(&whole, 24, 4, 0..24, 0..16, 1..3));
}

#[test]
fn reads_a_scanline_range_and_channel_subset_together() {
    let scratch = ScratchDir::new("both");
    let (path, spec, whole) = scanline_fixture(&scratch);

    let mut input = ImageInput::from_path(&path).unwrap();
    let roi = spec
        .data_window()
        .unwrap()
        .with_y(2..6)
        .unwrap()
        .with_channels(3..4)
        .unwrap();
    let mut read = vec![0.0_f32; roi.element_count().unwrap()];
    input.read_region_into(roi, &mut read).unwrap();
    input.close().unwrap();

    assert_eq!(read, crop(&whole, 24, 4, 0..24, 2..6, 3..4));
}

#[test]
fn reads_an_aligned_tile_block() {
    let scratch = ScratchDir::new("tileblock");
    let (path, spec, whole) = tiled_fixture(&scratch);

    let mut input = ImageInput::from_path(&path).unwrap();
    let roi = spec
        .data_window()
        .unwrap()
        .with_x(16..32)
        .unwrap()
        .with_y(0..16)
        .unwrap();
    let mut read = vec![f16::ZERO; roi.element_count().unwrap()];
    input.read_region_into(roi, &mut read).unwrap();
    input.close().unwrap();

    assert_eq!(read, crop(&whole, 40, 3, 16..32, 0..16, 0..3));
}

#[test]
fn reads_tile_blocks_clipped_to_the_data_window_edge() {
    let scratch = ScratchDir::new("tileedge");
    let (path, spec, whole) = tiled_fixture(&scratch);

    let mut input = ImageInput::from_path(&path).unwrap();

    // The right edge: 32..40 is eight pixels of a sixteen-pixel tile.
    let roi = spec
        .data_window()
        .unwrap()
        .with_x(32..40)
        .unwrap()
        .with_y(0..16)
        .unwrap();
    let mut read = vec![f16::ZERO; roi.element_count().unwrap()];
    input.read_region_into(roi, &mut read).unwrap();
    assert_eq!(read, crop(&whole, 40, 3, 32..40, 0..16, 0..3));

    // The bottom edge: 16..24 is eight rows of a sixteen-row tile.
    let roi = spec
        .data_window()
        .unwrap()
        .with_x(0..16)
        .unwrap()
        .with_y(16..24)
        .unwrap();
    let mut read = vec![f16::ZERO; roi.element_count().unwrap()];
    input.read_region_into(roi, &mut read).unwrap();
    assert_eq!(read, crop(&whole, 40, 3, 0..16, 16..24, 0..3));

    input.close().unwrap();
}

#[test]
fn reads_a_channel_subset_of_a_tile_block() {
    let scratch = ScratchDir::new("tilechannels");
    let (path, spec, whole) = tiled_fixture(&scratch);

    let mut input = ImageInput::from_path(&path).unwrap();
    let roi = spec
        .data_window()
        .unwrap()
        .with_x(0..16)
        .unwrap()
        .with_y(0..16)
        .unwrap()
        .with_channels(2..3)
        .unwrap();
    let mut read = vec![f16::ZERO; roi.element_count().unwrap()];
    input.read_region_into(roi, &mut read).unwrap();
    input.close().unwrap();

    assert_eq!(read.len(), 16 * 16);
    assert_eq!(read, crop(&whole, 40, 3, 0..16, 0..16, 2..3));
}

#[test]
fn reads_a_region_of_a_mip_level() {
    let scratch = ScratchDir::new("mipregion");
    let path = scratch.file("mipped.exr");

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
        output
            .write_image(&f16_ramp(level.element_count().unwrap()))
            .unwrap();
    }
    output.close().unwrap();

    // A 4x4 corner of the 8x8 second level, not of the base image.
    let whole_level = f16_ramp(levels[1].element_count().unwrap());
    let mut input = ImageInput::from_path(&path).unwrap();
    let roi = input
        .image_spec_at(0, 1)
        .unwrap()
        .data_window()
        .unwrap()
        .with_x(4..8)
        .unwrap()
        .with_y(0..4)
        .unwrap();
    let mut read = vec![f16::ZERO; roi.element_count().unwrap()];
    input.read_region_into_at(0, 1, roi, &mut read).unwrap();
    input.close().unwrap();

    assert_eq!(read, crop(&whole_level, 8, 3, 4..8, 0..4, 0..3));
}

#[test]
fn reads_a_region_of_an_offset_data_window() {
    let scratch = ScratchDir::new("offsetregion");
    let path = scratch.file("offset.exr");

    let spec = ImageSpec::new(12, 8, 3, PixelFormat::F32)
        .unwrap()
        .with_origin([100, -20, 0])
        .with_full_window([0, 0, 0], [64, 64, 1])
        .unwrap();
    let whole = f32_ramp(spec.element_count().unwrap());
    write_image(&path, &spec, &whole).unwrap();

    let mut input = ImageInput::from_path(&path).unwrap();
    // Rows are addressed in image coordinates, not buffer rows.
    let roi = spec.data_window().unwrap().with_y(-18..-16).unwrap();
    let mut read = vec![0.0_f32; roi.element_count().unwrap()];
    input.read_region_into(roi, &mut read).unwrap();
    input.close().unwrap();

    assert_eq!(read, crop(&whole, 12, 3, 0..12, 2..4, 0..3));
}

#[test]
fn rejects_a_partial_width_region_of_a_scanline_image() {
    let scratch = ScratchDir::new("partialwidth");
    let (path, spec, _) = scanline_fixture(&scratch);

    let mut input = ImageInput::from_path(&path).unwrap();
    let roi = spec.data_window().unwrap().with_x(0..8).unwrap();
    let mut read = vec![0.0_f32; roi.element_count().unwrap()];
    assert!(matches!(
        input.read_region_into(roi, &mut read),
        Err(Error::InvalidRegion { axis: "x", .. })
    ));
}

#[test]
fn rejects_tile_blocks_that_are_not_on_the_tile_grid() {
    let scratch = ScratchDir::new("unaligned");
    let (path, spec, _) = tiled_fixture(&scratch);

    let mut input = ImageInput::from_path(&path).unwrap();
    let roi = spec
        .data_window()
        .unwrap()
        .with_x(8..24)
        .unwrap()
        .with_y(0..16)
        .unwrap();
    let mut read = vec![f16::ZERO; roi.element_count().unwrap()];
    assert!(matches!(
        input.read_region_into(roi, &mut read),
        Err(Error::InvalidRegion { axis: "x", .. })
    ));
}

#[test]
fn rejects_regions_outside_the_data_window_and_channel_range() {
    let scratch = ScratchDir::new("outside");
    let (path, spec, _) = scanline_fixture(&scratch);

    let mut input = ImageInput::from_path(&path).unwrap();

    let roi = spec.data_window().unwrap().with_y(12..20).unwrap();
    let mut read = vec![0.0_f32; roi.element_count().unwrap()];
    assert!(matches!(
        input.read_region_into(roi, &mut read),
        Err(Error::InvalidRegion { axis: "y", .. })
    ));

    let roi = spec.data_window().unwrap().with_channels(2..6).unwrap();
    let mut read = vec![0.0_f32; roi.element_count().unwrap()];
    assert!(matches!(
        input.read_region_into(roi, &mut read),
        Err(Error::InvalidRoi(_))
    ));
}

#[test]
fn rejects_a_buffer_that_does_not_match_the_region() {
    let scratch = ScratchDir::new("badbuffer");
    let (path, spec, _) = scanline_fixture(&scratch);

    let mut input = ImageInput::from_path(&path).unwrap();
    let roi = spec.data_window().unwrap().with_y(0..4).unwrap();

    let mut short = vec![0.0_f32; roi.element_count().unwrap() - 1];
    assert!(matches!(
        input.read_region_into(roi, &mut short),
        Err(Error::BufferLength { .. })
    ));

    // A failed read leaves the destination untouched.
    let mut long = vec![7.0_f32; roi.element_count().unwrap() + 1];
    assert!(matches!(
        input.read_region_into(roi, &mut long),
        Err(Error::BufferLength { .. })
    ));
    assert!(long.iter().all(|&value| value == 7.0));
}

#[test]
fn a_whole_window_region_matches_the_whole_image_read() {
    let scratch = ScratchDir::new("wholewindow");
    let (path, spec, whole) = scanline_fixture(&scratch);

    let mut input = ImageInput::from_path(&path).unwrap();
    let roi = spec.data_window().unwrap();
    let mut read = vec![0.0_f32; roi.element_count().unwrap()];
    input.read_region_into(roi, &mut read).unwrap();
    input.close().unwrap();

    assert_eq!(read, whole);
}
