//! `TextureOptions::anisotropic` and `conservative_filter` reach OpenImageIO.
//!
//! A field that is plumbed but ignored is worse than a missing one, because
//! the caller believes they set something. So these tests do not check that
//! the options can be *set*; they check that setting them *changes the
//! answer*, by performing a lookup whose footprint is long and thin, which is
//! the only situation where either knob does anything.
//!
//! Self-contained: the texture is built here with `make_texture`, so this
//! runs without the opt-in corpus.

mod common;

use common::ScratchDir;
use oiio::{
    algo, make_texture, Derivatives, ImageBuf, ImageSpec, PixelFormat, TextureConfig, TextureMode,
    TextureOptions, TextureSystem,
};

/// Fine stripes in s and flat in t, so a footprint stretched along s has a
/// great deal of detail to average and the filter width matters.
fn striped(size: u32) -> ImageBuf {
    let spec = ImageSpec::new(size, size, 3, PixelFormat::F32).unwrap();
    let mut image = ImageBuf::new(&spec).unwrap();
    algo::fill(&mut image, &[0.0, 0.0, 0.0], None).unwrap();

    let size = i32::try_from(size).unwrap();
    for x in (0..size).step_by(2) {
        let roi = oiio::Roi::new(x..x + 1, 0..size, 0..1, 0..3).unwrap();
        algo::fill(&mut image, &[1.0, 1.0, 1.0], Some(roi)).unwrap();
    }
    image
}

fn a_striped_texture(scratch: &ScratchDir) -> std::path::PathBuf {
    let source = scratch.file("striped.exr");
    let texture = scratch.file("striped.tx");
    striped(256).write(&source).unwrap();
    make_texture(
        TextureMode::Texture,
        &source,
        &texture,
        &TextureConfig::new(),
    )
    .unwrap();
    texture
}

/// A footprint far longer in s than in t, which is what anisotropic filtering
/// exists for and the only case where capping the ratio changes anything.
fn grazing() -> Derivatives {
    Derivatives::default()
        .with_dsdx(0.25)
        .with_dtdx(0.0)
        .with_dsdy(0.0)
        .with_dtdy(0.002)
}

fn look_up(textures: &TextureSystem, path: &std::path::Path, options: &TextureOptions) -> f32 {
    let mut rgb = [0.0_f32; 3];
    textures
        .texture(path, options, 0.5, 0.5, grazing(), &mut rgb)
        .unwrap();
    assert!(rgb.iter().all(|v| v.is_finite()), "lookup gave {rgb:?}");
    rgb[0]
}

#[test]
fn the_anisotropic_cap_changes_the_result() {
    let scratch = ScratchDir::new("aniso");
    let path = a_striped_texture(&scratch);
    let textures = TextureSystem::new().unwrap();

    // A ratio of 1 forbids a long thin footprint, so the filter has to fall
    // back on a blurrier mip level. A ratio of 32 lets it follow the
    // footprint and keep detail. The two cannot agree, unless the option
    // never reached OpenImageIO.
    let narrow = look_up(
        &textures,
        &path,
        &TextureOptions::default().with_anisotropic(1),
    );
    let wide = look_up(
        &textures,
        &path,
        &TextureOptions::default().with_anisotropic(32),
    );

    println!("anisotropic 1 -> {narrow},  32 -> {wide}");
    assert!(
        (narrow - wide).abs() > 1e-6,
        "capping anisotropy at 1 gave {narrow} and at 32 gave {wide}; identical \
         results mean the option is not reaching OpenImageIO"
    );
}

#[test]
fn conservative_filtering_changes_the_result() {
    let scratch = ScratchDir::new("conservative");
    let path = a_striped_texture(&scratch);
    let textures = TextureSystem::new().unwrap();

    // Conservative filtering over-blurs rather than aliasing, so switching it
    // off should sharpen a grazing lookup over fine stripes.
    let on = look_up(
        &textures,
        &path,
        &TextureOptions::default().with_conservative_filter(true),
    );
    let off = look_up(
        &textures,
        &path,
        &TextureOptions::default().with_conservative_filter(false),
    );

    println!("conservative on -> {on},  off -> {off}");
    assert!(
        (on - off).abs() > 1e-6,
        "conservative filtering on gave {on} and off gave {off}; identical \
         results mean the option is not reaching OpenImageIO"
    );
}

#[test]
fn the_defaults_are_openimageios_own() {
    // Leaving these alone must change nothing, so that adding the fields did
    // not quietly alter what every existing caller gets.
    let defaults = TextureOptions::default();
    assert!(defaults.conservative_filter, "OpenImageIO defaults this on");
    assert_eq!(defaults.anisotropic, 32, "OpenImageIO defaults this to 32");
}
