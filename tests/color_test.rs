//! Colour spaces and conversion between them.

mod common;

use oiio::{algo, ColorConfig, Error, ImageBuf, ImageSpec, PixelFormat};

fn spec() -> ImageSpec {
    ImageSpec::new(8, 8, 3, PixelFormat::F32).unwrap()
}

fn filled(values: &[f32]) -> ImageBuf {
    let mut image = ImageBuf::new(&spec()).unwrap();
    algo::fill(&mut image, values, None).unwrap();
    image
}

fn first_pixel(image: &ImageBuf) -> Vec<f32> {
    let roi = image.spec().unwrap().data_window().unwrap();
    let mut values = vec![0.0_f32; roi.element_count().unwrap()];
    image.get_pixels_into(roi, &mut values).unwrap();
    values[..image.channel_count() as usize].to_vec()
}

#[test]
fn reports_the_active_configuration() {
    let config = ColorConfig::new().unwrap();
    let spaces = config.color_space_names();
    let roles = config.role_names();

    println!("configuration: {}", config.name());
    println!(
        "OpenColorIO support: {}",
        ColorConfig::supports_opencolorio()
    );
    println!("{} colour spaces, {} roles", spaces.len(), roles.len());
    for space in spaces.iter().take(12) {
        println!("  space {space}");
    }
    for role in roles.iter().take(12) {
        println!(
            "  role {role} -> {:?}",
            config.color_space_for_role(role).unwrap_or_default()
        );
    }

    // Any usable configuration defines something, even the built-in one.
    assert!(!spaces.is_empty(), "no colour spaces are available");
    assert!(config.has_color_space(&spaces[0]));
    assert!(!config.has_color_space("definitely-not-a-colour-space"));
    assert!(config
        .color_space_for_role("definitely-not-a-role")
        .is_none());
}

/// sRGB and linear differ by a transfer curve, so a mid-grey must move. This
/// is the conversion every pipeline does when it loads an 8-bit image.
#[test]
fn converts_between_srgb_and_linear() {
    let config = ColorConfig::new().unwrap();
    // Name the spaces the way this configuration does.
    let Some(linear) = config.color_space_for_role("scene_linear").or_else(|| {
        config
            .has_color_space("linear")
            .then(|| "linear".to_owned())
    }) else {
        eprintln!("skipping: this configuration has no linear space");
        return;
    };
    // Configurations name sRGB differently — "sRGB", "Utility - sRGB -
    // Texture", "sRGB Encoded Rec.709 (sRGB)" — so ask the configuration
    // rather than hardcoding a guess that would silently skip this test.
    // A role gives the scene-referred one, which is what an 8-bit texture is
    // in; failing that, look for a display space by name.
    let srgb = config
        .color_space_for_role("texture_paint")
        .or_else(|| config.color_space_for_role("color_picking"))
        .or_else(|| {
            config
                .color_space_names()
                .into_iter()
                .find(|name| name.to_lowercase().contains("srgb"))
        });
    let Some(srgb) = srgb else {
        panic!(
            "no sRGB-like space found among {:?}",
            config.color_space_names()
        );
    };
    println!("converting {srgb} <-> {linear}");

    let grey = 0.5_f32;
    let source = filled(&[grey, grey, grey]);

    let mut to_linear = ImageBuf::new(&spec()).unwrap();
    algo::color_convert(&mut to_linear, &source, &srgb, &linear, false, None).unwrap();
    let converted = first_pixel(&to_linear);

    // sRGB 0.5 is about 0.21 linear. The exact figure depends on the
    // configuration's transfer function, so assert the direction and rough
    // magnitude rather than a constant.
    println!("  {grey} in {srgb} is {:?} in {linear}", converted);
    assert!(
        converted[0] < grey,
        "converting sRGB to linear should darken mid-grey, got {converted:?}"
    );
    assert!(
        converted[0] > 0.1 && converted[0] < 0.3,
        "expected roughly 0.21, got {converted:?}"
    );

    // And back again, which should land near where it started.
    let mut back = ImageBuf::new(&spec()).unwrap();
    algo::color_convert(&mut back, &to_linear, &linear, &srgb, false, None).unwrap();
    let returned = first_pixel(&back);
    println!("  and back to {:?}", returned);
    assert!(
        (returned[0] - grey).abs() < 1e-3,
        "the round trip should return to {grey}, got {returned:?}"
    );
}

#[test]
fn converting_to_the_same_space_changes_nothing() {
    let config = ColorConfig::new().unwrap();
    let spaces = config.color_space_names();
    let Some(space) = spaces.first() else {
        eprintln!("skipping: no colour spaces available");
        return;
    };

    let source = filled(&[0.25, 0.5, 0.75]);
    let mut result = ImageBuf::new(&spec()).unwrap();
    algo::color_convert(&mut result, &source, space, space, false, None).unwrap();

    let before = first_pixel(&source);
    let after = first_pixel(&result);
    assert_eq!(before, after, "a conversion to the same space is a copy");
}

#[test]
fn rejects_an_unknown_colour_space() {
    let source = filled(&[0.5, 0.5, 0.5]);
    let mut result = ImageBuf::new(&spec()).unwrap();

    let error = algo::color_convert(
        &mut result,
        &source,
        "definitely-not-a-colour-space",
        "also-not-one",
        false,
        None,
    )
    .unwrap_err();
    println!("unknown space reported as: {error}");
}

#[test]
fn rejects_an_empty_space_name() {
    let source = filled(&[0.5, 0.5, 0.5]);
    let mut result = ImageBuf::new(&spec()).unwrap();

    assert!(matches!(
        algo::color_convert(&mut result, &source, "", "linear", false, None),
        Err(Error::InvalidImageSpec(_))
    ));
    assert!(matches!(
        algo::color_convert(&mut result, &source, "linear", "", false, None),
        Err(Error::InvalidImageSpec(_))
    ));
}

#[test]
fn a_missing_configuration_file_is_reported() {
    let scratch = common::ScratchDir::new("noconfig");
    let missing = scratch.file("no-such-config.ocio");
    // Either an error, or a configuration that knows nothing; both are
    // honest answers, but it must not pretend the file was loaded.
    match ColorConfig::from_path(&missing) {
        Ok(config) => {
            println!("absent config loaded as {:?}", config.name());
            assert!(
                !config.name().contains("no-such-config"),
                "a missing file should not be reported as loaded"
            );
        }
        Err(error) => println!("absent config reported as: {error}"),
    }
}

/// `has_color_space` compared against the enumerated names, exactly and
/// case-sensitively. OpenImageIO does not resolve names that way: colour space
/// lookup is case-insensitive, falls back to aliases, and resolves roles
/// separately. So the names `color_convert` accepts were a strict superset of
/// the ones this admitted, and a caller using it to validate input rejected
/// input that would have worked.
#[test]
fn has_color_space_agrees_with_what_a_conversion_accepts() {
    let Ok(config) = ColorConfig::new() else {
        eprintln!("no colour configuration available; skipping");
        return;
    };

    let source = ImageBuf::new(&ImageSpec::new(4, 4, 3, PixelFormat::F32).unwrap()).unwrap();
    let names = config.color_space_names();
    if names.is_empty() {
        eprintln!("the configuration lists no colour spaces; skipping");
        return;
    }
    let target = names
        .iter()
        .find(|name| name.eq_ignore_ascii_case("ACEScg"))
        .cloned()
        .unwrap_or_else(|| names[0].clone());

    // Every casing of a name it knows, plus the roles, must agree with what a
    // conversion will actually accept.
    let mut probes: Vec<String> = vec![
        target.to_lowercase(),
        target.to_uppercase(),
        "definitely not a colour space".to_owned(),
    ];
    probes.extend(["scene_linear", "data", "default"].map(str::to_owned));

    for probe in probes {
        let mut dst = ImageBuf::empty().unwrap();
        let converts =
            oiio::algo::color_convert(&mut dst, &source, &probe, &target, false, None).is_ok();
        assert_eq!(
            config.has_color_space(&probe),
            converts,
            "has_color_space and color_convert disagree about {probe:?}"
        );
    }
}
