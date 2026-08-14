//! ImageBuf: allocation, lazy file attachment, pixel transfer, and writing.

mod common;

use common::{f16_ramp, f32_ramp, write_image, ScratchDir};
use oiio::{f16, Error, ImageBuf, ImageSpec, PixelFormat, Storage};

fn fixture(scratch: &ScratchDir, name: &str) -> (std::path::PathBuf, ImageSpec, Vec<f32>) {
    let path = scratch.file(name);
    let spec = ImageSpec::new(12, 8, 3, PixelFormat::F32).unwrap();
    let pixels = f32_ramp(spec.element_count().unwrap());
    write_image(&path, &spec, &pixels).unwrap();
    (path, spec, pixels)
}

#[test]
fn allocates_a_zeroed_image() {
    let spec = ImageSpec::new(6, 4, 3, PixelFormat::F32).unwrap();
    let image = ImageBuf::new(&spec).unwrap();

    assert!(image.is_initialized());
    assert_eq!(image.storage(), Storage::Local);
    assert_eq!(image.channel_count(), 3);
    assert_eq!(image.spec().unwrap().dimensions(), [6, 4, 1]);

    let roi = spec.data_window().unwrap();
    let mut pixels = vec![1.0_f32; roi.element_count().unwrap()];
    image.get_pixels_into(roi, &mut pixels).unwrap();
    assert!(pixels.iter().all(|&value| value == 0.0));
}

#[test]
fn attaching_to_a_file_does_not_read_pixels_until_asked() {
    let scratch = ScratchDir::new("buflazy");
    let (path, spec, written) = fixture(&scratch, "image.exr");

    let mut image = ImageBuf::from_path(&path).unwrap();
    // The specification is available without having read any pixels.
    assert!(image.is_initialized());
    assert_eq!(image.spec().unwrap().dimensions(), spec.dimensions());
    assert_eq!(image.file_format_name(), "openexr");
    assert!(image.name().contains("image.exr"));

    image.read().unwrap();
    assert_ne!(image.storage(), Storage::Uninitialized);

    let roi = spec.data_window().unwrap();
    let mut pixels = vec![0.0_f32; roi.element_count().unwrap()];
    image.get_pixels_into(roi, &mut pixels).unwrap();
    assert_eq!(pixels, written);
}

#[test]
fn round_trips_pixels_through_a_region() {
    let spec = ImageSpec::new(8, 8, 3, PixelFormat::F32).unwrap();
    let mut image = ImageBuf::new(&spec).unwrap();

    // Write a 4x4 block into the middle, then read back the whole image.
    let block = spec
        .data_window()
        .unwrap()
        .with_x(2..6)
        .unwrap()
        .with_y(2..6)
        .unwrap();
    let values: Vec<f32> = (0..block.element_count().unwrap())
        .map(|index| index as f32 + 1.0)
        .collect();
    image.set_pixels(block, &values).unwrap();

    let mut read_back = vec![0.0_f32; block.element_count().unwrap()];
    image.get_pixels_into(block, &mut read_back).unwrap();
    assert_eq!(read_back, values);

    // Everything outside the block is still zero.
    let whole = spec.data_window().unwrap();
    let mut all = vec![0.0_f32; whole.element_count().unwrap()];
    image.get_pixels_into(whole, &mut all).unwrap();
    let inside_count = block.element_count().unwrap();
    let nonzero = all.iter().filter(|&&value| value != 0.0).count();
    assert_eq!(nonzero, inside_count);
}

#[test]
fn converts_between_pixel_types_on_transfer() {
    let scratch = ScratchDir::new("bufconvert");
    let path = scratch.file("half.exr");

    let spec = ImageSpec::new(4, 4, 3, PixelFormat::F16).unwrap();
    let written = f16_ramp(spec.element_count().unwrap());
    write_image(&path, &spec, &written).unwrap();

    let mut image = ImageBuf::from_path(&path).unwrap();
    image.read().unwrap();
    assert_eq!(image.spec().unwrap().format(), PixelFormat::F16);

    // Ask for float even though the image holds half; ImageBuf converts.
    let roi = spec.data_window().unwrap();
    let mut as_float = vec![0.0_f32; roi.element_count().unwrap()];
    image.get_pixels_into(roi, &mut as_float).unwrap();

    let expected: Vec<f32> = written.iter().map(|value| value.to_f32()).collect();
    assert_eq!(as_float, expected);
}

#[test]
fn reads_a_subimage_and_mip_level() {
    let scratch = ScratchDir::new("bufmip");
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

    let mut output = oiio::ImageOutput::create(&path, &levels[0]).unwrap();
    for (index, level) in levels.iter().enumerate() {
        if index > 0 {
            output.append_mip_level(level).unwrap();
        }
        output
            .write_image(&f16_ramp(level.element_count().unwrap()))
            .unwrap();
    }
    output.close().unwrap();

    // The 8x8 level, not the base image.
    let mut image = ImageBuf::from_path_at(&path, 0, 1).unwrap();
    image.read_at(0, 1, None).unwrap();
    assert_eq!(image.spec().unwrap().dimensions(), [8, 8, 1]);

    let expected = f16_ramp(levels[1].element_count().unwrap());
    let roi = image.spec().unwrap().data_window().unwrap();
    let mut pixels = vec![f16::ZERO; roi.element_count().unwrap()];
    image.get_pixels_into(roi, &mut pixels).unwrap();
    assert_eq!(pixels, expected);
}

#[test]
fn writes_an_image_to_a_file() {
    let scratch = ScratchDir::new("bufwrite");
    let path = scratch.file("written.exr");

    let spec = ImageSpec::new(5, 3, 3, PixelFormat::F32).unwrap();
    let mut image = ImageBuf::new(&spec).unwrap();
    let roi = spec.data_window().unwrap();
    let values = f32_ramp(roi.element_count().unwrap());
    image.set_pixels(roi, &values).unwrap();

    image.write(&path).unwrap();
    assert!(path.exists());

    // Read it back through a fresh buffer.
    let mut read_back = ImageBuf::from_path(&path).unwrap();
    read_back.read().unwrap();
    let mut pixels = vec![0.0_f32; roi.element_count().unwrap()];
    read_back.get_pixels_into(roi, &mut pixels).unwrap();
    assert_eq!(pixels, values);
}

#[test]
fn writes_in_a_chosen_pixel_format() {
    let scratch = ScratchDir::new("bufformat");
    let path = scratch.file("half.exr");

    let spec = ImageSpec::new(4, 4, 3, PixelFormat::F32).unwrap();
    let mut image = ImageBuf::new(&spec).unwrap();
    image.write_as(&path, Some(PixelFormat::F16)).unwrap();

    let read_back = ImageBuf::from_path(&path).unwrap();
    assert_eq!(read_back.spec().unwrap().format(), PixelFormat::F16);
}

#[test]
fn point_access_reads_writes_and_interpolates() {
    use oiio::Wrap;

    let spec = ImageSpec::new(4, 4, 3, PixelFormat::F32).unwrap();
    let mut buffer = ImageBuf::new(&spec).unwrap();
    buffer.set_pixel_at(1, 1, &[0.25, 0.5, 0.75]).unwrap();

    // Point reads, inside and outside the window.
    assert_eq!(buffer.channel_at(1, 1, 1, Wrap::Default).unwrap(), 0.5);
    assert_eq!(
        buffer.channel_at(100, 100, 1, Wrap::Black).unwrap(),
        0.0,
        "outside with black wrap is zero"
    );

    let mut pixel = [0.0_f32; 3];
    buffer.pixel_at_into(1, 1, Wrap::Clamp, &mut pixel).unwrap();
    assert_eq!(pixel, [0.25, 0.5, 0.75]);

    // A write outside the window is an error, not a silent no-op.
    assert!(buffer.set_pixel_at(10, 10, &[1.0, 1.0, 1.0]).is_err());

    // Interpolation midway between the written pixel and its zero neighbour.
    let mut mid = [0.0_f32; 3];
    buffer
        .interpolated_pixel_into(2.0, 1.5, Wrap::Black, &mut mid)
        .unwrap();
    assert!(
        mid[1] > 0.0 && mid[1] < 0.5,
        "halfway out of the bright pixel: {mid:?}"
    );

    // The slice must measure the channel count exactly.
    let mut short = [0.0_f32; 2];
    assert!(buffer
        .interpolated_pixel_into(2.0, 1.5, Wrap::Black, &mut short)
        .is_err());

    // NDC addressing spans the display window.
    let mut centre = [0.0_f32; 3];
    buffer
        .interpolated_pixel_ndc_into(0.375, 0.375, Wrap::Black, &mut centre)
        .unwrap();

    // Periodic wrap needs a real display window; a deep buffer is refused
    // outright.
    let deep_spec = ImageSpec::new(2, 2, 1, PixelFormat::F32).unwrap().as_deep();
    let deep = ImageBuf::new(&deep_spec).unwrap();
    assert!(deep.channel_at(0, 0, 0, Wrap::Black).is_err());
}

#[test]
fn metadata_setters_move_windows_and_merge_attributes() {
    let spec = ImageSpec::new(4, 4, 3, PixelFormat::F32).unwrap();
    let mut buffer = ImageBuf::new(&spec).unwrap();

    // Move the data window; the display window follows the setter.
    buffer.set_origin([10, 20, 0]);
    assert_eq!(buffer.spec().unwrap().origin(), [10, 20, 0]);

    buffer.set_full_window([0, 0, 0], [64, 32, 1]).unwrap();
    assert_eq!(buffer.spec().unwrap().full_dimensions(), [64, 32, 1]);

    // A zero-sized display window is refused before OpenImageIO stores it.
    assert!(buffer.set_full_window([0, 0, 0], [0, 32, 1]).is_err());

    let display = oiio::Roi::new(0..128, 0..64, 0..1, 0..3).unwrap();
    buffer.set_display_window(display);
    assert_eq!(buffer.spec().unwrap().full_dimensions(), [128, 64, 1]);

    buffer.set_orientation(6).unwrap();
    assert!(buffer.set_orientation(9).is_err(), "EXIF stops at 8");

    // Metadata copies and merges between buffers.
    let mut tagged = ImageBuf::new(&spec).unwrap();
    let source = ImageBuf::new(
        &spec
            .clone()
            .with_attribute("Software", "oiio-bind")
            .with_attribute("Artist", "Kim"),
    )
    .unwrap();
    tagged.copy_metadata(&source);
    assert!(tagged.spec().unwrap().attribute("Software").is_some());

    let mut merged = ImageBuf::new(&spec).unwrap();
    merged.merge_metadata(&source, false, "^Artist$").unwrap();
    let spec_after = merged.spec().unwrap();
    assert!(spec_after.attribute("Artist").is_some());
    assert!(
        spec_after.attribute("Software").is_none(),
        "the pattern selected only Artist"
    );

    // An invalid pattern is an error, not a regex_error abort.
    assert!(merged.merge_metadata(&source, false, "[unclosed").is_err());
}

#[test]
fn buffer_state_reports_subimage_mip_and_threads() {
    let mut buffer = ImageBuf::new(&ImageSpec::new(4, 4, 3, PixelFormat::F32).unwrap()).unwrap();
    assert_eq!(buffer.subimage(), 0, "a spec-built buffer is subimage 0");
    assert_eq!(buffer.mip_level(), 0);

    assert_eq!(buffer.threads(), 0, "zero means the global default");
    buffer.set_threads(2).unwrap();
    assert_eq!(buffer.threads(), 2);
    buffer.set_threads(0).unwrap();
    assert_eq!(buffer.threads(), 0);
}

#[test]
fn cloning_copies_the_pixels() {
    let spec = ImageSpec::new(4, 4, 3, PixelFormat::F32).unwrap();
    let mut original = ImageBuf::new(&spec).unwrap();
    let roi = spec.data_window().unwrap();
    let values = f32_ramp(roi.element_count().unwrap());
    original.set_pixels(roi, &values).unwrap();

    let mut copy = original.clone();

    // Changing the copy must not change the original.
    let zeros = vec![0.0_f32; roi.element_count().unwrap()];
    copy.set_pixels(roi, &zeros).unwrap();

    let mut from_original = vec![0.0_f32; roi.element_count().unwrap()];
    original.get_pixels_into(roi, &mut from_original).unwrap();
    assert_eq!(from_original, values);

    let mut from_copy = vec![1.0_f32; roi.element_count().unwrap()];
    copy.get_pixels_into(roi, &mut from_copy).unwrap();
    assert!(from_copy.iter().all(|&value| value == 0.0));

    // try_clone is the same copy with the failure reportable: OpenImageIO's
    // copy constructor swallows its own allocation failure and hands back a
    // copy whose first read would crash, so the crate checks for it.
    let fallible = original.try_clone().unwrap();
    let mut from_fallible = vec![0.0_f32; roi.element_count().unwrap()];
    fallible.get_pixels_into(roi, &mut from_fallible).unwrap();
    assert_eq!(from_fallible, values);
}

#[test]
fn rejects_a_buffer_that_does_not_match_the_region() {
    let spec = ImageSpec::new(4, 4, 3, PixelFormat::F32).unwrap();
    let mut image = ImageBuf::new(&spec).unwrap();
    let roi = spec.data_window().unwrap();

    let mut short = vec![0.0_f32; roi.element_count().unwrap() - 1];
    assert!(matches!(
        image.get_pixels_into(roi, &mut short),
        Err(Error::BufferLength { .. })
    ));

    let long = vec![0.0_f32; roi.element_count().unwrap() + 1];
    assert!(matches!(
        image.set_pixels(roi, &long),
        Err(Error::BufferLength { .. })
    ));
}

#[test]
fn reports_a_missing_file() {
    let scratch = ScratchDir::new("bufmissing");
    let missing = scratch.file("does-not-exist.exr");
    assert!(matches!(
        ImageBuf::from_path(&missing),
        Err(Error::OpenImage { .. })
    ));
}

/// Regression test: `name()` and `file_format_name()` return a borrowed
/// string. Both once pointed into a destroyed temporary, so they returned
/// whatever happened to be in freed memory. Reading each repeatedly, with
/// unrelated allocation in between, would surface that again.
#[test]
fn borrowed_names_stay_valid() {
    let scratch = ScratchDir::new("bufnames");
    let (path, _, _) = fixture(&scratch, "named.exr");

    let image = ImageBuf::from_path(&path).unwrap();
    let expected_format = "openexr";

    for _ in 0..64 {
        assert_eq!(image.file_format_name(), expected_format);
        assert!(image.name().contains("named.exr"));

        // Churn the allocator; a dangling borrow would be overwritten.
        let noise: Vec<String> = (0..32).map(|i| format!("scratch value {i}")).collect();
        assert_eq!(noise.len(), 32);

        assert_eq!(image.file_format_name(), expected_format);
        assert!(image.name().contains("named.exr"));
    }
}

#[test]
fn make_writable_promotes_a_file_backed_image() {
    let scratch = ScratchDir::new("bufwritable");
    let (path, spec, written) = fixture(&scratch, "image.exr");

    let mut image = ImageBuf::from_path(&path).unwrap();
    image.make_writable().unwrap();
    assert_eq!(image.storage(), Storage::Local);

    // It is now editable in place.
    let roi = spec.data_window().unwrap();
    let zeros = vec![0.0_f32; roi.element_count().unwrap()];
    image.set_pixels(roi, &zeros).unwrap();

    let mut pixels = vec![1.0_f32; roi.element_count().unwrap()];
    image.get_pixels_into(roi, &mut pixels).unwrap();
    assert!(pixels.iter().all(|&value| value == 0.0));
    assert_ne!(pixels, written);
}
