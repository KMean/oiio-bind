//! Resizing, compositing, and channel layout.

mod common;

use oiio::algo::{self, ChannelSource, FitMode};
use oiio::{Error, ImageBuf, ImageSpec, PixelFormat};

fn spec(width: u32, height: u32, channels: u32) -> ImageSpec {
    ImageSpec::new(width, height, channels, PixelFormat::F32).unwrap()
}

fn filled(width: u32, height: u32, values: &[f32]) -> ImageBuf {
    let mut image = ImageBuf::new(&spec(width, height, values.len() as u32)).unwrap();
    algo::fill(&mut image, values, None).unwrap();
    image
}

fn pixels_of(image: &ImageBuf) -> Vec<f32> {
    let roi = image.spec().unwrap().data_window().unwrap();
    let mut values = vec![0.0_f32; roi.element_count().unwrap()];
    image.get_pixels_into(roi, &mut values).unwrap();
    values
}

#[test]
fn resize_uses_the_destinations_dimensions() {
    let source = filled(16, 16, &[0.5, 0.25, 0.75]);

    // The destination decides the output size.
    let mut smaller = ImageBuf::new(&spec(8, 8, 3)).unwrap();
    algo::resize(&mut smaller, &source, None, None, None).unwrap();
    assert_eq!(smaller.spec().unwrap().dimensions(), [8, 8, 1]);

    // A flat source stays flat whatever the filter does.
    for value in pixels_of(&smaller).chunks(3) {
        assert!((value[0] - 0.5).abs() < 1e-5, "got {value:?}");
        assert!((value[1] - 0.25).abs() < 1e-5);
        assert!((value[2] - 0.75).abs() < 1e-5);
    }
}

#[test]
fn resize_accepts_a_named_filter() {
    let source = filled(16, 16, &[1.0, 1.0, 1.0]);

    for filter in ["box", "triangle", "lanczos3", "blackman-harris"] {
        let mut destination = ImageBuf::new(&spec(8, 8, 3)).unwrap();
        algo::resize(&mut destination, &source, Some(filter), None, None)
            .unwrap_or_else(|error| panic!("filter {filter} failed: {error}"));
        assert_eq!(destination.spec().unwrap().dimensions(), [8, 8, 1]);
    }
}

#[test]
fn an_unknown_filter_is_reported() {
    let source = filled(8, 8, &[1.0, 1.0, 1.0]);
    let mut destination = ImageBuf::new(&spec(4, 4, 3)).unwrap();

    let result = algo::resize(
        &mut destination,
        &source,
        Some("definitely-not-a-filter"),
        None,
        None,
    );
    assert!(result.is_err(), "an unknown filter should not be accepted");
}

#[test]
fn resample_changes_size_without_a_filter() {
    let source = filled(16, 8, &[0.25, 0.5, 0.75]);
    let mut destination = ImageBuf::new(&spec(8, 4, 3)).unwrap();

    algo::resample(&mut destination, &source, true, None).unwrap();
    assert_eq!(destination.spec().unwrap().dimensions(), [8, 4, 1]);
}

#[test]
fn fit_preserves_the_aspect_ratio() {
    // A wide source into a square destination: letterboxing keeps the shape.
    let source = filled(16, 8, &[1.0, 1.0, 1.0]);
    let mut destination = ImageBuf::new(&spec(8, 8, 3)).unwrap();

    algo::fit(
        &mut destination,
        &source,
        None,
        None,
        FitMode::Letterbox,
        false,
        None,
    )
    .unwrap();

    // A 2:1 source fitted into an 8x8 frame yields 8x4 of pixel data. The
    // letterboxing is expressed by the display window staying 8x8, with the
    // data window sitting inside it, rather than by padding the pixels.
    let fitted = destination.spec().unwrap();
    assert_eq!(fitted.dimensions(), [8, 4, 1], "aspect ratio preserved");
    assert_eq!(fitted.full_dimensions(), [8, 8, 1], "frame is still square");
    assert_eq!(
        fitted.origin()[1],
        2,
        "the data window is centred in the frame"
    );
}

#[test]
fn zover_picks_the_closer_surface_and_scale_masks() {
    // R,G,B,A,Z with the alpha and depth channels designated — naming a
    // channel "Z" alone does not set the spec's z_channel index, and zover
    // requires it.
    let deep_spec = |z: f32| {
        let spec = ImageSpec::new(4, 4, 5, PixelFormat::F32)
            .unwrap()
            .with_channel_names(["R", "G", "B", "A", "Z"])
            .unwrap()
            .with_alpha_channel(Some(3))
            .unwrap()
            .with_z_channel(Some(4))
            .unwrap();
        let mut image = ImageBuf::new(&spec).unwrap();
        algo::fill(&mut image, &[0.0, 0.0, 0.0, 1.0, z], None).unwrap();
        image
    };
    let mut near = deep_spec(1.0);
    let far = deep_spec(9.0);
    // Make the near surface red so the winner is visible.
    algo::fill(&mut near, &[1.0, 0.0, 0.0, 1.0, 1.0], None).unwrap();

    let mut result = ImageBuf::empty().unwrap();
    algo::zover(&mut result, &far, &near, false, None).unwrap();
    let winner = pixels_of(&result);
    assert!(
        (winner[0] - 1.0).abs() < 1e-5,
        "the closer red surface wins"
    );

    // A channel-count mismatch is refused, not quietly mangled.
    let mut result = ImageBuf::empty().unwrap();
    assert!(algo::zover(
        &mut result,
        &near,
        &filled(4, 4, &[0.5, 0.5, 0.5]),
        false,
        None
    )
    .is_err());

    // scale: a single-channel mask multiplies every channel.
    let source = filled(4, 4, &[0.8, 0.4, 0.2]);
    let mask = filled(4, 4, &[0.5]);
    let mut masked = ImageBuf::empty().unwrap();
    algo::scale(&mut masked, &source, &mask, None).unwrap();
    for value in pixels_of(&masked).chunks(3) {
        assert!((value[0] - 0.4).abs() < 1e-5, "got {value:?}");
        assert!((value[1] - 0.2).abs() < 1e-5);
        assert!((value[2] - 0.1).abs() < 1e-5);
    }
}

#[test]
fn channel_append_concatenates_and_chan_reductions_pick_extremes() {
    let rgb = filled(4, 4, &[0.1, 0.2, 0.3]);
    let alpha = filled(4, 4, &[0.9]);

    // Append: three channels then one, into an empty destination only.
    let mut layered = ImageBuf::empty().unwrap();
    algo::channel_append(&mut layered, &rgb, &alpha).unwrap();
    assert_eq!(layered.channel_count(), 4);
    let values = pixels_of(&layered);
    assert!((values[0] - 0.1).abs() < 1e-5 && (values[3] - 0.9).abs() < 1e-5);

    let mut allocated = ImageBuf::new(&spec(4, 4, 4)).unwrap();
    assert!(
        algo::channel_append(&mut allocated, &rgb, &alpha).is_err(),
        "a pre-allocated destination keeps its shape while the kernel writes another"
    );

    // maxchan/minchan reduce to one channel.
    let mut maxed = ImageBuf::empty().unwrap();
    algo::maxchan(&mut maxed, &rgb, None).unwrap();
    assert_eq!(maxed.channel_count(), 1);
    assert!((pixels_of(&maxed)[0] - 0.3).abs() < 1e-5);

    let mut minned = ImageBuf::empty().unwrap();
    algo::minchan(&mut minned, &rgb, None).unwrap();
    assert!((pixels_of(&minned)[0] - 0.1).abs() < 1e-5);

    // A channel range naming nothing is refused, not answered from a[chbegin].
    let mut result = ImageBuf::empty().unwrap();
    let bad = oiio::Roi::new(0..4, 0..4, 0..1, 5..8).unwrap();
    assert!(algo::maxchan(&mut result, &rgb, Some(bad)).is_err());
}

#[test]
fn repremult_needs_alpha_and_round_trips() {
    // Half-opaque premultiplied grey.
    let premultiplied = filled(4, 4, &[0.25, 0.25, 0.25, 0.5]);

    // unpremult then repremult lands back where it started, keeping the
    // zero-alpha semantics that plain premult would not.
    let mut unpremultiplied = ImageBuf::empty().unwrap();
    algo::unpremult(&mut unpremultiplied, &premultiplied, None).unwrap();
    let mut back = ImageBuf::empty().unwrap();
    algo::repremult(&mut back, &unpremultiplied, None).unwrap();
    for value in pixels_of(&back).chunks(4) {
        assert!((value[0] - 0.25).abs() < 1e-5, "got {value:?}");
        assert!((value[3] - 0.5).abs() < 1e-5);
    }

    // No alpha channel: an error, not a silently misplaced paste.
    let no_alpha = filled(4, 4, &[0.5, 0.5, 0.5]);
    let mut result = ImageBuf::empty().unwrap();
    assert!(algo::repremult(&mut result, &no_alpha, None).is_err());
}

#[test]
fn demosaic_decodes_a_flat_mosaic_and_names_its_limits() {
    use oiio::algo::MosaicPattern;

    // A flat mosaic decodes to the same flat grey whatever the cell layout.
    let mosaic = filled(8, 8, &[0.5]);
    let mut decoded = ImageBuf::empty().unwrap();
    algo::demosaic(
        &mut decoded,
        &mosaic,
        MosaicPattern::Bayer,
        "linear",
        "RGGB",
        None,
    )
    .unwrap();
    assert_eq!(decoded.channel_count(), 3);
    let centre = pixels_of(&decoded);
    assert!(
        centre.iter().all(|value| (value - 0.5).abs() < 1e-5),
        "a flat mosaic decodes flat"
    );

    // A layout OpenImageIO does not know is an error, as is a pre-allocated
    // destination, a negative origin, and a lopsided white balance.
    let mut result = ImageBuf::empty().unwrap();
    assert!(algo::demosaic(
        &mut result,
        &mosaic,
        MosaicPattern::Bayer,
        "linear",
        "QQQQ",
        None
    )
    .is_err());
    let mut allocated = ImageBuf::new(&spec(8, 8, 3)).unwrap();
    assert!(algo::demosaic(
        &mut allocated,
        &mosaic,
        MosaicPattern::Bayer,
        "linear",
        "RGGB",
        None
    )
    .is_err());
    let offset = ImageSpec::new(8, 8, 1, PixelFormat::F32)
        .unwrap()
        .with_origin([-4, 0, 0]);
    let shifted = ImageBuf::new(&offset).unwrap();
    let mut result = ImageBuf::empty().unwrap();
    assert!(algo::demosaic(
        &mut result,
        &shifted,
        MosaicPattern::Bayer,
        "linear",
        "RGGB",
        None
    )
    .is_err());
    let mut result = ImageBuf::empty().unwrap();
    assert!(algo::demosaic(
        &mut result,
        &mosaic,
        MosaicPattern::Bayer,
        "linear",
        "RGGB",
        Some(&[1.0, 2.0])
    )
    .is_err());
}

#[test]
fn over_composites_using_alpha() {
    // Foreground: half-opaque white, premultiplied. Background: opaque black.
    let foreground = filled(4, 4, &[0.5, 0.5, 0.5, 0.5]);
    let background = filled(4, 4, &[0.0, 0.0, 0.0, 1.0]);

    let mut result = ImageBuf::empty().unwrap();
    algo::over(&mut result, &foreground, &background, None).unwrap();

    // over = fg + bg * (1 - fg.alpha) = 0.5 + 0 * 0.5 = 0.5 for colour,
    // and alpha = 0.5 + 1 * 0.5 = 1.0.
    for pixel in pixels_of(&result).chunks(4) {
        assert!((pixel[0] - 0.5).abs() < 1e-5, "colour: {pixel:?}");
        assert!((pixel[3] - 1.0).abs() < 1e-5, "alpha: {pixel:?}");
    }
}

#[test]
fn premultiply_round_trips() {
    // Unassociated: full-intensity colour at half alpha.
    let straight = filled(4, 4, &[1.0, 0.5, 0.25, 0.5]);

    let mut premultiplied = ImageBuf::empty().unwrap();
    algo::premult(&mut premultiplied, &straight, None).unwrap();
    for pixel in pixels_of(&premultiplied).chunks(4) {
        assert!((pixel[0] - 0.5).abs() < 1e-5, "{pixel:?}");
        assert!((pixel[1] - 0.25).abs() < 1e-5);
        assert!((pixel[3] - 0.5).abs() < 1e-5, "alpha is left alone");
    }

    let mut back = ImageBuf::empty().unwrap();
    algo::unpremult(&mut back, &premultiplied, None).unwrap();
    for pixel in pixels_of(&back).chunks(4) {
        assert!((pixel[0] - 1.0).abs() < 1e-5, "{pixel:?}");
        assert!((pixel[1] - 0.5).abs() < 1e-5);
    }
}

#[test]
fn channels_reorders_drops_and_adds() {
    let rgb = filled(4, 4, &[0.1, 0.2, 0.3]);

    // RGB to BGRA, inventing an opaque alpha the source does not have.
    let mut bgra = ImageBuf::empty().unwrap();
    algo::channels(
        &mut bgra,
        &rgb,
        &[
            ChannelSource::Channel(2),
            ChannelSource::Channel(1),
            ChannelSource::Channel(0),
            ChannelSource::Constant(1.0),
        ],
        Some(&["B", "G", "R", "A"]),
    )
    .unwrap();

    let spec = bgra.spec().unwrap();
    assert_eq!(spec.channel_count(), 4);
    assert_eq!(spec.channel_names(), ["B", "G", "R", "A"]);
    for pixel in pixels_of(&bgra).chunks(4) {
        assert!((pixel[0] - 0.3).abs() < 1e-5, "{pixel:?}");
        assert!((pixel[1] - 0.2).abs() < 1e-5);
        assert!((pixel[2] - 0.1).abs() < 1e-5);
        assert!((pixel[3] - 1.0).abs() < 1e-5);
    }
}

#[test]
fn channels_can_extract_a_single_channel() {
    let rgb = filled(4, 4, &[0.1, 0.2, 0.3]);

    let mut green = ImageBuf::empty().unwrap();
    algo::channels(&mut green, &rgb, &[ChannelSource::Channel(1)], Some(&["Y"])).unwrap();

    assert_eq!(green.spec().unwrap().channel_count(), 1);
    for value in pixels_of(&green) {
        assert!((value - 0.2).abs() < 1e-5);
    }
}

#[test]
fn channels_rejects_a_mismatched_name_list() {
    let rgb = filled(4, 4, &[0.1, 0.2, 0.3]);
    let mut result = ImageBuf::empty().unwrap();

    assert!(matches!(
        algo::channels(&mut result, &rgb, &[], None),
        Err(Error::InvalidImageSpec(_))
    ));
    assert!(matches!(
        algo::channels(
            &mut result,
            &rgb,
            &[ChannelSource::Channel(0), ChannelSource::Channel(1)],
            Some(&["only-one"]),
        ),
        Err(Error::InvalidImageSpec(_))
    ));
}

#[test]
fn channel_sum_collapses_to_one_channel() {
    let rgb = filled(4, 4, &[0.25, 0.5, 0.25]);

    let mut luminance = ImageBuf::empty().unwrap();
    algo::channel_sum(&mut luminance, &rgb, &[1.0, 1.0, 1.0], None).unwrap();
    assert_eq!(luminance.spec().unwrap().channel_count(), 1);
    for value in pixels_of(&luminance) {
        assert!((value - 1.0).abs() < 1e-5, "got {value}");
    }

    // Weighted, as a luma calculation would be.
    let mut weighted = ImageBuf::empty().unwrap();
    algo::channel_sum(&mut weighted, &rgb, &[2.0, 0.0, 0.0], None).unwrap();
    for value in pixels_of(&weighted) {
        assert!((value - 0.5).abs() < 1e-5, "got {value}");
    }
}

/// A circular shift is a bijection: every pixel lands somewhere, wrapped
/// shifts land where modular arithmetic says, and a pre-allocated
/// destination — whose extra pixels the bijection would never write — is
/// refused.
#[test]
fn circular_shift_wraps_and_requires_an_empty_destination() {
    let mut source = ImageBuf::new(&spec(4, 4, 1)).unwrap();
    for y in 0..4 {
        for x in 0..4 {
            source.set_pixel_at(x, y, &[(x + 4 * y) as f32]).unwrap();
        }
    }

    let mut shifted = ImageBuf::empty().unwrap();
    algo::circular_shift(&mut shifted, &source, [1, 2, 0], None).unwrap();
    for y in 0..4_i32 {
        for x in 0..4_i32 {
            let sx = (x - 1).rem_euclid(4);
            let sy = (y - 2).rem_euclid(4);
            assert_eq!(
                shifted.channel_at(x, y, 0, oiio::Wrap::Default).unwrap(),
                (sx + 4 * sy) as f32,
                "({x}, {y})"
            );
        }
    }

    // The inverse shift restores the original, negative amounts wrapping
    // the other way.
    let mut back = ImageBuf::empty().unwrap();
    algo::circular_shift(&mut back, &shifted, [-1, -2, 0], None).unwrap();
    for y in 0..4 {
        for x in 0..4 {
            assert_eq!(
                back.channel_at(x, y, 0, oiio::Wrap::Default).unwrap(),
                (x + 4 * y) as f32
            );
        }
    }

    let mut allocated = ImageBuf::new(&spec(4, 4, 1)).unwrap();
    let error = algo::circular_shift(&mut allocated, &source, [1, 0, 0], None).unwrap_err();
    assert!(
        error.to_string().contains("empty destination"),
        "unexpected error: {error}"
    );
}
