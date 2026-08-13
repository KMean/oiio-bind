//! Turning images into textures.
//!
//! Unlike `texture_test`, none of this is opt-in: the point of `make_texture`
//! is that the crate can now produce the mipmapped, tiled files it previously
//! could only read from someone else's corpus.

mod common;

use common::ScratchDir;
use oiio::{
    algo, make_texture, make_texture_from_buffer, Error, ImageBuf, ImageInput, ImageSpec,
    PixelFormat, TextureConfig, TextureMode, TextureSystem, WrapMode,
};
use std::path::Path;

/// A source image with enough structure that mip levels differ from each
/// other, and enough resolution to have several of them.
fn checkerboard(size: u32) -> ImageBuf {
    let spec = ImageSpec::new(size, size, 3, PixelFormat::F32).unwrap();
    let mut image = ImageBuf::new(&spec).unwrap();
    algo::fill(&mut image, &[0.0, 0.0, 0.0], None).unwrap();

    let square = i32::try_from(size / 8).unwrap().max(1);
    let size = i32::try_from(size).unwrap();
    for y in (0..size).step_by(square as usize) {
        for x in (0..size).step_by(square as usize) {
            if ((x / square) + (y / square)) % 2 == 0 {
                let roi = oiio::Roi::new(x..x + square, y..y + square, 0..1, 0..3).unwrap();
                algo::fill(&mut image, &[0.9, 0.5, 0.1], Some(roi)).unwrap();
            }
        }
    }
    image
}

/// How many mip levels a file has, counted by asking for each in turn.
fn mip_levels(path: &Path) -> u32 {
    let mut input = ImageInput::from_path(path).unwrap();
    let mut levels = 0;
    while input.image_spec_at(0, levels).is_ok() {
        levels += 1;
    }
    levels
}

#[test]
fn writes_a_mipmapped_tiled_texture() {
    let scratch = ScratchDir::new("maketx");
    let output = scratch.file("checker.tx");

    make_texture_from_buffer(
        TextureMode::Texture,
        &checkerboard(64),
        &output,
        &TextureConfig::new(),
    )
    .unwrap();

    assert!(output.is_file(), "make_texture should have written a file");

    let mut input = ImageInput::from_path(&output).unwrap();
    let spec = input.image_spec().unwrap();
    assert!(
        spec.is_tiled(),
        "a texture should be tiled, got tile dimensions {:?}",
        spec.tile_dimensions()
    );
    assert_eq!(spec.dimensions(), [64, 64, 1]);

    // 64x64 down to 1x1 is seven levels, and each is half the last.
    let levels = mip_levels(&output);
    println!("{} mip levels", levels);
    assert_eq!(levels, 7, "a 64x64 texture should mip all the way down");

    assert_eq!(input.image_spec_at(0, 1).unwrap().dimensions(), [32, 32, 1]);
}

#[test]
fn the_configuration_reaches_the_file() {
    let scratch = ScratchDir::new("maketxconfig");
    let output = scratch.file("configured.tx");

    let config = TextureConfig::new()
        .with_format(PixelFormat::U16)
        .with_tile_size([16, 16, 1])
        .with_wrap_modes(WrapMode::Periodic, WrapMode::Clamp);

    make_texture_from_buffer(TextureMode::Texture, &checkerboard(64), &output, &config).unwrap();

    let input = ImageInput::from_path(&output).unwrap();
    let spec = input.image_spec().unwrap();

    assert_eq!(
        spec.format(),
        PixelFormat::U16,
        "the configured data format should be what was written"
    );
    assert_eq!(spec.tile_dimensions(), [16, 16, 1]);

    let wrap = spec
        .attribute("wrapmodes")
        .expect("a texture should record its wrap modes");
    println!("wrapmodes: {wrap:?}");
    assert!(
        format!("{wrap:?}").contains("periodic,clamp"),
        "the configured wrap modes should reach the file, got {wrap:?}"
    );
}

/// The container has the last word on data format, and takes it silently.
///
/// A `.tx` is a TIFF, and OpenImageIO's TIFF writer "silently change[s]
/// requests for unsupported 'half' to 'float'" unless the `tiff:half`
/// attribute is set. OpenEXR does the reverse to integer formats. Neither
/// reports anything, so this is asserted rather than left to surprise someone.
#[test]
fn the_output_format_can_override_the_configured_one() {
    let scratch = ScratchDir::new("maketxformats");

    let written = |name: &str, format: PixelFormat| {
        let output = scratch.file(name);
        make_texture_from_buffer(
            TextureMode::Texture,
            &checkerboard(16),
            &output,
            &TextureConfig::new().with_format(format),
        )
        .unwrap();
        ImageInput::from_path(&output)
            .unwrap()
            .image_spec()
            .unwrap()
            .format()
    };

    // TIFF cannot store half, and promotes rather than refusing.
    assert_eq!(written("half.tx", PixelFormat::F16), PixelFormat::F32);
    // The same request to a container that can hold it is honoured.
    assert_eq!(written("half.exr", PixelFormat::F16), PixelFormat::F16);
    // OpenEXR has no integer pixel type, and demotes rather than refusing.
    assert_eq!(written("byte.exr", PixelFormat::U8), PixelFormat::F16);
    // TIFF holds every integer format asked of it.
    assert_eq!(written("byte.tx", PixelFormat::U8), PixelFormat::U8);
    assert_eq!(written("short.tx", PixelFormat::U16), PixelFormat::U16);
}

/// Without a pyramid the file is still a texture, just a one-level one.
#[test]
fn mipmapping_can_be_turned_off() {
    let scratch = ScratchDir::new("maketxflat");
    let output = scratch.file("flat.tx");

    make_texture_from_buffer(
        TextureMode::Texture,
        &checkerboard(64),
        &output,
        &TextureConfig::new().with_mipmap(false),
    )
    .unwrap();

    assert_eq!(
        mip_levels(&output),
        1,
        "with mipmapping off there should be exactly one level"
    );
}

/// The whole point: a texture this crate wrote is one this crate can look up.
#[test]
fn a_written_texture_can_be_looked_up() {
    let scratch = ScratchDir::new("maketxlookup");
    let output = scratch.file("lookup.tx");

    make_texture_from_buffer(
        TextureMode::Texture,
        &checkerboard(64),
        &output,
        &TextureConfig::new().with_wrap_modes(WrapMode::Clamp, WrapMode::Clamp),
    )
    .unwrap();

    let textures = TextureSystem::new().unwrap();
    assert_eq!(textures.resolution(&output).unwrap(), [64, 64]);

    let mut rgb = [0.0_f32; 3];
    textures
        .texture(
            &output,
            &oiio::TextureOptions::default(),
            0.5,
            0.5,
            oiio::Derivatives::uniform(1.0 / 64.0),
            &mut rgb,
        )
        .unwrap();
    println!("centre of a texture we made: {rgb:?}");
    assert!(rgb.iter().all(|value| (0.0..=1.0).contains(value)));

    // A lookup with a wide footprint reads a coarse mip level, which for a
    // checkerboard averages towards the mean of its two colours.
    let mut wide = [0.0_f32; 3];
    textures
        .texture(
            &output,
            &oiio::TextureOptions::default(),
            0.5,
            0.5,
            oiio::Derivatives::uniform(0.5),
            &mut wide,
        )
        .unwrap();
    println!("the same point, filtered across half the texture: {wide:?}");
    assert!(
        (wide[0] - 0.45).abs() < 0.15,
        "a coarse level should approach the average of 0.9 and 0.0, got {wide:?}"
    );
}

/// The file-reading form streams, so it is the one to use on a real asset.
#[test]
fn reads_its_input_from_a_file() {
    let scratch = ScratchDir::new("maketxfile");
    let source = scratch.file("source.exr");
    let output = scratch.file("fromfile.tx");

    checkerboard(32).write(&source).unwrap();
    make_texture(
        TextureMode::Texture,
        &source,
        &output,
        &TextureConfig::new(),
    )
    .unwrap();

    let input = ImageInput::from_path(&output).unwrap();
    assert!(input.image_spec().unwrap().is_tiled());
    assert_eq!(mip_levels(&output), 6);
}

#[test]
fn reports_a_missing_input() {
    let scratch = ScratchDir::new("maketxmissing");
    let error = make_texture(
        TextureMode::Texture,
        &scratch.file("no-such-image.exr"),
        &scratch.file("out.tx"),
        &TextureConfig::new(),
    )
    .unwrap_err();

    println!("missing input reported as: {error}");
    let Error::Operation { message, .. } = &error else {
        panic!("expected an operation error, got {error:?}");
    };
    assert!(
        !message.is_empty() && message != "OpenImageIO did not provide an error message",
        "make_texture has no destination image to record an error in, so the \
         message must come from the global channel or the operation's own \
         stream; got {message:?}"
    );
}

#[test]
fn an_unwritable_output_is_reported() {
    let error = make_texture_from_buffer(
        TextureMode::Texture,
        &checkerboard(16),
        Path::new("no-such-directory-here/out.tx"),
        &TextureConfig::new(),
    )
    .unwrap_err();
    println!("unwritable output reported as: {error}");
    assert!(matches!(error, Error::Operation { .. }));
}

/// A format with no concrete size cannot be a texture's data format; leaving
/// it unset is how you ask for the input's.
#[test]
fn an_unknown_output_format_is_refused() {
    let scratch = ScratchDir::new("maketxformat");
    let error = make_texture_from_buffer(
        TextureMode::Texture,
        &checkerboard(16),
        &scratch.file("unknown.tx"),
        &TextureConfig::new().with_format(PixelFormat::Other),
    )
    .unwrap_err();

    assert!(matches!(error, Error::InvalidImageSpec(_)), "{error:?}");
}

/// Setting a name twice keeps the last value, so a configuration can be built
/// up and then overridden.
#[test]
fn the_last_value_for_an_attribute_wins() {
    let config = TextureConfig::new()
        .with_compression("zip")
        .with_compression("none");
    let other = TextureConfig::new().with_compression("none");
    assert_eq!(config, other);
}

