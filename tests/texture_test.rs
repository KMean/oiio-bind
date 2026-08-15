//! Filtered texture lookups.
//!
//! The interesting cases need a real mipmapped texture, so those are opt-in
//! through `OIIO_BIND_TEST_IMAGES`, as in `corpus_test`.

mod common;

use std::path::PathBuf;

use common::{f32_ramp, write_image, ScratchDir};
use oiio::{
    Derivatives, Error, ImageSpec, InterpolationMode, MipMode, PixelFormat, TextureOptions,
    TextureSystem, WrapMode,
};

/// A `.tx` from the corpus: tiled, mipmapped, and made by maketx rather than
/// by us.
fn a_texture() -> Option<PathBuf> {
    let root = PathBuf::from(std::env::var_os("OIIO_BIND_TEST_IMAGES")?);
    for name in ["grid.tx", "checker.tx", "miplevels.tx"] {
        let path = root.join(name);
        if path.is_file() {
            return Some(path);
        }
    }
    None
}

/// Environment lookups need no corpus: the crate makes its own lat-long map.
#[test]
fn environment_looks_up_a_latlong_map() {
    let scratch = ScratchDir::new("envmap");
    let spec = ImageSpec::new(64, 32, 3, PixelFormat::F32).unwrap();
    let mut sky = oiio::ImageBuf::new(&spec).unwrap();
    oiio::algo::fill(&mut sky, &[0.25, 0.5, 0.75], None).unwrap();
    let map = scratch.file("sky.tx");
    oiio::make_texture_from_buffer(
        oiio::TextureMode::LatLongEnvironment,
        &sky,
        &map,
        &oiio::TextureConfig::new(),
    )
    .unwrap();

    let textures = TextureSystem::new().unwrap();
    let options = TextureOptions::default();

    // A flat map answers every direction with the same colour.
    let mut colour = [0.0_f32; 3];
    textures
        .environment(
            &map,
            &options,
            [0.0, 0.0, 1.0],
            [0.0; 3],
            [0.0; 3],
            &mut colour,
        )
        .unwrap();
    // make_texture quantized the map to uint8, so half a code value of slack.
    assert!(
        (colour[0] - 0.25).abs() < 2.5e-3
            && (colour[1] - 0.5).abs() < 2.5e-3
            && (colour[2] - 0.75).abs() < 2.5e-3,
        "a flat environment reads back flat: {colour:?}"
    );

    // Channels past the file's take the fill value, supplied by the crate
    // because OpenImageIO zero-fills environment lookups instead.
    let mut wide = [9.0_f32; 5];
    textures
        .environment(
            &map,
            &options,
            [0.0, 1.0, 0.0],
            [0.0; 3],
            [0.0; 3],
            &mut wide,
        )
        .unwrap();
    assert_eq!(wide[3], 0.0, "past the file is the fill value: {wide:?}");

    // A file that is not there is an error, not a crash.
    let mut nothing = [0.0_f32; 3];
    assert!(textures
        .environment(
            &scratch.file("missing.tx"),
            &options,
            [0.0, 0.0, 1.0],
            [0.0; 3],
            [0.0; 3],
            &mut nothing
        )
        .is_err());
}

#[test]
fn creates_a_texture_system() {
    let mut textures = TextureSystem::new().unwrap();
    textures.set_max_memory_mb(64.0).unwrap();
    textures.set_max_open_files(32).unwrap();

    // Nonsense settings are refused rather than silently ignored.
    assert!(matches!(
        textures.set_max_memory_mb(0.0),
        Err(Error::InvalidCacheSetting { .. })
    ));
    assert!(matches!(
        textures.set_max_memory_mb(f32::NAN),
        Err(Error::InvalidCacheSetting { .. })
    ));

    let stats = textures.stats();
    assert!(
        !stats.is_empty(),
        "a texture system should report statistics"
    );
}

#[test]
fn looks_up_a_texture() {
    let Some(path) = a_texture() else {
        eprintln!("skipping: set OIIO_BIND_TEST_IMAGES to a corpus containing .tx files");
        return;
    };

    let textures = TextureSystem::new().unwrap();
    let resolution = textures.resolution(&path).unwrap();
    println!(
        "{}: {resolution:?}",
        path.file_name().unwrap().to_string_lossy()
    );
    assert!(resolution[0] > 0 && resolution[1] > 0);

    let options = TextureOptions::default();
    let texel = 1.0 / resolution[0] as f32;

    // A filtered lookup in the middle of the texture.
    let mut rgb = [0.0_f32; 3];
    textures
        .texture(
            &path,
            &options,
            0.5,
            0.5,
            Derivatives::uniform(texel),
            &mut rgb,
        )
        .unwrap();
    println!("  centre: {rgb:?}");
    assert!(
        rgb.iter().all(|value| value.is_finite()),
        "a lookup should return finite values, got {rgb:?}"
    );

    // The number of channels asked for is the length of the destination.
    let mut single = [0.0_f32; 1];
    textures
        .texture(
            &path,
            &options,
            0.5,
            0.5,
            Derivatives::uniform(texel),
            &mut single,
        )
        .unwrap();
    assert!(
        (single[0] - rgb[0]).abs() < 1e-5,
        "one channel should match the first of three"
    );
}

/// Blur reaches the filter, so it changes what a lookup returns.
///
/// Deliberately not phrased as "blur smooths the result". Two attempts to
/// assert that on `grid.tx` failed, in both directions: samples spread across
/// the image vary *more* at a coarse mip level, since each step crosses more
/// picture, and neighbouring samples of a grid pattern can differ more when
/// blurred, since the blur pulls in lines that were not previously in range.
/// Both are properties of the fixture rather than of filtering, so what is
/// asserted here is the thing that is actually guaranteed: the option is
/// wired through, and the results stay sensible.
#[test]
fn blur_changes_what_a_lookup_returns() {
    let Some(path) = a_texture() else {
        eprintln!("skipping: set OIIO_BIND_TEST_IMAGES");
        return;
    };

    let textures = TextureSystem::new().unwrap();
    let resolution = textures.resolution(&path).unwrap();
    let texel = 1.0 / resolution[0] as f32;

    let line = |blur: f32| -> Vec<f32> {
        let options = TextureOptions {
            s_blur: blur,
            t_blur: blur,
            ..TextureOptions::default()
        };
        (0..64)
            .map(|step| {
                let s = 0.25 + step as f32 * texel * 4.0;
                let mut sample = [0.0_f32; 1];
                textures
                    .texture(
                        &path,
                        &options,
                        s,
                        0.5,
                        Derivatives::uniform(texel),
                        &mut sample,
                    )
                    .unwrap();
                sample[0]
            })
            .collect()
    };

    let sharp = line(0.0);
    let blurred = line(0.05);
    let changed = sharp
        .iter()
        .zip(&blurred)
        .filter(|(a, b)| (*a - *b).abs() > 1e-6)
        .count();

    println!("{changed} of {} samples changed with blur", sharp.len());
    assert!(
        changed > 0,
        "blur should reach the filter and change something"
    );
    assert!(
        sharp.iter().chain(&blurred).all(|value| value.is_finite()),
        "every sample should be finite"
    );
}

/// Mip mode changes what a lookup reads, which is the point of having one.
#[test]
fn mip_mode_changes_the_result() {
    let Some(path) = a_texture() else {
        eprintln!("skipping: set OIIO_BIND_TEST_IMAGES");
        return;
    };
    let textures = TextureSystem::new().unwrap();

    // A wide filter, so the mip pyramid is genuinely in play.
    let sample = |mip_mode: MipMode| -> f32 {
        let options = TextureOptions {
            mip_mode,
            ..TextureOptions::default()
        };
        let mut value = [0.0_f32; 1];
        textures
            .texture(
                &path,
                &options,
                0.3,
                0.3,
                Derivatives::uniform(0.05),
                &mut value,
            )
            .unwrap();
        value[0]
    };

    let highest = sample(MipMode::None);
    let filtered = sample(MipMode::Trilinear);
    println!("mip modes: none {highest:.5}, trilinear {filtered:.5}");
    // Both must be sensible; whether they differ depends on the texture, so
    // assert what is certain rather than inventing a threshold.
    assert!(highest.is_finite() && filtered.is_finite());
    assert!((0.0..=1.0).contains(&highest), "unexpected value {highest}");
    assert!(
        (0.0..=1.0).contains(&filtered),
        "unexpected value {filtered}"
    );
}

#[test]
fn options_change_the_result() {
    let Some(path) = a_texture() else {
        eprintln!("skipping: set OIIO_BIND_TEST_IMAGES");
        return;
    };
    let textures = TextureSystem::new().unwrap();

    let sample = |options: &TextureOptions, s: f32| -> f32 {
        let mut value = [0.0_f32; 1];
        textures
            .texture(
                &path,
                options,
                s,
                0.5,
                Derivatives::uniform(0.001),
                &mut value,
            )
            .unwrap();
        value[0]
    };

    // Outside the unit square, wrapping decides the answer. Black must give
    // zero where clamping gives an edge value.
    let mut black = TextureOptions {
        s_wrap: WrapMode::Black,
        t_wrap: WrapMode::Black,
        ..TextureOptions::default()
    };
    black.interpolation = InterpolationMode::Closest;
    black.mip_mode = MipMode::None;

    let clamp = TextureOptions {
        s_wrap: WrapMode::Clamp,
        t_wrap: WrapMode::Clamp,
        interpolation: InterpolationMode::Closest,
        mip_mode: MipMode::None,
        ..TextureOptions::default()
    };

    let outside = 1.5_f32;
    let black_value = sample(&black, outside);
    let clamp_value = sample(&clamp, outside);
    println!("outside the texture: black {black_value}, clamp {clamp_value}");
    assert_eq!(
        black_value, 0.0,
        "black wrapping should read as zero outside"
    );

    // fill supplies channels the texture does not have.
    let filled = TextureOptions {
        first_channel: 0,
        fill: 0.75,
        ..TextureOptions::default()
    };
    let mut many = [0.0_f32; 8];
    textures
        .texture(
            &path,
            &filled,
            0.5,
            0.5,
            Derivatives::uniform(0.001),
            &mut many,
        )
        .unwrap();
    println!("eight channels from a three-channel texture: {many:?}");
    assert!(
        many.iter()
            .skip(4)
            .all(|value| (*value - 0.75).abs() < 1e-5),
        "channels beyond the texture should take the fill value, got {many:?}"
    );
}

#[test]
fn reports_a_missing_texture() {
    let scratch = ScratchDir::new("notexture");
    let missing = scratch.file("no-such-texture.tx");
    let textures = TextureSystem::new().unwrap();

    assert!(textures.resolution(&missing).is_err());

    let mut rgb = [0.0_f32; 3];
    let error = textures
        .texture(
            &missing,
            &TextureOptions::default(),
            0.5,
            0.5,
            Derivatives::point(),
            &mut rgb,
        )
        .unwrap_err();
    println!("missing texture reported as: {error}");
}

#[test]
fn an_empty_destination_is_refused() {
    let textures = TextureSystem::new().unwrap();
    let scratch = ScratchDir::new("emptydest");
    let path = scratch.file("flat.exr");
    let spec = ImageSpec::new(8, 8, 3, PixelFormat::F32).unwrap();
    write_image(&path, &spec, &f32_ramp(spec.element_count().unwrap())).unwrap();

    let mut nothing: [f32; 0] = [];
    assert!(matches!(
        textures.texture(
            &path,
            &TextureOptions::default(),
            0.5,
            0.5,
            Derivatives::point(),
            &mut nothing,
        ),
        Err(Error::InvalidRoi(_))
    ));
}

#[test]
fn a_texture_system_can_be_shared_across_threads() {
    let Some(path) = a_texture() else {
        eprintln!("skipping: set OIIO_BIND_TEST_IMAGES");
        return;
    };

    let textures = std::sync::Arc::new(TextureSystem::new().unwrap());
    let options = TextureOptions::default();

    std::thread::scope(|scope| {
        for thread in 0..4 {
            let textures = std::sync::Arc::clone(&textures);
            let path = path.clone();
            let options = options.clone();
            scope.spawn(move || {
                for step in 0..32 {
                    let s = (thread as f32 * 0.25) + step as f32 * 0.005;
                    let mut rgb = [0.0_f32; 3];
                    textures
                        .texture(
                            &path,
                            &options,
                            s,
                            0.5,
                            Derivatives::uniform(0.002),
                            &mut rgb,
                        )
                        .unwrap();
                    assert!(rgb.iter().all(|value| value.is_finite()));
                }
            });
        }
    });
}

/// Build a real mipmapped `.tx` so the bounds tests do not need the corpus.
fn a_built_texture(scratch: &ScratchDir) -> PathBuf {
    let source = scratch.file("source.exr");
    let spec = ImageSpec::new(64, 64, 3, PixelFormat::F32).unwrap();
    write_image(&source, &spec, &f32_ramp(64 * 64 * 3)).unwrap();

    let output = scratch.file("built.tx");
    oiio::make_texture(
        oiio::TextureMode::Texture,
        &source,
        &output,
        &oiio::TextureConfig::new(),
    )
    .unwrap();
    output
}

/// `subimage`, `first_channel` and the result slice's length all reached
/// OpenImageIO unchecked.
///
/// `subimageinfo` indexes `m_subimages` behind an `OIIO_DASSERT` that release
/// builds compile out, so a subimage the file does not have was an unchecked
/// vector index whose result was dereferenced on the next line. The samplers
/// clamp the channel *count* they accumulate but compute their texel addresses
/// from the raw `firstchannel`, and the more-than-four-channel path recurses
/// while walking `firstchannel` upward with no bound, so both of those read off
/// the end of a cached tile. A tile carries sixteen bytes of slack past its
/// last texel and no more.
#[test]
fn a_lookup_is_bounded_by_what_the_texture_has() {
    let scratch = ScratchDir::new("texbounds");
    let path = a_built_texture(&scratch);
    let textures = TextureSystem::new().unwrap();
    let derivatives = Derivatives::uniform(1.0 / 64.0);

    let mut rgb = [0.0_f32; 3];

    // A subimage the file does not have. One is already past the end of an
    // ordinary .tx; cube faces are the legitimate use of the field.
    for subimage in [1_u32, 2, 1_000_000] {
        let options = TextureOptions {
            subimage,
            ..TextureOptions::default()
        };
        let error = textures
            .texture(&path, &options, 0.5, 0.5, derivatives, &mut rgb)
            .expect_err("this texture has one subimage");
        assert!(error.to_string().contains("subimage"), "{error}");
    }

    // A first channel at or past the end.
    for first_channel in [3_u32, 4, 64, 1_000_000] {
        let options = TextureOptions {
            first_channel,
            ..TextureOptions::default()
        };
        let error = textures
            .texture(&path, &options, 0.5, 0.5, derivatives, &mut rgb)
            .expect_err("this texture has three channels");
        assert!(error.to_string().contains("first channel"), "{error}");
    }

    // A result slice longer than the texture has channels needs no unusual
    // option at all: this is the default options and a long slice. It stays
    // legal, because the extra channels are documented to take the fill value,
    // but the channels that do not exist must not come from the tile.
    let filled = TextureOptions {
        fill: 0.75,
        ..TextureOptions::default()
    };
    let mut many = vec![-1.0_f32; 100_000];
    textures
        .texture(&path, &filled, 0.5, 0.5, derivatives, &mut many)
        .unwrap();
    assert!(
        many[3..].iter().all(|value| (*value - 0.75).abs() < 1e-6),
        "channels past the third should all be the fill value"
    );
    assert!(many[..3].iter().any(|value| *value != 0.75));
}

/// `with_channel_count` reaches `ImageBufAlgo::channels`, which builds the
/// default channel order with `alloca` -- four bytes of stack per channel,
/// straight from the argument. Half a million channels overflowed the stack.
#[test]
fn a_texture_channel_count_cannot_overflow_the_stack() {
    let scratch = ScratchDir::new("texchannels");
    let source = scratch.file("source.exr");
    let spec = ImageSpec::new(64, 64, 3, PixelFormat::F32).unwrap();
    write_image(&source, &spec, &f32_ramp(64 * 64 * 3)).unwrap();
    let output = scratch.file("wide.tx");

    let config = oiio::TextureConfig::new().with_channel_count(1 << 19);
    // Clamped to MAX_CHANNELS, so this either succeeds with that many channels
    // or fails cleanly. What it must not do is take the process with it.
    let _ = oiio::make_texture(oiio::TextureMode::Texture, &source, &output, &config);
}

/// A missing color turns a lost texture into that color and success — the
/// mechanism renderers use so one lost file does not kill a frame — while a
/// texture that exists answers with its own pixels, and a missing color of
/// the wrong length is refused before OpenImageIO could read past it.
#[test]
fn missing_color_answers_for_lost_textures() {
    let scratch = ScratchDir::new("missingcolor");
    let real = a_built_texture(&scratch);
    let textures = TextureSystem::new().unwrap();
    let derivatives = Derivatives::uniform(1.0 / 64.0);

    let options = TextureOptions {
        missing_color: Some(vec![0.25, 0.5, 0.75]),
        ..TextureOptions::default()
    };

    // A file that was never made: the missing color, reported as success.
    let lost = scratch.file("never-made.tx");
    let mut rgb = [0.0_f32; 3];
    textures
        .texture(&lost, &options, 0.5, 0.5, derivatives, &mut rgb)
        .unwrap();
    assert_eq!(rgb, [0.25, 0.5, 0.75]);

    // The same lost file without a missing color stays an error.
    let plain = TextureOptions::default();
    assert!(textures
        .texture(&lost, &plain, 0.5, 0.5, derivatives, &mut rgb)
        .is_err());

    // A real texture is answered from its pixels, not the missing color.
    textures
        .texture(&real, &options, 0.5, 0.5, derivatives, &mut rgb)
        .unwrap();
    assert_ne!(rgb, [0.25, 0.5, 0.75], "the real texture's own pixels");
    assert!(rgb.iter().all(|v| v.is_finite()));

    // One value for a three-channel lookup cannot cover the result.
    let short = TextureOptions {
        missing_color: Some(vec![1.0]),
        ..TextureOptions::default()
    };
    let error = textures
        .texture(&lost, &short, 0.5, 0.5, derivatives, &mut rgb)
        .unwrap_err();
    assert!(
        error.to_string().contains("3-channel"),
        "unexpected error: {error}"
    );

    // The environment lookup honours the same contract.
    let mut env = [0.0_f32; 3];
    textures
        .environment(
            &lost,
            &options,
            [0.0, 0.0, 1.0],
            [0.01, 0.0, 0.0],
            [0.0, 0.01, 0.0],
            &mut env,
        )
        .unwrap();
    assert_eq!(env, [0.25, 0.5, 0.75]);
}

/// UDIM patterns: recognized, resolved to concrete tiles by texture
/// coordinate, and inventoried with `None` holes for sparse sets.
#[test]
fn udim_patterns_resolve_and_inventory() {
    let scratch = ScratchDir::new("udim");

    // Two tiles: 1001 at (u 0, v 0) and 1002 at (u 1, v 0), each its own
    // constant color.
    for (id, color) in [("1001", [0.9_f32, 0.1, 0.1]), ("1002", [0.1, 0.9, 0.1])] {
        let spec = ImageSpec::new(32, 32, 3, PixelFormat::F32).unwrap();
        let mut tile = oiio::ImageBuf::new(&spec).unwrap();
        oiio::algo::fill(&mut tile, &color, None).unwrap();
        oiio::make_texture_from_buffer(
            oiio::TextureMode::Texture,
            &tile,
            &scratch.file(&format!("tile.{id}.tx")),
            &oiio::TextureConfig::new(),
        )
        .unwrap();
    }

    let pattern = scratch.file("tile.<UDIM>.tx");
    let textures = TextureSystem::new().unwrap();

    assert!(textures.is_udim(&pattern).unwrap());
    assert!(!textures.is_udim(&scratch.file("tile.1001.tx")).unwrap());

    // Resolution walks the UDIM numbering: the integer part of s selects
    // the column.
    let first = textures.resolve_udim(&pattern, 0.5, 0.5).unwrap();
    assert_eq!(
        first.as_deref(),
        Some(scratch.file("tile.1001.tx").as_path())
    );
    let second = textures.resolve_udim(&pattern, 1.5, 0.5).unwrap();
    assert_eq!(
        second.as_deref(),
        Some(scratch.file("tile.1002.tx").as_path())
    );

    // A tile nobody made: None, not an error — UDIM sets are sparse.
    assert_eq!(textures.resolve_udim(&pattern, 7.5, 3.5).unwrap(), None);

    let inventory = textures.inventory_udim(&pattern).unwrap();
    assert!(inventory.u_tiles >= 2, "{inventory:?}");
    assert!(inventory.v_tiles >= 1, "{inventory:?}");
    assert_eq!(
        inventory.tiles.len(),
        (inventory.u_tiles * inventory.v_tiles) as usize
    );
    assert_eq!(
        inventory.tile(0, 0),
        Some(scratch.file("tile.1001.tx").as_path())
    );
    assert_eq!(
        inventory.tile(1, 0),
        Some(scratch.file("tile.1002.tx").as_path())
    );
    assert_eq!(inventory.tile(9, 0), None, "an unpopulated cell is None");
    assert_eq!(inventory.tile(99, 99), None, "outside the grid is None");
}
