//! Generators and drawing.
//!
//! Text needs a font, which not every machine and not every OpenImageIO build
//! has, so those tests accept a failure and check it is reported rather than
//! asserting on glyphs. Everything else is unconditional.

mod common;

use oiio::algo::{Noise, TextAlignX, TextAlignY, TextOptions};
use oiio::{algo, ImageBuf, ImageSpec, PixelFormat, Roi};

fn canvas(width: u32, height: u32, channels: u32) -> ImageBuf {
    let spec = ImageSpec::new(width, height, channels, PixelFormat::F32).unwrap();
    let mut image = ImageBuf::new(&spec).unwrap();
    algo::zero(&mut image, None).unwrap();
    image
}

fn pixel(image: &ImageBuf, x: i32, y: i32) -> Vec<f32> {
    let channels = image.spec().unwrap().channel_count();
    let roi = Roi::new(x..x + 1, y..y + 1, 0..1, 0..channels).unwrap();
    let mut values = vec![0.0_f32; channels as usize];
    image.get_pixels_into(roi, &mut values).unwrap();
    values
}

#[test]
fn a_gradient_runs_from_top_to_bottom() {
    let mut image = canvas(4, 8, 1);
    algo::fill_gradient(&mut image, &[0.0], &[1.0], None).unwrap();

    assert_eq!(pixel(&image, 0, 0)[0], 0.0, "the top is the first colour");
    assert_eq!(pixel(&image, 0, 7)[0], 1.0, "the bottom is the second");
    let middle = pixel(&image, 0, 4)[0];
    assert!(
        middle > 0.4 && middle < 0.7,
        "the middle should be about half way, got {middle}"
    );
}

/// The ramp covers the region, not the image, so a region gets a whole
/// gradient rather than a slice of a longer one.
#[test]
fn a_gradient_is_measured_over_its_region() {
    let mut image = canvas(4, 8, 1);
    let lower_half = Roi::new(0..4, 4..8, 0..1, 0..1).unwrap();
    algo::fill_gradient(&mut image, &[0.0], &[1.0], Some(lower_half)).unwrap();

    assert_eq!(
        pixel(&image, 0, 4)[0],
        0.0,
        "the region's own top should be the first colour, not a midpoint"
    );
    assert_eq!(pixel(&image, 0, 7)[0], 1.0);
    assert_eq!(pixel(&image, 0, 0)[0], 0.0, "outside is untouched");
}

#[test]
fn four_corners_interpolate() {
    let mut image = canvas(8, 8, 1);
    algo::fill_corners(&mut image, &[0.0], &[1.0], &[1.0], &[0.0], None).unwrap();

    assert_eq!(pixel(&image, 0, 0)[0], 0.0);
    assert_eq!(pixel(&image, 7, 0)[0], 1.0);
    assert_eq!(pixel(&image, 0, 7)[0], 1.0);
    assert_eq!(pixel(&image, 7, 7)[0], 0.0);
}

#[test]
fn a_checkerboard_alternates() {
    let mut image = canvas(8, 8, 1);
    algo::checker(&mut image, [2, 2, 1], &[0.0], &[1.0], [0, 0, 0], None).unwrap();

    let a = pixel(&image, 0, 0)[0];
    let b = pixel(&image, 2, 0)[0];
    let c = pixel(&image, 0, 2)[0];
    println!("checker: {a}, {b}, {c}");
    assert_ne!(a, b, "neighbouring squares should differ across");
    assert_ne!(a, c, "and down");
    assert_eq!(
        pixel(&image, 1, 1)[0],
        a,
        "inside one square the colour is constant"
    );
}

/// The square sizes divide the coordinate, and OpenImageIO does not check
/// them, so a zero would be a division by zero.
#[test]
fn a_checkerboard_square_cannot_be_empty() {
    let mut image = canvas(8, 8, 1);
    let error = algo::checker(&mut image, [0, 2, 1], &[0.0], &[1.0], [0, 0, 0], None).unwrap_err();
    println!("zero square rejected as: {error}");
    assert!(error.to_string().contains("at least 1"), "{error}");

    assert!(algo::checker(&mut image, [2, 2, 0], &[0.0], &[1.0], [0, 0, 0], None).is_err());
}

#[test]
fn noise_varies_the_image() {
    let mut image = canvas(16, 16, 1);
    algo::noise(
        &mut image,
        Noise::Uniform { min: 0.0, max: 1.0 },
        false,
        1,
        None,
    )
    .unwrap();

    let stats = algo::pixel_stats(&image, None).unwrap();
    println!("uniform noise: {} to {}", stats.min[0], stats.max[0]);
    assert!(stats.max[0] > stats.min[0], "noise should vary");
    assert!(stats.min[0] >= 0.0 && stats.max[0] <= 1.0);
}

/// The same seed gives the same noise, and a different one does not.
#[test]
fn noise_is_repeatable_from_its_seed() {
    let generate = |seed: i32| {
        let mut image = canvas(8, 8, 1);
        algo::noise(
            &mut image,
            Noise::Uniform { min: 0.0, max: 1.0 },
            false,
            seed,
            None,
        )
        .unwrap();
        pixel(&image, 3, 3)[0]
    };

    assert_eq!(generate(7), generate(7), "one seed, one result");
    assert_ne!(generate(7), generate(8), "another seed, another result");
}

/// Noise is added to what is already there rather than replacing it, which is
/// easy to mistake for a generator.
#[test]
fn noise_adds_to_what_is_already_there() {
    let mut image = canvas(8, 8, 1);
    algo::fill(&mut image, &[10.0], None).unwrap();
    algo::noise(
        &mut image,
        Noise::Uniform { min: 0.0, max: 1.0 },
        false,
        1,
        None,
    )
    .unwrap();

    let stats = algo::pixel_stats(&image, None).unwrap();
    println!(
        "noise over a fill of 10: {} to {}",
        stats.min[0], stats.max[0]
    );
    assert!(
        stats.min[0] >= 10.0,
        "the noise should have been added to the 10 already there, got {}",
        stats.min[0]
    );
}

/// Salt is the exception: it assigns to a portion of the pixels.
#[test]
fn salt_noise_assigns_to_a_portion() {
    let mut image = canvas(32, 32, 1);
    algo::noise(
        &mut image,
        Noise::Salt {
            value: 1.0,
            portion: 0.5,
        },
        false,
        1,
        None,
    )
    .unwrap();

    let stats = algo::pixel_stats(&image, None).unwrap();
    println!("salt at half: average {}", stats.average[0]);
    assert!(
        stats.average[0] > 0.2 && stats.average[0] < 0.8,
        "about half the pixels should have been set, got an average of {}",
        stats.average[0]
    );
    assert_eq!(stats.max[0], 1.0);
}

#[test]
fn a_point_and_a_line_are_drawn() {
    let mut image = canvas(16, 16, 1);
    algo::render_point(&mut image, 4, 5, &[1.0], None).unwrap();
    assert_eq!(pixel(&image, 4, 5)[0], 1.0);
    assert_eq!(pixel(&image, 5, 5)[0], 0.0);

    let mut lined = canvas(16, 16, 1);
    algo::render_line(&mut lined, [0, 0], [15, 0], &[1.0], false, None).unwrap();
    assert_eq!(pixel(&lined, 0, 0)[0], 1.0, "the line includes its start");
    assert_eq!(pixel(&lined, 15, 0)[0], 1.0, "and its end");
    assert_eq!(pixel(&lined, 7, 0)[0], 1.0);
    assert_eq!(pixel(&lined, 7, 1)[0], 0.0, "and nothing off it");
}

/// A point outside the region is a silent no-op rather than an error, which is
/// OpenImageIO's own behaviour.
#[test]
fn a_point_outside_the_image_draws_nothing() {
    let mut image = canvas(8, 8, 1);
    algo::render_point(&mut image, 100, 100, &[1.0], None).unwrap();
    let stats = algo::pixel_stats(&image, None).unwrap();
    assert_eq!(stats.max[0], 0.0, "nothing should have been drawn");
}

#[test]
fn a_box_is_outlined_or_filled() {
    let mut outline = canvas(16, 16, 1);
    algo::render_box(&mut outline, [2, 2], [6, 6], &[1.0], false, None).unwrap();
    assert_eq!(pixel(&outline, 2, 2)[0], 1.0, "a corner");
    assert_eq!(
        pixel(&outline, 6, 6)[0],
        1.0,
        "the opposite corner, included"
    );
    assert_eq!(pixel(&outline, 4, 2)[0], 1.0, "the top edge");
    assert_eq!(pixel(&outline, 4, 4)[0], 0.0, "but not the middle");

    let mut filled = canvas(16, 16, 1);
    algo::render_box(&mut filled, [2, 2], [6, 6], &[1.0], true, None).unwrap();
    assert_eq!(pixel(&filled, 4, 4)[0], 1.0, "filled reaches the middle");
}

/// The filled path intersects an empty region for reversed corners and draws
/// nothing while reporting success. The outline path handles either order.
#[test]
fn a_filled_box_needs_its_corners_in_order() {
    let mut image = canvas(16, 16, 1);
    let error = algo::render_box(&mut image, [6, 6], [2, 2], &[1.0], true, None).unwrap_err();
    println!("reversed corners rejected as: {error}");

    // An outline is happy either way round.
    algo::render_box(&mut image, [6, 6], [2, 2], &[1.0], false, None).unwrap();
    assert_eq!(pixel(&image, 2, 2)[0], 1.0);
}

/// Text with no glyphs leaves OpenImageIO's measured box inverted, and it
/// builds an image from that box without checking, underflowing its width.
#[test]
fn text_with_nothing_to_draw_is_refused() {
    let mut image = canvas(64, 32, 1);

    for text in ["", "\n", "\r\n\r\n"] {
        let error = algo::render_text(&mut image, [4, 20], text, &TextOptions::default(), None)
            .unwrap_err();
        println!("{text:?} rejected as: {error}");
    }

    assert!(algo::text_size("", 16, "").is_err());
    assert!(algo::text_size("\n", 16, "").is_err());
}

#[test]
fn a_font_size_of_zero_is_refused() {
    let mut image = canvas(64, 32, 1);
    let options = TextOptions {
        size: 0,
        ..TextOptions::default()
    };
    assert!(algo::render_text(&mut image, [4, 20], "hello", &options, None).is_err());
    assert!(algo::text_size("hello", 0, "").is_err());
}

/// Drawing text needs a font, which this build may not have. Either outcome is
/// acceptable; silently doing nothing is not.
#[test]
fn text_is_drawn_or_the_reason_is_reported() {
    let mut image = canvas(128, 48, 1);
    let options = TextOptions {
        size: 24,
        color: &[1.0],
        align_x: TextAlignX::Center,
        align_y: TextAlignY::Center,
        shadow: 0,
        ..TextOptions::default()
    };

    match algo::render_text(&mut image, [64, 24], "Ag", &options, None) {
        Ok(()) => {
            let stats = algo::pixel_stats(&image, None).unwrap();
            println!("drew text, brightest pixel {}", stats.max[0]);
            assert!(
                stats.max[0] > 0.0,
                "if render_text succeeded it should have marked the image"
            );

            // And the measurement should agree that there was something to
            // draw. OpenImageIO measures only x and y here and leaves z and
            // the channels as empty 0..0 ranges, so the region it hands back
            // is not one a caller could use; the binding completes it.
            let measured = algo::text_size("Ag", 24, "").unwrap();
            println!(
                "measured {}x{}, z {:?}, channels {:?}",
                measured.width(),
                measured.height(),
                measured.z(),
                measured.channels()
            );
            assert!(measured.width() > 0 && measured.height() > 0);
            assert_eq!(measured.z(), 0..1, "text is two-dimensional");
            assert_eq!(
                measured.channels(),
                0..1,
                "and measured once, not per channel"
            );
        }
        Err(error) => {
            println!("no font available, reported as: {error}");
        }
    }
}

#[test]
fn a_font_that_does_not_exist_is_reported() {
    let mut image = canvas(64, 32, 1);
    let options = TextOptions {
        font: "no-such-font-anywhere.ttf",
        ..TextOptions::default()
    };
    let outcome = algo::render_text(&mut image, [4, 20], "hello", &options, None);
    assert!(outcome.is_err(), "an unfindable font should be an error");
    println!("missing font reported as: {}", outcome.unwrap_err());
}
