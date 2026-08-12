use std::{
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

use anyhow::Result;
use oiio::{Error, ImageInput, ImageOutput};

static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);

struct TempImage {
    path: PathBuf,
}

impl TempImage {
    fn new(extension: &str) -> Self {
        let sequence = TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let filename = format!(
            "oiio-bind-{}-{}.{}",
            std::process::id(),
            sequence,
            extension
        );
        Self {
            path: std::env::temp_dir().join(filename),
        }
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TempImage {
    fn drop(&mut self) {
        match std::fs::remove_file(&self.path) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) if !std::thread::panicking() => {
                panic!("failed to remove temporary image {:?}: {error}", self.path)
            }
            Err(_) => {}
        }
    }
}

#[test]
fn exact_u16_png_round_trip() -> Result<()> {
    let image = TempImage::new("png");
    let pixels = u16_pattern();

    let mut output = ImageOutput::create::<u16>(image.path(), 4, 3, 4)?;
    assert_eq!(output.format_name(), "png");
    assert_eq!(output.image_spec()?.dimensions(), [4, 3, 1]);
    assert_eq!(output.image_spec()?.channel_count(), 4);
    output.write_image(&pixels)?;
    output.close()?;

    let mut input = ImageInput::from_path(image.path())?;
    let mut decoded = vec![0_u16; pixels.len()];
    input.read_image_into(&mut decoded)?;
    input.close()?;
    assert_eq!(decoded, pixels);
    Ok(())
}

#[test]
fn exact_f32_exr_round_trip() -> Result<()> {
    let image = TempImage::new("exr");
    let pixels = f32_pattern();

    let mut output = ImageOutput::create::<f32>(image.path(), 4, 3, 4)?;
    output.write_image(&pixels)?;
    output.close()?;

    let mut input = ImageInput::from_path(image.path())?;
    let mut decoded = vec![0.0_f32; pixels.len()];
    input.read_image_into(&mut decoded)?;
    input.close()?;
    assert_eq!(decoded, pixels);
    Ok(())
}

#[test]
fn safe_output_rejects_short_and_long_buffers_before_valid_write() -> Result<()> {
    let image = TempImage::new("png");
    let mut output = ImageOutput::create::<u16>(image.path(), 4, 3, 4)?;
    let pixels = u16_pattern();

    let mut long = pixels.clone();
    long.push(0);
    for malformed in [&pixels[..pixels.len() - 1], long.as_slice()] {
        let actual = malformed.len();
        let error = output
            .write_image(malformed)
            .expect_err("a malformed output buffer must be rejected");
        assert!(matches!(
            error,
            Error::BufferLength {
                expected: 48,
                actual: got
            } if got == actual
        ));
    }

    output.write_image(&pixels)?;
    output.close()?;

    let mut input = ImageInput::from_path(image.path())?;
    let mut decoded = vec![0_u16; pixels.len()];
    input.read_image_into(&mut decoded)?;
    input.close()?;
    assert_eq!(decoded, pixels);
    Ok(())
}

#[test]
fn sys_bounded_output_rejects_short_and_long_byte_slices() -> Result<()> {
    let image = TempImage::new("png");
    let format = oiio_sys::typedesc::typedesc_from_basetype_arraylen(
        oiio_sys::typedesc::BaseType::Uint16,
        0,
    );
    let spec = oiio_sys::imageio::imagespec_from_resolution_format(4, 3, 4, format);
    let mut output = oiio_sys::imageio::imageoutput_create_without_ioproxy(
        image.path().to_str().expect("temporary path must be UTF-8"),
        "",
    );
    assert!(!output.is_null());
    assert!(oiio_sys::imageio::imageoutput_open(
        output.pin_mut(),
        image.path().to_str().expect("temporary path must be UTF-8"),
        spec.as_ref().expect("ImageSpec construction must succeed"),
        oiio_sys::imageio::OpenMode::Create,
    ));

    let pixels = u16_pattern();
    let exact_byte_len = std::mem::size_of_val(pixels.as_slice());
    let mut aligned_storage = pixels.clone();
    aligned_storage.push(0);
    for byte_len in [exact_byte_len - 1, exact_byte_len + 1] {
        // Preserve u16 alignment while deliberately varying the byte extent.
        let malformed =
            unsafe { std::slice::from_raw_parts(aligned_storage.as_ptr().cast::<u8>(), byte_len) };
        let succeeded = unsafe {
            oiio_sys::imageio::imageoutput_write_image_span(output.pin_mut(), format, malformed)
        };
        assert!(!succeeded);
        let error = oiio_sys::imageio::imageoutput_geterror(
            output.as_ref().expect("ImageOutput must remain alive"),
            true,
        );
        assert!(!error.is_empty());
        assert!(error.contains("buffer") || error.contains("layout"));
    }

    let aligned_exact = unsafe {
        std::slice::from_raw_parts(aligned_storage.as_ptr().cast::<u8>(), exact_byte_len)
    };
    assert!(unsafe {
        oiio_sys::imageio::imageoutput_write_image_span(output.pin_mut(), format, aligned_exact)
    });
    assert!(oiio_sys::imageio::imageoutput_close(output.pin_mut()));
    drop(output);

    let mut input = ImageInput::from_path(image.path())?;
    let mut decoded = vec![0_u16; pixels.len()];
    input.read_image_into(&mut decoded)?;
    input.close()?;
    assert_eq!(decoded, pixels);
    Ok(())
}

#[test]
fn output_reports_invalid_dimensions_extensions_and_paths() {
    for (width, height, channels) in [
        (0, 1, 1),
        (1, 0, 1),
        (1, 1, 0),
        (u32::MAX, 1, 1),
        (1, u32::MAX, 1),
        (1, 1, u32::MAX),
    ] {
        let image = TempImage::new("png");
        assert!(matches!(
            ImageOutput::create::<u8>(image.path(), width, height, channels),
            Err(Error::InvalidImageSpec(_))
        ));
    }

    let unsupported = TempImage::new("definitely-not-an-image-format");
    match ImageOutput::create::<u8>(unsupported.path(), 1, 1, 1) {
        Err(Error::OpenImage { message, .. }) => assert!(!message.is_empty()),
        Err(error) => panic!("unexpected unsupported-format error: {error}"),
        Ok(_) => panic!("an unsupported output format must fail"),
    }

    let missing_parent = TempImage::new("png");
    let missing_parent = missing_parent.path().with_file_name(format!(
        "oiio-bind-missing-parent-{}-{}",
        std::process::id(),
        TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed)
    ));
    let invalid_path = missing_parent.join("image.png");
    match ImageOutput::create::<u8>(&invalid_path, 1, 1, 1) {
        Err(Error::OpenImage { message, .. }) => assert!(!message.is_empty()),
        Err(error) => panic!("unexpected unwritable-path error: {error}"),
        Ok(_) => panic!("an output path with a missing parent must fail"),
    }

    let non_utf8 = non_utf8_path();
    assert!(matches!(
        ImageOutput::create::<u8>(&non_utf8, 1, 1, 1),
        Err(Error::NonUtf8Path(_))
    ));
}

fn u16_pattern() -> Vec<u16> {
    (0..48)
        .map(|index| {
            if index % 4 == 3 {
                u16::MAX
            } else {
                ((index * 1_337 + index * index * 17) % 65_536) as u16
            }
        })
        .collect()
}

fn f32_pattern() -> Vec<f32> {
    (0..48)
        .map(|index| match index % 8 {
            0 => 0.0,
            1 => 1.0,
            2 => -1.0,
            3 => 0.5,
            4 => 65_504.0,
            5 => -123.25,
            6 => index as f32 / 7.0,
            _ => f32::from_bits(0x3f00_0000 + index),
        })
        .collect()
}

#[cfg(unix)]
fn non_utf8_path() -> PathBuf {
    use std::os::unix::ffi::OsStringExt;

    std::env::temp_dir().join(std::ffi::OsString::from_vec(vec![
        b'o', b'i', b'i', b'o', b'-', 0xff, b'.', b'p', b'n', b'g',
    ]))
}

#[cfg(windows)]
fn non_utf8_path() -> PathBuf {
    use std::os::windows::ffi::OsStringExt;

    std::env::temp_dir().join(std::ffi::OsString::from_wide(&[
        b'o' as u16,
        b'i' as u16,
        b'i' as u16,
        b'o' as u16,
        b'-' as u16,
        0xd800,
        b'.' as u16,
        b'p' as u16,
        b'n' as u16,
        b'g' as u16,
    ]))
}
