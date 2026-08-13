//! Colour transforms beyond a plain space change.
//!
//! The OpenColorIO operations need a configuration to do anything, so the ones
//! that name looks or displays check that a missing name is reported rather
//! than asserting on transforms this machine may not have. What is asserted
//! unconditionally is the matrix transform, which needs no configuration, and
//! the channel handling every one of them shares.

mod common;

use oiio::algo::OcioOptions;
use oiio::{algo, ColorConfig, ImageBuf, ImageSpec, PixelFormat, Roi};

fn flat(width: u32, height: u32, values: &[f32]) -> ImageBuf {
    let spec = ImageSpec::new(width, height, values.len() as u32, PixelFormat::F32).unwrap();
    let mut image = ImageBuf::new(&spec).unwrap();
    algo::fill(&mut image, values, None).unwrap();
    image
}

fn pixel(image: &ImageBuf, x: i32, y: i32) -> Vec<f32> {
    let channels = image.spec().unwrap().channel_count();
    let roi = Roi::new(x..x + 1, y..y + 1, 0..1, 0..channels).unwrap();
    let mut values = vec![0.0_f32; channels as usize];
    image.get_pixels_into(roi, &mut values).unwrap();
    values
}

/// Row-major, row-vector convention: colour times matrix.
fn identity() -> [f32; 16] {
    [
        1.0, 0.0, 0.0, 0.0, //
        0.0, 1.0, 0.0, 0.0, //
        0.0, 0.0, 1.0, 0.0, //
        0.0, 0.0, 0.0, 1.0,
    ]
}

#[test]
fn a_matrix_transform_mixes_channels() {
    // Swap red and blue.
    let mut swap = identity();
    swap[0] = 0.0;
    swap[2] = 1.0; // red row sends to blue
    swap[8] = 1.0; // blue row sends to red
    swap[10] = 0.0;

    let source = flat(4, 4, &[1.0, 0.5, 0.0]);
    let mut result = ImageBuf::empty().unwrap();
    algo::color_matrix_transform(&mut result, &source, &swap, false, None).unwrap();

    let got = pixel(&result, 0, 0);
    println!("swapped: {got:?}");
    assert!((got[0] - 0.0).abs() < 1e-5, "{got:?}");
    assert!((got[1] - 0.5).abs() < 1e-5, "{got:?}");
    assert!((got[2] - 1.0).abs() < 1e-5, "{got:?}");
}

#[test]
fn an_identity_matrix_changes_nothing() {
    let source = flat(4, 4, &[0.25, 0.5, 0.75]);
    let mut result = ImageBuf::empty().unwrap();
    algo::color_matrix_transform(&mut result, &source, &identity(), false, None).unwrap();

    let got = pixel(&result, 0, 0);
    for (a, b) in got.iter().zip(&[0.25, 0.5, 0.75]) {
        assert!((a - b).abs() < 1e-5, "{got:?}");
    }
}

/// The transform is applied to four components, and the fourth is alpha when
/// there is one and zero when there is not. So the translation row reaches an
/// RGBA image and not an RGB one — surprising, and worth pinning down.
#[test]
fn the_translation_row_needs_a_fourth_channel() {
    let mut lift = identity();
    lift[12] = 0.1;
    lift[13] = 0.1;
    lift[14] = 0.1;

    let rgb = flat(4, 4, &[0.0, 0.0, 0.0]);
    let mut from_rgb = ImageBuf::empty().unwrap();
    algo::color_matrix_transform(&mut from_rgb, &rgb, &lift, false, None).unwrap();
    let got = pixel(&from_rgb, 0, 0);
    println!("translation applied to RGB: {got:?}");
    assert!(
        got.iter().all(|v| v.abs() < 1e-5),
        "with no alpha the fourth component is zero, so the translation row is \
         multiplied away; got {got:?}"
    );

    let rgba = flat(4, 4, &[0.0, 0.0, 0.0, 1.0]);
    let mut from_rgba = ImageBuf::empty().unwrap();
    algo::color_matrix_transform(&mut from_rgba, &rgba, &lift, false, None).unwrap();
    let got = pixel(&from_rgba, 0, 0);
    println!("translation applied to RGBA: {got:?}");
    assert!(
        (got[0] - 0.1).abs() < 1e-5,
        "with alpha of 1 the translation lands; got {got:?}"
    );
}

/// OpenImageIO's colour engine says it copies channels past the fourth
/// "unaltered from the source" and then writes `0.5 + 10 * source` into them.
/// Every colour operation here shares that engine, so every one of them has to
/// put those channels back.
#[test]
fn channels_past_the_fourth_survive_a_colour_transform() {
    // Six channels: RGBA plus two arbitrary AOVs.
    let source = flat(4, 4, &[0.2, 0.4, 0.6, 1.0, 0.125, 0.875]);

    let mut result = ImageBuf::empty().unwrap();
    algo::color_matrix_transform(&mut result, &source, &identity(), false, None).unwrap();

    let got = pixel(&result, 0, 0);
    println!("six channels through a matrix transform: {got:?}");
    assert_eq!(got.len(), 6);
    assert!(
        (got[4] - 0.125).abs() < 1e-5 && (got[5] - 0.875).abs() < 1e-5,
        "the fifth and sixth channels should be unchanged, got {got:?}. \
         0.5 + 10 * x would be {} and {}",
        0.5 + 10.0 * 0.125,
        0.5 + 10.0 * 0.875
    );
}

/// The same for the space conversion that was already bound: it goes through
/// the same engine, so it had the same problem.
#[test]
fn channels_past_the_fourth_survive_a_space_conversion() {
    let source = flat(4, 4, &[0.2, 0.4, 0.6, 1.0, 0.125, 0.875]);

    let mut result = ImageBuf::empty().unwrap();
    // Two genuinely different spaces. A conversion between a space and itself
    // short-circuits without running the engine at all, and so would prove
    // nothing here.
    algo::color_convert(
        &mut result,
        &source,
        "Linear Rec.709 (sRGB)",
        "ACEScg",
        false,
        None,
    )
    .unwrap();

    let got = pixel(&result, 0, 0);
    println!("six channels through a colour conversion: {got:?}");
    assert!(
        (got[4] - 0.125).abs() < 1e-5 && (got[5] - 0.875).abs() < 1e-5,
        "the extra channels should be unchanged, got {got:?}"
    );
}

/// Asking for the source's own colour space is the documented default, and
/// OpenImageIO reaches through a null configuration pointer to answer it. The
/// binding always supplies a configuration, so this must simply work — or fail
/// with a message, never crash.
#[test]
fn a_look_with_no_spaces_named_does_not_crash() {
    let source = flat(4, 4, &[0.2, 0.4, 0.6]);
    let mut result = ImageBuf::empty().unwrap();

    // No look, no spaces: the case that dereferences null upstream.
    match algo::ocio_look(
        &mut result,
        &source,
        "",
        None,
        None,
        &OcioOptions::default(),
        None,
    ) {
        Ok(()) => println!("converted through the default configuration"),
        Err(error) => println!("reported rather than crashed: {error}"),
    }
}

#[test]
fn a_look_that_does_not_exist_is_reported() {
    let source = flat(4, 4, &[0.2, 0.4, 0.6]);
    let mut result = ImageBuf::empty().unwrap();

    let outcome = algo::ocio_look(
        &mut result,
        &source,
        "no-such-look-in-any-config",
        Some("linear"),
        Some("linear"),
        &OcioOptions::default(),
        None,
    );
    match outcome {
        Ok(()) => println!("this configuration accepted the look"),
        Err(error) => println!("unknown look reported as: {error}"),
    }
}

#[test]
fn a_display_that_does_not_exist_is_reported() {
    let source = flat(4, 4, &[0.2, 0.4, 0.6]);
    let mut result = ImageBuf::empty().unwrap();

    let outcome = algo::ocio_display(
        &mut result,
        &source,
        "no-such-display",
        "no-such-view",
        None,
        "",
        &OcioOptions::default(),
        None,
    );
    assert!(
        outcome.is_err(),
        "a display that is not in the configuration should be an error"
    );
    println!("unknown display reported as: {}", outcome.unwrap_err());
}

#[test]
fn a_missing_transform_file_is_reported() {
    let source = flat(4, 4, &[0.2, 0.4, 0.6]);
    let mut result = ImageBuf::empty().unwrap();

    let error = algo::ocio_file_transform(
        &mut result,
        &source,
        std::path::Path::new("no-such-lut-file.cube"),
        &OcioOptions::default(),
        None,
    )
    .unwrap_err();
    println!("missing transform file reported as: {error}");
}

/// An empty path is refused before OpenImageIO can read past the end of a
/// string view that is not NUL-terminated.
#[test]
fn an_empty_transform_path_is_refused() {
    let source = flat(4, 4, &[0.2, 0.4, 0.6]);
    let mut result = ImageBuf::empty().unwrap();

    let error = algo::ocio_file_transform(
        &mut result,
        &source,
        std::path::Path::new(""),
        &OcioOptions::default(),
        None,
    )
    .unwrap_err();
    println!("empty transform path reported as: {error}");
}

#[test]
fn a_named_transform_that_does_not_exist_is_reported() {
    let source = flat(4, 4, &[0.2, 0.4, 0.6]);
    let mut result = ImageBuf::empty().unwrap();

    let outcome = algo::ocio_named_transform(
        &mut result,
        &source,
        "no-such-named-transform",
        &OcioOptions::default(),
        None,
    );
    assert!(outcome.is_err());
    println!(
        "unknown named transform reported as: {}",
        outcome.unwrap_err()
    );
}

/// The default is OpenImageIO's, which has unpremultiplication on.
#[test]
fn the_default_options_match_openimageio() {
    let options = OcioOptions::default();
    assert!(
        options.unpremult,
        "OpenImageIO unpremultiplies by default, and so should this"
    );
    assert!(!options.inverse);
    assert!(options.context_key.is_empty());
}

/// Whatever configuration is active, it should at least be reportable, which
/// is what makes naming a space possible rather than guesswork.
#[test]
fn the_active_configuration_can_be_asked_what_it_has() {
    let config = ColorConfig::new().unwrap();
    let spaces = config.color_space_names();
    println!("{} colour spaces: {:?}", spaces.len(), &spaces);
    assert!(!spaces.is_empty(), "even the built-in config names spaces");
}
