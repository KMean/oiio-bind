//! The image values are `Send`: each one is built on one thread, moved to
//! another, used there, and dropped there.
//!
//! A static bound would prove the impl exists. These tests exercise what the
//! impl actually claims, which is that the C++ object behind the pointer has
//! no thread affinity: the work and the destructor both run somewhere other
//! than where the value was made.
//!
//! None of these types is `Sync`, deliberately. There is no stable way to
//! assert a negative bound, so that side is documented in the comments above
//! each `unsafe impl` rather than tested here.

mod common;

use common::{f32_ramp, write_image, ScratchDir};
use oiio::{ColorConfig, DeepImage, ImageBuf, ImageInput, ImageOutput, ImageSpec, PixelFormat};
use std::thread;

#[test]
fn the_image_values_are_send() {
    fn assert_send<T: Send>() {}

    assert_send::<ImageBuf>();
    assert_send::<ImageInput>();
    assert_send::<ImageOutput>();
    assert_send::<DeepImage>();
    assert_send::<ColorConfig>();
}

#[test]
fn an_image_buf_survives_the_move_and_drops_on_the_far_thread() {
    let spec = ImageSpec::new(8, 4, 3, PixelFormat::F32).unwrap();
    let pixels = f32_ramp(spec.element_count().unwrap());
    let roi = spec.data_window().unwrap();
    let mut image = ImageBuf::new(&spec).unwrap();
    image.set_pixels(roi, &pixels).unwrap();

    // The buffer crosses here, is read on the far side, and is dropped there.
    let read_back = thread::spawn(move || {
        let mut out = vec![0.0f32; pixels.len()];
        image.get_pixels_into(roi, &mut out).unwrap();
        out
    })
    .join()
    .unwrap();

    assert_eq!(read_back, f32_ramp(read_back.len()));
}

#[test]
fn a_reader_opened_on_one_thread_reads_on_another() {
    let scratch = ScratchDir::new("send-reader");
    let path = scratch.file("send_reader.exr");
    let spec = ImageSpec::new(6, 4, 3, PixelFormat::F32).unwrap();
    let pixels = f32_ramp(spec.element_count().unwrap());
    write_image(&path, &spec, &pixels).unwrap();

    // Opened here, so the file handle and the format plugin's state were
    // created on this thread.
    let input = ImageInput::from_path(&path).unwrap();

    let got = thread::spawn(move || {
        let mut input = input;
        let mut out = vec![0.0f32; pixels.len()];
        input.read_image_into(&mut out).unwrap();
        // Closing by value on the far thread, which is what keeps the close
        // and the destructor together.
        input.close().unwrap();
        out
    })
    .join()
    .unwrap();

    assert_eq!(got, f32_ramp(got.len()));
}

#[test]
fn a_writer_opened_on_one_thread_finishes_on_another() {
    let scratch = ScratchDir::new("send-writer");
    let path = scratch.file("send_writer.exr");
    let spec = ImageSpec::new(6, 4, 3, PixelFormat::F32).unwrap();
    let pixels = f32_ramp(spec.element_count().unwrap());

    let output = ImageOutput::create(&path, &spec).unwrap();

    let moved = pixels.clone();
    thread::spawn(move || {
        let mut output = output;
        output.write_image(&moved).unwrap();
        output.close().unwrap();
    })
    .join()
    .unwrap();

    // The file is complete and correct, so the buffered state and the flush
    // both survived the move.
    let mut back = vec![0.0f32; pixels.len()];
    ImageInput::from_path(&path)
        .unwrap()
        .read_image_into(&mut back)
        .unwrap();
    assert_eq!(back, pixels);
}

#[test]
fn a_deep_image_survives_the_move() {
    let spec = ImageSpec::new(2, 2, 1, PixelFormat::F32)
        .unwrap()
        .with_channel_names(["Z"])
        .unwrap()
        .as_deep();
    let mut deep = DeepImage::new(&spec).unwrap();
    deep.set_sample_count(0, 0, 2).unwrap();
    deep.set_value(0, 0, 0, 0, 1.5).unwrap();
    deep.set_value(0, 0, 0, 1, 2.5).unwrap();

    let values = thread::spawn(move || {
        let first = deep.value(0, 0, 0, 0).unwrap();
        let second = deep.value(0, 0, 0, 1).unwrap();
        (first, second)
    })
    .join()
    .unwrap();

    assert_eq!(values, (1.5, 2.5));
}

#[test]
fn a_colour_config_survives_the_move() {
    let config = ColorConfig::new().unwrap();

    // The config is lazily built, so whether it was realised on this thread or
    // the far one, the first real use happens after the move.
    let count = thread::spawn(move || config.color_space_names().len())
        .join()
        .unwrap();

    assert!(count > 0, "a config with no colour spaces is not a config");
}

/// The point of all of this: a pool of threads each doing real work on its own
/// image. This is the pattern the crate's own domain assumes and the one that
/// did not compile before.
#[test]
fn images_can_be_built_on_a_pool_and_collected() {
    let specs: Vec<_> = (1..=4)
        .map(|n| ImageSpec::new(n * 2, 4, 3, PixelFormat::F32).unwrap())
        .collect();

    let workers: Vec<_> = specs
        .into_iter()
        .map(|spec| {
            thread::spawn(move || {
                let roi = spec.data_window().unwrap();
                let mut image = ImageBuf::new(&spec).unwrap();
                let pixels = f32_ramp(roi.element_count().unwrap());
                image.set_pixels(roi, &pixels).unwrap();
                image
            })
        })
        .collect();

    // Each image was made on its own thread and is dropped on this one, the
    // reverse of the moves above.
    let widths: Vec<u32> = workers
        .into_iter()
        .map(|w| w.join().unwrap().spec().unwrap().dimensions()[0])
        .collect();

    assert_eq!(widths, vec![2, 4, 6, 8]);
}
