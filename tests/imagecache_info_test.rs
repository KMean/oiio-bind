//! The image-info queries and thumbnails: what a cache can say about a file
//! without decoding it, and the postage stamps some formats carry.

mod common;

use common::{f32_ramp, write_image, ScratchDir};
use oiio::{ImageBuf, ImageCache, ImageOutput, ImageSpec, PixelFormat};

/// The plain-file queries: existence, format, counts, and the honest `None`
/// for averages no un-mipped file can answer.
#[test]
fn info_queries_answer_for_a_plain_file() {
    let scratch = ScratchDir::new("cacheinfo");
    let path = scratch.file("plain.exr");
    let spec = ImageSpec::new(16, 8, 3, PixelFormat::F32).unwrap();
    write_image(&path, &spec, &f32_ramp(16 * 8 * 3)).unwrap();

    let cache = ImageCache::new().unwrap();

    assert!(cache.exists(&path).unwrap());
    assert!(!cache.exists(&scratch.file("never-written.exr")).unwrap());

    assert_eq!(cache.file_format(&path).unwrap(), "openexr");
    assert_eq!(cache.subimage_count(&path).unwrap(), 1);
    assert_eq!(cache.mip_level_count(&path, 0).unwrap(), 1);
    assert!(!cache.is_udim(&path).unwrap());

    // No mip pyramid, so no 1×1 level to derive an average from.
    assert_eq!(cache.average_color(&path, 0).unwrap(), None);
    assert_eq!(cache.average_alpha(&path, 0).unwrap(), None);
    assert_eq!(cache.constant_color(&path, 0).unwrap(), None);

    // A missing file is an error for every query but existence.
    let missing = scratch.file("missing.exr");
    assert!(cache.file_format(&missing).is_err());
    assert!(cache.subimage_count(&missing).is_err());
}

/// The texture queries: a `.tx` knows what kind of texture it is, how deep
/// its pyramid goes, and — through the 1×1 level — its average color; one
/// made from a constant image with detection on knows its constant color.
#[test]
fn info_queries_answer_for_a_texture() {
    let scratch = ScratchDir::new("cacheinfotx");

    let spec = ImageSpec::new(64, 64, 4, PixelFormat::F32)
        .unwrap()
        .with_channel_names(["R", "G", "B", "A"])
        .unwrap();
    let mut flat = ImageBuf::new(&spec).unwrap();
    let color = [0.25_f32, 0.5, 0.75, 1.0];
    oiio::algo::fill(&mut flat, &color, None).unwrap();

    let texture = scratch.file("constant.tx");
    oiio::make_texture_from_buffer(
        oiio::TextureMode::Texture,
        &flat,
        &texture,
        &oiio::TextureConfig::new()
            .with_format(PixelFormat::F32)
            .with_constant_color_detect(true),
    )
    .unwrap();

    let cache = ImageCache::new().unwrap();

    assert_eq!(cache.texture_type(&texture).unwrap(), "Plain Texture");
    assert_eq!(cache.texture_format(&texture).unwrap(), "Plain Texture");

    let average = cache
        .average_color(&texture, 0)
        .unwrap()
        .expect("a mipmapped texture has a 1x1 level to average from");
    assert_eq!(average.len(), 4);
    for (channel, (got, wanted)) in average.iter().zip(color).enumerate() {
        assert!(
            (got - wanted).abs() < 1e-3,
            "average channel {channel}: {got} != {wanted}"
        );
    }

    let alpha = cache
        .average_alpha(&texture, 0)
        .unwrap()
        .expect("the texture has an alpha channel");
    assert!((alpha - 1.0).abs() < 1e-3, "average alpha: {alpha}");

    let constant = cache
        .constant_color(&texture, 0)
        .unwrap()
        .expect("constant-color detection marked this texture");
    for (channel, (got, wanted)) in constant.iter().zip(color).enumerate() {
        assert!(
            (got - wanted).abs() < 1e-3,
            "constant channel {channel}: {got} != {wanted}"
        );
    }
    let constant_alpha = cache.constant_alpha(&texture, 0).unwrap();
    assert!(
        constant_alpha.is_some_and(|a| (a - 1.0).abs() < 1e-3),
        "constant alpha: {constant_alpha:?}"
    );
}

/// Thumbnails round-trip through the one format that stores them, and
/// everything else answers honestly.
#[test]
fn thumbnails_round_trip_through_targa() {
    let scratch = ScratchDir::new("thumbnail");

    // Build the thumbnail: a solid color, stored as bytes as Targa will.
    let thumb_spec = ImageSpec::new(8, 8, 3, PixelFormat::U8).unwrap();
    let mut thumb = ImageBuf::new(&thumb_spec).unwrap();
    oiio::algo::fill(&mut thumb, &[1.0, 0.5, 0.0], None).unwrap();

    // A buffer carries a thumbnail like any other metadata.
    let image_spec = ImageSpec::new(32, 32, 3, PixelFormat::U8).unwrap();
    let mut carrier = ImageBuf::new(&image_spec).unwrap();
    assert!(!carrier.has_thumbnail());
    carrier.set_thumbnail(&thumb).unwrap();
    assert!(carrier.has_thumbnail());
    let copy = carrier.thumbnail().unwrap().expect("just attached");
    let mut copied = [0.0_f32; 3];
    copy.pixel_at_into(4, 4, oiio::Wrap::Default, &mut copied)
        .unwrap();
    assert!((copied[0] - 1.0).abs() < 2e-2, "thumbnail red: {copied:?}");
    carrier.clear_thumbnail();
    assert!(!carrier.has_thumbnail());

    // Write a Targa with the thumbnail attached, after the pixels — Targa
    // reports `thumbnail_after_write`, and stores the stamp at close.
    let path = scratch.file("thumbed.tga");
    let mut output = ImageOutput::create(&path, &image_spec).unwrap();
    assert!(output.supports("thumbnail"));
    let pixels = vec![128_u8; 32 * 32 * 3];
    output.write_image(&pixels).unwrap();
    output.set_thumbnail(&thumb).unwrap();
    output.close().unwrap();

    // Read the postage stamp back through the cache.
    let cache = ImageCache::new().unwrap();
    let stamp = cache
        .thumbnail(&path, 0)
        .unwrap()
        .expect("the Targa was written with a thumbnail");
    let stamp_spec = stamp.spec().unwrap();
    assert_eq!(stamp_spec.dimensions(), [8, 8, 1]);
    let mut got = [0.0_f32; 3];
    stamp
        .pixel_at_into(4, 4, oiio::Wrap::Default, &mut got)
        .unwrap();
    // The stamp comes back with red and blue exchanged: through 3.1,
    // OpenImageIO's Targa writer dumps the thumbnail's RGB bytes raw while
    // its reader decodes the stamp as the BGR the TGA format stores.
    // Unreleased 3.2 fixed the writer (targaoutput.cpp converts to BGR,
    // bottom-up, on `main`). Pinned as 3.1 behaves, so a fix arriving in a
    // 3.1 patch breaks this test loudly instead of silently changing
    // written files.
    for (channel, (value, wanted)) in got.iter().zip([0.0, 0.5, 1.0]).enumerate() {
        assert!(
            (value - wanted).abs() < 2e-2,
            "stamp channel {channel}: {value} != {wanted}"
        );
    }

    // A format that stores no thumbnails answers None on read...
    let exr = scratch.file("no-thumb.exr");
    let exr_spec = ImageSpec::new(8, 8, 3, PixelFormat::F32).unwrap();
    write_image(&exr, &exr_spec, &f32_ramp(8 * 8 * 3)).unwrap();
    assert!(cache.thumbnail(&exr, 0).unwrap().is_none());

    // ...and is refused with a clear message on write, where OpenImageIO
    // would fail without recording one.
    let mut exr_out = ImageOutput::create(&scratch.file("refused.exr"), &exr_spec).unwrap();
    let error = exr_out.set_thumbnail(&thumb).unwrap_err();
    assert!(
        error.to_string().contains("does not store thumbnails"),
        "unexpected error: {error}"
    );
}

/// The Targa-specific silent failures are errors here: a channel-count
/// mismatch Targa would refuse without a message, and the 256-pixel stamp
/// OpenImageIO's own downsizing would truncate to zero dimensions.
#[test]
fn oversized_and_mismatched_thumbnails_are_refused() {
    let scratch = ScratchDir::new("thumbguard");
    let image_spec = ImageSpec::new(32, 32, 3, PixelFormat::U8).unwrap();
    let mut output = ImageOutput::create(&scratch.file("guarded.tga"), &image_spec).unwrap();

    // Channel mismatch: a 1-channel stamp on a 3-channel image.
    let gray_spec = ImageSpec::new(8, 8, 1, PixelFormat::U8).unwrap();
    let mut gray = ImageBuf::new(&gray_spec).unwrap();
    oiio::algo::fill(&mut gray, &[0.5], None).unwrap();
    let error = output.set_thumbnail(&gray).unwrap_err();
    assert!(
        error.to_string().contains("channels"),
        "unexpected error: {error}"
    );

    // An oversized stamp: the TGA field is a single byte per dimension, and
    // through 3.1 OpenImageIO's downsizing clamps to 256 — one too many —
    // so this would silently write a zero-dimension thumbnail.
    let big_spec = ImageSpec::new(256, 256, 3, PixelFormat::U8).unwrap();
    let mut big = ImageBuf::new(&big_spec).unwrap();
    oiio::algo::fill(&mut big, &[0.5, 0.5, 0.5], None).unwrap();
    let error = output.set_thumbnail(&big).unwrap_err();
    assert!(
        error.to_string().contains("256"),
        "unexpected error: {error}"
    );
}

/// The sixth review's regressions: a real UDIM pattern answers true (the
/// query name is capitalized "UDIM" — the lowercase spelling OpenImageIO
/// documents is never answered by its implementation), an unreadable file
/// is an error on every thumbnail call rather than melting into None, a
/// stale queued error cannot turn a later documented-None into Err, and a
/// UDIM pattern is refused by the queries that would poison or misread it.
#[test]
fn udim_and_broken_file_queries_answer_honestly() {
    let scratch = ScratchDir::new("sixthcache");
    let cache = ImageCache::new().unwrap();

    // Two real tiles make the pattern a genuine UDIM set.
    for id in ["1001", "1002"] {
        let spec = ImageSpec::new(8, 8, 3, PixelFormat::F32).unwrap();
        write_image(
            &scratch.file(&format!("t.{id}.exr")),
            &spec,
            &f32_ramp(8 * 8 * 3),
        )
        .unwrap();
    }
    let pattern = scratch.file("t.<UDIM>.exr");
    assert!(cache.is_udim(&pattern).unwrap(), "a real UDIM set is UDIM");
    assert!(!cache.is_udim(&scratch.file("t.1001.exr")).unwrap());

    // Consistent tiles agree, so the aggregate count answers.
    assert_eq!(cache.subimage_count(&pattern).unwrap(), 1);

    // The queries that would poison the pattern's cache record or read
    // an aggregate that is not meaningful refuse it instead.
    assert!(cache.thumbnail(&pattern, 0).is_err());
    assert!(cache.file_format(&pattern).is_err());

    // An unreadable file: an error on the first thumbnail call and still
    // an error on the second, where OpenImageIO reports brokenness only
    // once.
    let corrupt = scratch.file("corrupt.exr");
    std::fs::write(&corrupt, b"not an image at all").unwrap();
    assert!(cache.thumbnail(&corrupt, 0).is_err(), "first call");
    assert!(cache.thumbnail(&corrupt, 0).is_err(), "second call");

    // And the stale queued error from touching the corrupt file cannot
    // turn a later documented-None answer into an Err.
    let plain = scratch.file("plain.exr");
    let spec = ImageSpec::new(8, 8, 3, PixelFormat::F32).unwrap();
    write_image(&plain, &spec, &f32_ramp(8 * 8 * 3)).unwrap();
    assert_eq!(cache.constant_color(&plain, 0).unwrap(), None);
    assert!(cache.thumbnail(&plain, 0).unwrap().is_none());
}
