//! Every fieldless public enum prints a name and parses back from it.
//!
//! The point is a value that can come from a command line or a configuration
//! file. `oiiox`, the pipeline tool in this repository, is the obvious
//! consumer: `-d half` and `--wrap periodic` should not need a hand-written
//! match in every caller.

use oiio::algo::{FitMode, MosaicPattern, NonFiniteFix, TextAlignX, TextAlignY};
use oiio::{InterpolationMode, MipMode, PixelFormat, Storage, TextureMode, Wrap, WrapMode};
use std::str::FromStr;

/// Name, print, parse, and get the same value back, for every variant.
macro_rules! round_trip {
    ($name:ident, $kind:ty, [$($variant:expr),+ $(,)?]) => {
        #[test]
        fn $name() {
            let all = [$($variant),+];
            assert_eq!(
                all.len(),
                <$kind>::NAMES.len(),
                "NAMES and the variant list disagree for {}",
                stringify!($kind)
            );

            for value in all {
                let printed = value.to_string();
                assert_eq!(printed, value.name(), "Display should be name()");
                assert!(
                    <$kind>::NAMES.contains(&value.name()),
                    "{printed:?} is missing from NAMES"
                );
                assert_eq!(
                    <$kind>::from_str(&printed).unwrap(),
                    value,
                    "{printed:?} did not parse back to itself"
                );
                // Parsing is case insensitive, because a config file or a
                // command line is written by a person.
                assert_eq!(<$kind>::from_str(&printed.to_uppercase()).unwrap(), value);
            }

            let error = <$kind>::from_str("definitely-not-a-value").unwrap_err();
            let text = error.to_string();
            assert!(
                text.contains("definitely-not-a-value"),
                "the error should quote the input: {text}"
            );
            // And it must list what would have worked, since that is the whole
            // reason to parse rather than match.
            for name in <$kind>::NAMES {
                assert!(
                    text.contains(name),
                    "the error should list {name:?} among the alternatives: {text}"
                );
            }
        }
    };
}

round_trip!(
    pixel_format_round_trips,
    PixelFormat,
    [
        PixelFormat::U8,
        PixelFormat::I8,
        PixelFormat::U16,
        PixelFormat::I16,
        PixelFormat::U32,
        PixelFormat::I32,
        PixelFormat::U64,
        PixelFormat::I64,
        PixelFormat::F16,
        PixelFormat::F32,
        PixelFormat::F64,
        PixelFormat::Other,
    ]
);

round_trip!(
    wrap_mode_round_trips,
    WrapMode,
    [
        WrapMode::Default,
        WrapMode::Black,
        WrapMode::Clamp,
        WrapMode::Periodic,
        WrapMode::Mirror,
    ]
);

round_trip!(
    mip_mode_round_trips,
    MipMode,
    [
        MipMode::Default,
        MipMode::None,
        MipMode::OneLevel,
        MipMode::Trilinear,
        MipMode::Anisotropic,
    ]
);

round_trip!(
    interpolation_round_trips,
    InterpolationMode,
    [
        InterpolationMode::Closest,
        InterpolationMode::Bilinear,
        InterpolationMode::Bicubic,
        InterpolationMode::SmartBicubic,
    ]
);

round_trip!(
    texture_mode_round_trips,
    TextureMode,
    [
        TextureMode::Texture,
        TextureMode::Shadow,
        TextureMode::LatLongEnvironment,
        TextureMode::LightProbe,
        TextureMode::BumpWithSlopes,
    ]
);

round_trip!(
    wrap_round_trips,
    Wrap,
    [
        Wrap::Default,
        Wrap::Black,
        Wrap::Clamp,
        Wrap::Periodic,
        Wrap::Mirror,
    ]
);

round_trip!(
    storage_round_trips,
    Storage,
    [
        Storage::Uninitialized,
        Storage::Local,
        Storage::App,
        Storage::Cache,
    ]
);

round_trip!(
    fit_mode_round_trips,
    FitMode,
    [FitMode::Letterbox, FitMode::Width, FitMode::Height]
);

round_trip!(
    non_finite_fix_round_trips,
    NonFiniteFix,
    [
        NonFiniteFix::None,
        NonFiniteFix::Black,
        NonFiniteFix::Box3,
        NonFiniteFix::Error,
    ]
);

round_trip!(
    mosaic_pattern_round_trips,
    MosaicPattern,
    [MosaicPattern::Bayer, MosaicPattern::XTrans]
);

round_trip!(
    text_align_x_round_trips,
    TextAlignX,
    [TextAlignX::Left, TextAlignX::Right, TextAlignX::Center]
);

round_trip!(
    text_align_y_round_trips,
    TextAlignY,
    [
        TextAlignY::Baseline,
        TextAlignY::Top,
        TextAlignY::Bottom,
        TextAlignY::Center,
    ]
);

/// The names that OpenImageIO itself fixes must not drift, because a string
/// written for `oiiotool` has to mean the same thing here.
#[test]
fn the_names_openimageio_fixes_are_openimageios() {
    // oiiotool -d takes these.
    assert_eq!(PixelFormat::F16.name(), "half");
    assert_eq!(PixelFormat::F32.name(), "float");
    assert_eq!(PixelFormat::U16.name(), "uint16");
    // oiiotool --fixnan takes exactly these four.
    assert_eq!(NonFiniteFix::Box3.name(), "box3");
    assert_eq!(NonFiniteFix::None.name(), "none");
    // The demosaic pattern attribute takes these.
    assert_eq!(MosaicPattern::XTrans.name(), "xtrans");
    // Wrap modes, as a texture file records them.
    assert_eq!(WrapMode::Periodic.name(), "periodic");
}
