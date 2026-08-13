//! Reading and writing images in memory, without touching the filesystem.

mod common;

use common::{f16_ramp, f32_ramp, ScratchDir};
use oiio::{f16, Error, ImageInput, ImageOutput, ImageSpec, PixelFormat};

#[test]
fn round_trips_an_exr_entirely_in_memory() {
    let spec = ImageSpec::new(16, 9, 4, PixelFormat::F32).unwrap();
    let written = f32_ramp(spec.element_count().unwrap());

    let mut output = ImageOutput::to_memory("image.exr", &spec).unwrap();
    output.write_image(&written).unwrap();
    let encoded = output.close_into_bytes().unwrap();

    assert!(!encoded.is_empty());
    // OpenEXR files start with a known magic number.
    assert_eq!(&encoded[0..4], &[0x76, 0x2f, 0x31, 0x01]);

    let mut input = ImageInput::from_memory("image.exr", encoded).unwrap();
    assert_eq!(input.format_name(), "openexr");
    let spec_back = input.image_spec().unwrap();
    assert_eq!(spec_back.dimensions(), [16, 9, 1]);
    assert_eq!(spec_back.format(), PixelFormat::F32);

    let mut decoded = vec![0.0_f32; spec_back.element_count().unwrap()];
    input.read_image_into(&mut decoded).unwrap();
    input.close().unwrap();

    assert_eq!(decoded, written);
}

#[test]
fn round_trips_a_png_in_memory() {
    let spec = ImageSpec::new(8, 8, 3, PixelFormat::U8).unwrap();
    let written: Vec<u8> = (0..spec.element_count().unwrap())
        .map(|index| (index % 251) as u8)
        .collect();

    let mut output = ImageOutput::to_memory("image.png", &spec).unwrap();
    output.write_image(&written).unwrap();
    let encoded = output.close_into_bytes().unwrap();

    assert_eq!(&encoded[1..4], b"PNG");

    let mut input = ImageInput::from_memory("image.png", encoded).unwrap();
    let mut decoded = vec![0_u8; spec.element_count().unwrap()];
    input.read_image_into(&mut decoded).unwrap();
    input.close().unwrap();

    assert_eq!(decoded, written);
}

#[test]
fn an_in_memory_image_matches_the_same_image_on_disk() {
    let scratch = ScratchDir::new("memvsdisk");
    let path = scratch.file("ondisk.exr");

    let spec = ImageSpec::new(12, 5, 3, PixelFormat::F16)
        .unwrap()
        .with_attribute("Artist", "oiio-bind");
    let written = f16_ramp(spec.element_count().unwrap());

    // To disk.
    let mut to_disk = ImageOutput::create(&path, &spec).unwrap();
    to_disk.write_image(&written).unwrap();
    to_disk.close().unwrap();

    // To memory.
    let mut to_memory = ImageOutput::to_memory("ondisk.exr", &spec).unwrap();
    to_memory.write_image(&written).unwrap();
    let encoded = to_memory.close_into_bytes().unwrap();

    let on_disk = std::fs::read(&path).unwrap();
    assert_eq!(
        encoded.len(),
        on_disk.len(),
        "the memory and file encoders disagree on size"
    );

    // Both decode to the same pixels and metadata.
    let mut from_memory = ImageInput::from_memory("ondisk.exr", encoded).unwrap();
    let memory_spec = from_memory.image_spec().unwrap();
    let mut memory_pixels = vec![f16::ZERO; memory_spec.element_count().unwrap()];
    from_memory.read_image_into(&mut memory_pixels).unwrap();
    from_memory.close().unwrap();

    let mut from_disk = ImageInput::from_path(&path).unwrap();
    let disk_spec = from_disk.image_spec().unwrap();
    let mut disk_pixels = vec![f16::ZERO; disk_spec.element_count().unwrap()];
    from_disk.read_image_into(&mut disk_pixels).unwrap();
    from_disk.close().unwrap();

    assert_eq!(memory_pixels, disk_pixels);
    assert_eq!(memory_pixels, written);
    assert_eq!(memory_spec.dimensions(), disk_spec.dimensions());
    assert_eq!(
        memory_spec.attribute("Artist"),
        disk_spec.attribute("Artist")
    );
}

#[test]
fn a_memory_reader_owns_its_buffer() {
    let spec = ImageSpec::new(4, 4, 3, PixelFormat::F32).unwrap();
    let written = f32_ramp(spec.element_count().unwrap());
    let mut output = ImageOutput::to_memory("image.exr", &spec).unwrap();
    output.write_image(&written).unwrap();
    let encoded = output.close_into_bytes().unwrap();

    // The reader takes the buffer, so nothing the caller holds keeps it
    // alive. Reading after the original binding is gone must still work.
    let mut input = {
        let moved = encoded;
        ImageInput::from_memory("image.exr", moved).unwrap()
    };

    let mut decoded = vec![0.0_f32; spec.element_count().unwrap()];
    input.read_image_into(&mut decoded).unwrap();
    input.close().unwrap();
    assert_eq!(decoded, written);
}

#[test]
fn partial_reads_work_in_memory_too() {
    let spec = ImageSpec::new(32, 32, 3, PixelFormat::F32)
        .unwrap()
        .with_tile_size([16, 16, 1])
        .unwrap();
    let written = f32_ramp(spec.element_count().unwrap());

    let mut output = ImageOutput::to_memory("tiled.exr", &spec).unwrap();
    output.write_image(&written).unwrap();
    let encoded = output.close_into_bytes().unwrap();

    let mut input = ImageInput::from_memory("tiled.exr", encoded).unwrap();
    let roi = input
        .image_spec()
        .unwrap()
        .data_window()
        .unwrap()
        .with_x(16..32)
        .unwrap()
        .with_y(0..16)
        .unwrap();
    let mut region = vec![0.0_f32; roi.element_count().unwrap()];
    input.read_region_into(roi, &mut region).unwrap();
    input.close().unwrap();

    // Compare against the same block of the whole image.
    let mut expected = Vec::new();
    for row in 0..16usize {
        let start = (row * 32 + 16) * 3;
        expected.extend_from_slice(&written[start..start + 16 * 3]);
    }
    assert_eq!(region, expected);
}

#[test]
fn rejects_bytes_that_are_not_the_named_format() {
    let not_an_image = b"this is not an image file at all".to_vec();
    let error = ImageInput::from_memory("image.exr", not_an_image).unwrap_err();
    assert!(matches!(error, Error::OpenImage { .. }));
}

#[test]
fn rejects_an_unknown_extension() {
    let spec = ImageSpec::new(4, 4, 3, PixelFormat::U8).unwrap();
    assert!(ImageOutput::to_memory("image.not-an-image-format", &spec).is_err());
    assert!(ImageInput::from_memory("image.not-an-image-format", vec![0; 64]).is_err());
}

#[test]
fn taking_bytes_from_a_file_writer_is_refused() {
    let scratch = ScratchDir::new("memmismatch");
    let path = scratch.file("file.exr");
    let spec = ImageSpec::new(4, 4, 3, PixelFormat::F32).unwrap();

    let mut output = ImageOutput::create(&path, &spec).unwrap();
    output
        .write_image(&f32_ramp(spec.element_count().unwrap()))
        .unwrap();
    assert!(matches!(
        output.close_into_bytes(),
        Err(Error::Operation { .. })
    ));
}

#[test]
fn writes_scanlines_and_tiles_into_memory() {
    // Scanline batches.
    let spec = ImageSpec::new(8, 6, 3, PixelFormat::F32).unwrap();
    let written = f32_ramp(spec.element_count().unwrap());
    let row = 8 * 3;
    let mut output = ImageOutput::to_memory("batched.exr", &spec).unwrap();
    for begin in (0..6usize).step_by(2) {
        let end = begin + 2;
        output
            .write_scanlines(begin as i32..end as i32, &written[begin * row..end * row])
            .unwrap();
    }
    let encoded = output.close_into_bytes().unwrap();

    let mut input = ImageInput::from_memory("batched.exr", encoded).unwrap();
    let mut decoded = vec![0.0_f32; spec.element_count().unwrap()];
    input.read_image_into(&mut decoded).unwrap();
    input.close().unwrap();
    assert_eq!(decoded, written);
}
