//! `premult` and `unpremult` on a source with no alpha channel are documented
//! to copy instead of dividing. That copy must carry the same channel guard a
//! direct `copy` does.
//!
//! The shim once called `OIIO::ImageBufAlgo::copy` directly, which routed
//! around the guard and made these two functions the only public way into
//! OpenImageIO's unchecked read past the source's pixel (reported upstream as
//! issue #5661). A one-channel source copied into a pre-allocated five-channel
//! destination returned success with the source's *following* pixels in the
//! channels it does not have, and read past the end of its allocation at the
//! last pixel.

mod common;

use oiio::{algo, ImageBuf, ImageSpec, PixelFormat, Roi};

const W: u32 = 4;
const H: u32 = 4;
const SENTINEL: f32 = -1.0;

/// A one-channel ramp: pixel (x, y) holds y * W + x, so 0..15 in raster order.
/// The distinct values are the point. A constant source would make an over-read
/// into the next pixel return that same constant and hide the defect.
fn ramp_1ch() -> ImageBuf {
    let spec = ImageSpec::new(W, H, 1, PixelFormat::F32).unwrap();
    let mut image = ImageBuf::new(&spec).unwrap();
    algo::zero(&mut image, None).unwrap();
    for y in 0..H as i32 {
        for x in 0..W as i32 {
            image
                .set_pixel_at(x, y, &[(y * W as i32 + x) as f32])
                .unwrap();
        }
    }
    image
}

fn sentinel_5ch() -> ImageBuf {
    let spec = ImageSpec::new(W, H, 5, PixelFormat::F32).unwrap();
    let mut image = ImageBuf::new(&spec).unwrap();
    algo::fill(&mut image, &[SENTINEL; 5], None).unwrap();
    image
}

fn pixel_5ch(image: &ImageBuf, x: i32, y: i32) -> [f32; 5] {
    let roi = Roi::new(x..x + 1, y..y + 1, 0..1, 0..5).unwrap();
    let mut pixel = [0.0_f32; 5];
    image.get_pixels_into(roi, &mut pixel).unwrap();
    pixel
}

#[test]
fn premult_into_a_wider_destination_is_refused_like_copy() {
    let src = ramp_1ch();
    assert_eq!(
        src.spec().unwrap().alpha_channel(),
        None,
        "the fixture must have no alpha channel, or premult takes its other branch"
    );

    // The guard a direct copy applies, for comparison.
    let mut copied = sentinel_5ch();
    let copy_err = algo::copy(&mut copied, &src, None, None)
        .expect_err("copy into a wider allocated destination is refused")
        .to_string();
    assert!(
        copy_err.contains("channels"),
        "copy's refusal should name the channel counts, got: {copy_err}"
    );

    for name in ["premult", "unpremult"] {
        let mut dst = sentinel_5ch();
        let result = match name {
            "premult" => algo::premult(&mut dst, &src, None),
            _ => algo::unpremult(&mut dst, &src, None),
        };
        let message = result
            .expect_err(&format!(
                "{name} on an alpha-less source copies, and that copy must be \
                 refused into a wider destination rather than reading past the \
                 source's pixel"
            ))
            .to_string();
        assert!(
            message.contains("channels"),
            "{name} should be refused for the same reason copy is, got: {message}"
        );
    }
}

#[test]
fn premult_and_unpremult_never_write_channels_the_source_lacks() {
    let src = ramp_1ch();

    for name in ["premult", "unpremult"] {
        let mut dst = sentinel_5ch();
        let result = match name {
            "premult" => algo::premult(&mut dst, &src, None),
            _ => algo::unpremult(&mut dst, &src, None),
        };

        // Either it refuses, or it leaves the channels the source does not
        // have exactly as it found them. What it must never do is fill them
        // from past the source's pixel.
        if result.is_ok() {
            for y in 0..H as i32 {
                for x in 0..W as i32 {
                    let p = pixel_5ch(&dst, x, y);
                    assert_eq!(
                        p[0],
                        (y * W as i32 + x) as f32,
                        "{name} at ({x},{y}): channel 0 should be the source"
                    );
                    for (c, &v) in p.iter().enumerate().skip(1) {
                        assert_eq!(
                            v, SENTINEL,
                            "{name} at ({x},{y}) channel {c} is {v}; the source \
                             has no channel {c}, so this came from past its pixel"
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn repremult_still_refuses_a_source_with_no_alpha() {
    let src = ramp_1ch();
    let mut dst = sentinel_5ch();
    let err = algo::repremult(&mut dst, &src, None)
        .expect_err("an image with no alpha cannot be re-premultiplied");
    let message = err.to_string();
    assert!(
        message.contains("alpha"),
        "the error should say why, got: {message}"
    );
}
