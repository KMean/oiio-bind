/// imageio tests
use anyhow::Result;
use oiio::Error;
use std::path::{Path, PathBuf};

fn fixture_path(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/images")
        .join(name)
}

#[test]
fn test_openimageio_version() {
    let runtime_version = oiio_sys::imageio::openimageio_version();
    let build_version = oiio_sys::imageio::openimageio_build_version();
    assert_eq!(
        runtime_version / 100,
        build_version / 100,
        "OpenImageIO headers and runtime library must use the same major/minor line"
    );
    assert!(
        runtime_version >= build_version,
        "the OpenImageIO runtime must not be older than the build headers"
    );
    assert!(
        (30_104..30_200).contains(&runtime_version),
        "this oiio-bind baseline targets OpenImageIO 3.1.x, found {runtime_version}"
    );
}

/// oiio_sys::imageio tests for basic functionality
#[test]
fn test_imageinput_open_without_config() -> Result<()> {
    let _ = oiio_sys::imageio::get_error(true);

    let bad_filename = fixture_path("does-not-exist.png");
    let imageio = oiio_sys::imageio::imageinput_open_without_config(
        bad_filename.to_str().expect("test path must be UTF-8"),
    )?;
    assert!(imageio.is_null());
    assert!(oiio_sys::imageio::has_error());

    let error = oiio_sys::imageio::get_error(true);
    assert!(!error.is_empty());
    assert!(!oiio_sys::imageio::has_error()); // State must be error-free after get_error(true).

    let filename = fixture_path("test16.png");
    let imageio = oiio_sys::imageio::imageinput_open_without_config(
        filename.to_str().expect("test path must be UTF-8"),
    )?;
    assert!(!imageio.is_null());

    Ok(())
}

#[test]
fn test_safe_imageinput_opens_png() -> Result<()> {
    let mut input = oiio::ImageInput::from_path(&fixture_path("test16.png"))?;

    assert_eq!(input.format_name(), "png");
    let spec = input.image_spec()?;
    assert_eq!(spec.dimensions(), [16, 16, 1]);
    assert_eq!(spec.channel_count(), 4);
    assert_eq!(spec.channel_names(), ["R", "G", "B", "A"]);
    assert_eq!(spec.alpha_channel(), Some(3));
    input.set_threads(2);
    assert_eq!(input.threads(), 2);

    Ok(())
}

#[test]
fn test_safe_imageinput_reports_missing_file() {
    let result = oiio::ImageInput::from_path(&fixture_path("does-not-exist.png"));
    assert!(result.is_err());
}

#[test]
fn safe_imageinput_decodes_exact_u16_pixels() -> Result<()> {
    let mut input = oiio::ImageInput::from_path(&fixture_path("test16.png"))?;
    let spec = input.image_spec_at(0, 0)?;
    let mut pixels = vec![0_u16; spec.element_count()?];

    input.read_image_into(&mut pixels)?;

    assert_pixel(&pixels, 0, 0, [65_535, 0, 0, 65_535]);
    assert_pixel(&pixels, 1, 0, [61_166, 4_369, 0, 65_535]);
    assert_pixel(&pixels, 15, 0, [0, 65_535, 0, 65_535]);
    assert_pixel(&pixels, 0, 15, [0, 0, 65_535, 65_535]);
    assert_pixel(&pixels, 15, 15, [65_535, 65_535, 65_535, 65_535]);

    input.close()?;
    Ok(())
}

#[test]
fn safe_imageinput_rejects_wrong_buffer_without_writing() -> Result<()> {
    let mut input = oiio::ImageInput::from_path(&fixture_path("test16.png"))?;
    let sentinel = 0xA55A_u16;
    let mut storage = vec![sentinel; 1_040];

    let error = input
        .read_image_into(&mut storage[..1_023])
        .expect_err("an undersized buffer must be rejected");
    assert!(matches!(
        error,
        Error::BufferLength {
            expected: 1_024,
            actual: 1_023
        }
    ));
    assert!(storage.iter().all(|&value| value == sentinel));

    let error = input
        .read_image_into(&mut storage[..1_025])
        .expect_err("an oversized buffer must be rejected");
    assert!(matches!(
        error,
        Error::BufferLength {
            expected: 1_024,
            actual: 1_025
        }
    ));
    assert!(storage.iter().all(|&value| value == sentinel));

    Ok(())
}

#[test]
fn sys_bounded_read_defensively_rejects_short_byte_slice() -> Result<()> {
    let path = fixture_path("test16.png");
    let mut input = oiio_sys::imageio::imageinput_open_without_config(
        path.to_str().expect("test path must be UTF-8"),
    )?;
    let sentinel = 0xA5A5_u16;
    let mut storage = vec![sentinel; 1_024];
    let bytes = unsafe { std::slice::from_raw_parts_mut(storage.as_mut_ptr().cast::<u8>(), 2_048) };
    let format = oiio_sys::typedesc::typedesc_from_basetype_arraylen(
        oiio_sys::typedesc::BaseType::Uint16,
        0,
    );

    let succeeded = unsafe {
        oiio_sys::imageio::imageinput_read_image_span(
            input.pin_mut(),
            0,
            0,
            0,
            4,
            format,
            &mut bytes[..2_046],
        )
    };

    assert!(!succeeded);
    assert!(storage.iter().all(|&value| value == sentinel));
    Ok(())
}

fn assert_pixel(pixels: &[u16], x: usize, y: usize, expected: [u16; 4]) {
    let offset = (y * 16 + x) * 4;
    assert_eq!(pixels[offset..offset + 4], expected);
}
