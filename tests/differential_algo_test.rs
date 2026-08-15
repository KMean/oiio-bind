//! The safe algo layer compared against the raw sys bridge, bit for bit.
//!
//! Each operation runs twice on identically constructed inputs — once
//! through `oiio::algo`, once through the raw `oiio-sys` calls — and every
//! result value must match to the last bit. This proves the safe layer's
//! glue transparent: region conversion, operand marshalling, and result
//! plumbing add nothing and lose nothing.
//!
//! Scope, stated honestly: the raw arm goes through the same C++ shims the
//! safe arm uses, so a bug inside a shim is invisible to this comparison —
//! the NDC wiring defect the fifth review caught lived exactly there, and
//! only a behavioural expectation caught it. Shim semantics stay covered by
//! the hand-computed expectations in the algo tests; this file covers the
//! layer above them.

mod common;

use oiio::{algo, ImageBuf, ImageSpec, PixelFormat};
use oiio_sys::imagebuf::{self as sysbuf, InitializePixels, WrapMode};
use oiio_sys::imagebufalgo as sysalgo;
use oiio_sys::imageio as sysio;
use oiio_sys::typedesc::{typedesc_from_basetype_arraylen, BaseType};

const WIDTH: i32 = 16;
const HEIGHT: i32 = 12;
const CHANNELS: i32 = 4;

/// A deterministic value for one channel of one pixel.
fn value_at(x: i32, y: i32, channel: i32) -> f32 {
    ((x * 31 + y * 7 + channel * 13) % 97) as f32 / 97.0
}

/// The safe arm's input, filled pixel by pixel.
fn safe_input() -> ImageBuf {
    let spec = ImageSpec::new(
        WIDTH as u32,
        HEIGHT as u32,
        CHANNELS as u32,
        PixelFormat::F32,
    )
    .unwrap();
    let mut image = ImageBuf::new(&spec).unwrap();
    for y in 0..HEIGHT {
        for x in 0..WIDTH {
            let pixel: Vec<f32> = (0..CHANNELS).map(|c| value_at(x, y, c)).collect();
            image.set_pixel_at(x, y, &pixel).unwrap();
        }
    }
    image
}

/// The raw arm's input, constructed through the sys bridge alone.
fn raw_input() -> cxx::UniquePtr<sysbuf::ImageBuf> {
    let float = typedesc_from_basetype_arraylen(BaseType::Float32, 0);
    let spec = sysio::imagespec_new(WIDTH, HEIGHT, CHANNELS, float);
    let mut buffer = sysbuf::imagebuf_new_from_spec(spec.as_ref().unwrap(), InitializePixels::Yes);
    for y in 0..HEIGHT {
        for x in 0..WIDTH {
            let pixel: Vec<f32> = (0..CHANNELS).map(|c| value_at(x, y, c)).collect();
            sysbuf::imagebuf_setpixel(buffer.pin_mut(), x, y, 0, &pixel);
        }
    }
    buffer
}

/// An empty raw destination, as `ImageBuf::empty` makes for the safe arm.
fn raw_empty() -> cxx::UniquePtr<sysbuf::ImageBuf> {
    sysbuf::imagebuf_default()
}

/// Compare every value of the two results bit for bit.
fn assert_bit_identical(operation: &str, safe: &ImageBuf, raw: &cxx::UniquePtr<sysbuf::ImageBuf>) {
    let spec = safe.spec().unwrap();
    let raw_ref = raw.as_ref().unwrap();
    assert_eq!(
        spec.channel_count() as i32,
        sysbuf::imagebuf_nchannels(raw_ref),
        "{operation}: channel counts differ"
    );
    let window = spec.data_window().unwrap();
    for y in window.y() {
        for x in window.x() {
            for channel in 0..spec.channel_count() {
                let ours = safe.channel_at(x, y, channel, oiio::Wrap::Black).unwrap();
                let theirs = sysbuf::imagebuf_getchannel(
                    raw_ref,
                    x,
                    y,
                    0,
                    channel as i32,
                    WrapMode::WrapBlack,
                );
                assert_eq!(
                    ours.to_bits(),
                    theirs.to_bits(),
                    "{operation}: ({x},{y}) channel {channel}: {ours} vs {theirs}"
                );
            }
        }
    }
}

#[test]
fn premult_over_flip_crop_and_resize_are_bit_identical() {
    let safe_src = safe_input();
    let raw_src = raw_input();
    let all = sysio::roi_default();

    // premult.
    let mut safe_dst = ImageBuf::empty().unwrap();
    algo::premult(&mut safe_dst, &safe_src, None).unwrap();
    let mut raw_dst = raw_empty();
    assert!(unsafe {
        sysalgo::imagebufalgo_premult(raw_dst.pin_mut(), raw_src.as_ref().unwrap(), &all, 1)
    });
    assert_bit_identical("premult", &safe_dst, &raw_dst);

    // over, compositing the input over its own premultiplied version.
    let mut safe_over = ImageBuf::empty().unwrap();
    algo::over(&mut safe_over, &safe_src, &safe_dst, None).unwrap();
    let mut raw_over = raw_empty();
    assert!(unsafe {
        sysalgo::imagebufalgo_over(
            raw_over.pin_mut(),
            raw_src.as_ref().unwrap(),
            raw_dst.as_ref().unwrap(),
            &all,
            1,
        )
    });
    assert_bit_identical("over", &safe_over, &raw_over);

    // flip.
    let mut safe_flip = ImageBuf::empty().unwrap();
    algo::flip(&mut safe_flip, &safe_src, None).unwrap();
    let mut raw_flip = raw_empty();
    assert!(unsafe {
        sysalgo::imagebufalgo_flip(raw_flip.pin_mut(), raw_src.as_ref().unwrap(), &all, 1)
    });
    assert_bit_identical("flip", &safe_flip, &raw_flip);

    // crop, an interior window.
    let region = oiio::Roi::new(2..10, 3..9, 0..1, 0..CHANNELS as u32).unwrap();
    let mut safe_crop = ImageBuf::empty().unwrap();
    algo::crop(&mut safe_crop, &safe_src, Some(region)).unwrap();
    let raw_region = sysio::roi_new(2, 10, 3, 9, 0, 1, 0, CHANNELS);
    let mut raw_crop = raw_empty();
    assert!(unsafe {
        sysalgo::imagebufalgo_crop(
            raw_crop.pin_mut(),
            raw_src.as_ref().unwrap(),
            &raw_region,
            1,
        )
    });
    assert_bit_identical("crop", &safe_crop, &raw_crop);

    // resize to half size with an explicit filter, the deterministic kernel.
    let half = oiio::Roi::new(0..WIDTH / 2, 0..HEIGHT / 2, 0..1, 0..CHANNELS as u32).unwrap();
    let mut safe_resize = ImageBuf::empty().unwrap();
    algo::resize(
        &mut safe_resize,
        &safe_src,
        Some("triangle"),
        None,
        Some(half),
    )
    .unwrap();
    let raw_half = sysio::roi_new(0, WIDTH / 2, 0, HEIGHT / 2, 0, 1, 0, CHANNELS);
    let mut raw_resize = raw_empty();
    assert!(unsafe {
        sysalgo::imagebufalgo_resize(
            raw_resize.pin_mut(),
            raw_src.as_ref().unwrap(),
            "triangle",
            0.0,
            &raw_half,
            1,
        )
    });
    assert_bit_identical("resize", &safe_resize, &raw_resize);
}
