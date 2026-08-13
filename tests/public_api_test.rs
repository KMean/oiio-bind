//! Everything the crate exports, named from outside it.
//!
//! Integration tests are separate crates that link `oiio` the way a
//! dependent would, so this fails if a type stops being re-exported, if a
//! private type leaks into a public signature, or if an error variant a
//! caller needs to match on becomes unnameable.

use oiio::algo::{self, ChannelSource, CompareSummary, FitMode};
use oiio::{
    f16, AttributeValue, ColorConfig, DeepChannel, DeepImage, Error, ImageBuf, ImageCache,
    ImageCacheBuilder, ImageHandle, ImageInput, ImageOutput, ImageSpec, Perthread, Pixel,
    PixelFormat, Result, Roi, Storage, TileGuard,
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

    let _builder: ImageCacheBuilder = ImageCache::builder();
    let cache = ImageCache::new()?;
    let _state: Perthread<'_> = cache.thread_state()?;
    let _: PixelFormat = <f16 as Pixel>::FORMAT;
    let _: FitMode = FitMode::default();
    let _spaces = ColorConfig::new()?.color_space_names();

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
