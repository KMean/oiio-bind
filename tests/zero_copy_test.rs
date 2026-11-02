//! Borrowing an `ImageBuf`'s pixels instead of copying them.
//!
//! The interesting half of these is the refusals. A zero-copy view is only
//! sound when the buffer owns contiguous, already-read pixels of exactly the
//! requested type, and each of those conditions has a test that it is checked
//! rather than assumed.

mod common;

use common::{f32_ramp, write_image, ScratchDir};
use oiio::{f16, ImageBuf, ImageSpec, PixelFormat};

#[test]
fn borrowed_pixels_are_the_same_values_a_copy_would_give() {
    let spec = ImageSpec::new(8, 4, 3, PixelFormat::F32).unwrap();
    let roi = spec.data_window().unwrap();
    let expected = f32_ramp(roi.element_count().unwrap());

    let mut image = ImageBuf::new(&spec).unwrap();
    image.set_pixels(roi, &expected).unwrap();

    let borrowed: &[f32] = image.pixels().unwrap();
    assert_eq!(borrowed.len(), expected.len());
    assert_eq!(borrowed, expected.as_slice());

    // And the same values the copying path reports, so the layout assumption
    // is right rather than merely self-consistent.
    let mut copied = vec![0.0f32; expected.len()];
    image.get_pixels_into(roi, &mut copied).unwrap();
    assert_eq!(borrowed, copied.as_slice());
}

#[test]
fn borrowing_really_is_the_buffers_own_memory() {
    let spec = ImageSpec::new(4, 2, 3, PixelFormat::F32).unwrap();
    let roi = spec.data_window().unwrap();
    let mut image = ImageBuf::new(&spec).unwrap();
    image
        .set_pixels(roi, &vec![1.0f32; roi.element_count().unwrap()])
        .unwrap();

    // Writing through the mutable view must be visible to every other route
    // into the image. If it were a copy, this would not show up.
    {
        let pixels: &mut [f32] = image.pixels_mut().unwrap();
        pixels[0] = 42.0;
        pixels[5] = -7.5;
    }

    let mut copied = vec![0.0f32; roi.element_count().unwrap()];
    image.get_pixels_into(roi, &mut copied).unwrap();
    assert_eq!(copied[0], 42.0);
    assert_eq!(copied[5], -7.5);
    assert_eq!(image.channel_at(0, 0, 0, oiio::Wrap::Black).unwrap(), 42.0);
}

#[test]
fn a_file_backed_buffer_can_be_borrowed_after_it_is_read() {
    let scratch = ScratchDir::new("zerocopy");
    let path = scratch.file("ramp.exr");
    let spec = ImageSpec::new(6, 3, 3, PixelFormat::F32).unwrap();
    let roi = spec.data_window().unwrap();
    let written = f32_ramp(roi.element_count().unwrap());
    write_image(&path, &spec, &written).unwrap();

    let mut image = ImageBuf::from_path(&path).unwrap();
    image.read().unwrap();

    let borrowed: &[f32] = image.pixels().unwrap();
    assert_eq!(borrowed.len(), written.len());
    assert_eq!(borrowed, written.as_slice());
}

#[test]
fn the_wrong_type_is_refused_rather_than_reinterpreted() {
    let spec = ImageSpec::new(4, 2, 3, PixelFormat::F32).unwrap();
    let image = ImageBuf::new(&spec).unwrap();

    // Nothing is converted when borrowing, so asking for f16 over an f32
    // buffer must fail. Reinterpreting would read two pixels per element and
    // report garbage, or worse, run off the end.
    let error = image
        .pixels::<f16>()
        .expect_err("a half view of a float buffer must be refused");
    let text = error.to_string();
    assert!(
        text.contains("F32") && text.contains("get_pixels_into"),
        "the error should say what it holds and what to do instead: {text}"
    );

    // u8 likewise, where the element size differs the other way.
    assert!(image.pixels::<u8>().is_err());

    // The matching type still works, so the check is on the type rather than
    // on borrowing itself.
    assert!(image.pixels::<f32>().is_ok());
}

#[test]
fn a_deep_buffer_is_refused() {
    let spec = ImageSpec::new(2, 2, 1, PixelFormat::F32)
        .unwrap()
        .with_channel_names(["Z"])
        .unwrap()
        .as_deep();
    let image = ImageBuf::new(&spec).unwrap();

    let error = image
        .pixels::<f32>()
        .expect_err("a deep image has samples, not pixels");
    assert!(
        error.to_string().contains("deep"),
        "the error should say so: {error}"
    );
}

#[test]
fn an_unread_buffer_is_refused_rather_than_lending_uninitialised_memory() {
    // An ImageBuf with no spec and no pixels at all.
    let image = ImageBuf::empty().unwrap();
    let error = image
        .pixels::<f32>()
        .expect_err("there is nothing here to borrow");
    // Whichever check catches it first, it must be an error and not a slice.
    assert!(!error.to_string().is_empty());
}

#[test]
fn the_borrow_keeps_the_buffer_immutable_for_its_lifetime() {
    // This is the property the safety argument rests on, and it is enforced by
    // the compiler rather than at run time. The following must not compile:
    //
    //     let pixels = image.pixels::<f32>().unwrap();
    //     image.read().unwrap();          // &mut while &  -> E0502
    //     println!("{}", pixels[0]);
    //
    // so the test is that the borrow ends before the mutation, and that the
    // mutation is then visible.
    let spec = ImageSpec::new(4, 2, 3, PixelFormat::F32).unwrap();
    let roi = spec.data_window().unwrap();
    let mut image = ImageBuf::new(&spec).unwrap();
    image
        .set_pixels(roi, &vec![3.0f32; roi.element_count().unwrap()])
        .unwrap();

    let first = image.pixels::<f32>().unwrap()[0];
    assert_eq!(first, 3.0);

    image
        .set_pixels(roi, &vec![9.0f32; roi.element_count().unwrap()])
        .unwrap();
    assert_eq!(image.pixels::<f32>().unwrap()[0], 9.0);
}

#[test]
fn borrowing_costs_nothing_for_a_large_image() {
    // The point of the exercise: a frame that would be expensive to copy.
    // 1024x1024 RGBA f32 is 16 MB, which get_pixels_into would allocate and
    // memcpy and this does not.
    let spec = ImageSpec::new(1024, 1024, 4, PixelFormat::F32).unwrap();
    let image = ImageBuf::new(&spec).unwrap();

    let pixels: &[f32] = image.pixels().unwrap();
    assert_eq!(pixels.len(), 1024 * 1024 * 4);
    // Freshly allocated buffers are zeroed, which is also a check that the
    // length is not merely plausible but addresses real memory throughout.
    assert_eq!(pixels[0], 0.0);
    assert_eq!(pixels[pixels.len() - 1], 0.0);
}
