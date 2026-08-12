/// imageio tests
use anyhow::Result;
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
        (30_100..30_200).contains(&runtime_version),
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
    input.set_threads(2);
    assert_eq!(input.threads(), 2);

    Ok(())
}

#[test]
fn test_safe_imageinput_reports_missing_file() {
    let result = oiio::ImageInput::from_path(&fixture_path("does-not-exist.png"));
    assert!(result.is_err());
}
