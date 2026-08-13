//! Everything the crate exports, named from outside it.
//!
//! Integration tests are separate crates that link `oiio` the way a
//! dependent would, so this fails if a type stops being re-exported, if a
//! private type leaks into a public signature, or if an error variant a
//! caller needs to match on becomes unnameable.

use oiio::algo::{
    self, ChannelSource, CompareSummary, ContrastRemap, FitMode, Operand, PixelStats, WarpOptions,
};
use oiio::{
    f16, make_texture, make_texture_from_buffer, AttributeValue, ColorConfig, DeepChannel,
    DeepImage, Derivatives, Error, ImageBuf, ImageCache, ImageCacheBuilder, ImageHandle,
    ImageInput, ImageOutput, ImageSpec, InterpolationMode, MipMode, Perthread, Pixel, PixelFormat,
    Result, Roi, Storage, TextureConfig, TextureMode, TextureOptions, TextureSystem, TileGuard,
    WrapMode,
};

/// Types reachable only from a live file or cache still have to be nameable,
/// which is what these signatures assert.
fn _names_borrowed_types(
    handle: &ImageHandle<'_>,
    tile: &TileGuard<'_>,
    deep: &DeepImage,
    state: &Perthread<'_>,
) -> (String, PixelFormat, usize) {
    let _ = format!("{state:?}");
    (handle.filename(), tile.format(), deep.channel_count())
}

fn _names_deep_channel(channel: &DeepChannel) -> (&str, PixelFormat) {
    (channel.name(), channel.format())
}

/// The loop a dependent actually writes: describe, build, process, write,
/// read back, inspect. Entirely in memory, so it needs no scratch directory.
#[test]
fn a_dependent_can_use_the_whole_surface() -> Result<()> {
    let spec = ImageSpec::new(64, 32, 4, PixelFormat::F16)?
        .with_channel_names(["R", "G", "B", "A"])?
        .with_attribute("Artist", "oiio-bind");

    let mut image = ImageBuf::new(&spec)?;
    algo::fill(&mut image, &[0.25, 0.5, 0.75, 1.0], None)?;
    assert_eq!(image.storage(), Storage::Local);

    let mut smaller = ImageBuf::new(&ImageSpec::new(32, 16, 4, PixelFormat::F16)?)?;
    algo::resize(&mut smaller, &image, Some("lanczos3"), None, None)?;

    let mut swapped = ImageBuf::empty()?;
    algo::channels(
        &mut swapped,
        &smaller,
        &[
            ChannelSource::Channel(2),
            ChannelSource::Channel(1),
            ChannelSource::Channel(0),
            ChannelSource::Constant(1.0),
        ],
        Some(&["B", "G", "R", "A"]),
    )?;
    assert_eq!(swapped.spec()?.channel_names(), ["B", "G", "R", "A"]);

    let summary: CompareSummary = algo::compare(&smaller, &smaller, 0.0, 0.0, None);
    assert_eq!(summary.max_error, 0.0);
    assert!(!summary.failed);

    // Write and read back without touching the filesystem.
    let pixels = vec![f16::ZERO; spec.element_count()?];
    let mut output: ImageOutput = ImageOutput::to_memory("image.exr", &spec)?;
    output.write_image(&pixels)?;
    let encoded: Vec<u8> = output.close_into_bytes()?;

    let mut input: ImageInput = ImageInput::from_memory("image.exr", encoded)?;
    let read_spec: ImageSpec = input.image_spec()?;
    let roi: Roi = read_spec.data_window()?.with_channels(0..2)?;
    let mut region = vec![f16::ZERO; roi.element_count()?];
    input.read_region_into(roi, &mut region)?;
    assert_eq!(region.len(), 64 * 32 * 2);

    match read_spec.attribute("Artist") {
        Some(AttributeValue::String(name)) => assert_eq!(name, "oiio-bind"),
        other => panic!("expected a string attribute, got {other:?}"),
    }

    // The pixel maths, including both shapes of Operand.
    let mut math = ImageBuf::new(&spec)?;
    algo::mad(
        &mut math,
        &image,
        Operand::Constant(&[2.0]),
        Operand::Image(&image),
        None,
    )?;
    algo::invert(&mut math, &image, None)?;
    algo::pow(&mut math, &image, &[2.0], None)?;
    algo::clamp(&mut math, &image, &[0.0], &[1.0], true, None)?;
    algo::min(&mut math, &image, Operand::Image(&image), None)?;
    algo::max(&mut math, &image, Operand::Constant(&[0.5]), None)?;
    algo::contrast_remap(&mut math, &image, &ContrastRemap::default(), None)?;
    algo::saturate(&mut math, &image, 0.5, 0, None)?;
    algo::paste(&mut math, [0, 0, 0], 0, &image, None)?;
    // Rotation and warping.
    let mut turned = ImageBuf::empty()?;
    algo::rotate_90(&mut turned, &image, None)?;
    assert_eq!(turned.spec()?.dimensions(), [32, 64, 1]);
    algo::rotate_180(&mut turned, &image, None)?;
    algo::rotate_270(&mut turned, &image, None)?;
    algo::reorient(&mut turned, &image)?;
    algo::rotate(
        &mut turned,
        &image,
        0.0,
        Some([0.0, 0.0]),
        &WarpOptions {
            recompute_region: true,
            ..WarpOptions::default()
        },
        None,
    )?;
    algo::warp(
        &mut turned,
        &image,
        &[1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0],
        &WarpOptions {
            filter: Some("box"),
            filter_width: Some(1.0),
            wrap: Some("clamp"),
            edge_clamp: true,
            ..WarpOptions::default()
        },
        None,
    )?;

    let mut trimmed = ImageBuf::empty()?;
    algo::cut(
        &mut trimmed,
        &image,
        Some(spec.data_window()?.with_x(0..8)?),
    )?;
    assert_eq!(trimmed.spec()?.origin(), [0, 0, 0]);

    // The measurements.
    let stats: PixelStats = algo::pixel_stats(&image, None)?;
    assert_eq!(stats.min.len(), 4);
    let counts: Vec<u64> = algo::histogram(&image, 0, 8, 0.0..1.0, false, None)?;
    assert_eq!(counts.iter().sum::<u64>(), 64 * 32);
    assert!(algo::constant_color(&image, 0.0, None)?.is_some());
    assert!(algo::is_constant_channel(&image, 3, 1.0, 0.0, None)?);
    assert!(!algo::is_monochrome(&image, 0.0, None)?);
    let occupied: Option<Roi> = algo::nonzero_region(&image, None)?;
    assert!(occupied.is_some());
    assert_eq!(algo::pixel_hash_sha1(&image, "", None)?.len(), 40);

    let _builder: ImageCacheBuilder = ImageCache::builder();
    let cache = ImageCache::new()?;
    let _state: Perthread<'_> = cache.thread_state()?;
    let _: PixelFormat = <f16 as Pixel>::FORMAT;
    let _: FitMode = FitMode::default();
    let _spaces = ColorConfig::new()?.color_space_names();

    Ok(())
}

/// The texture half of the crate, from making a file to reading it back.
///
/// Both free functions are named, so neither can quietly stop being exported,
/// and every option type is constructed rather than merely imported.
#[test]
fn a_dependent_can_make_and_look_up_a_texture() -> Result<()> {
    let directory = std::env::temp_dir().join("oiio-bind-public-api");
    std::fs::create_dir_all(&directory).unwrap();
    let source = directory.join("public-api-source.exr");
    let texture = directory.join("public-api.tx");

    let mut image = ImageBuf::new(&ImageSpec::new(32, 32, 3, PixelFormat::F32)?)?;
    algo::fill(&mut image, &[0.4, 0.6, 0.8], None)?;
    image.write(&source)?;

    let config = TextureConfig::new()
        .with_format(PixelFormat::F32)
        .with_tile_size([16, 16, 1])
        .with_wrap_modes(WrapMode::Clamp, WrapMode::Periodic)
        .with_filter("lanczos3")
        .with_mipmap(true)
        .with_attribute("maketx:updatemode", 0);

    make_texture(TextureMode::Texture, &source, &texture, &config)?;
    make_texture_from_buffer(TextureMode::Texture, &image, &texture, &config)?;

    let textures = TextureSystem::new()?;
    assert_eq!(textures.resolution(&texture)?, [32, 32]);

    let options = TextureOptions {
        mip_mode: MipMode::Trilinear,
        interpolation: InterpolationMode::Bilinear,
        s_wrap: WrapMode::Clamp,
        t_wrap: WrapMode::Clamp,
        ..TextureOptions::default()
    };
    let mut rgb = [0.0_f32; 3];
    textures.texture(
        &texture,
        &options,
        0.5,
        0.5,
        Derivatives::uniform(1.0 / 32.0),
        &mut rgb,
    )?;
    assert!((rgb[0] - 0.4).abs() < 0.05, "unexpected lookup {rgb:?}");

    let mut point = [0.0_f32; 3];
    textures.texture(
        &texture,
        &options,
        0.5,
        0.5,
        Derivatives::point(),
        &mut point,
    )?;
    assert!(point.iter().all(|value| value.is_finite()));

    std::fs::remove_file(&source).ok();
    std::fs::remove_file(&texture).ok();
    Ok(())
}

/// A caller has to be able to match on errors to handle them.
#[test]
fn errors_are_matchable_from_outside() {
    let missing = ImageInput::from_path(std::path::Path::new("no-such-file-here.exr"));
    assert!(matches!(missing, Err(Error::OpenImage { .. })));

    let spec = ImageSpec::new(4, 4, 3, PixelFormat::F32).unwrap();
    let image = ImageBuf::new(&spec).unwrap();
    let roi = spec.data_window().unwrap();
    let mut short = vec![0.0_f32; roi.element_count().unwrap() - 1];
    assert!(matches!(
        image.get_pixels_into(roi, &mut short),
        Err(Error::BufferLength {
            expected: 48,
            actual: 47
        })
    ));

    assert!(matches!(
        ImageSpec::new(0, 4, 3, PixelFormat::F32),
        Err(Error::InvalidImageSpec(_))
    ));
    assert!(matches!(
        Roi::new(4..4, 0..1, 0..1, 0..1),
        Err(Error::InvalidRoi(_))
    ));
}
