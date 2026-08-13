//! Rotation and warping.
//!
//! The right-angle rotations take a region of the *source*, and the arbitrary
//! rotation takes radians clockwise; both are easy to get backwards, so both
//! are pinned down here.

mod common;

use oiio::algo::WarpOptions;
use oiio::{algo, ImageBuf, ImageSpec, PixelFormat, Roi};
use std::f32::consts::PI;

/// A single-channel image whose pixel values encode their own position, so a
/// rotation is visible in the values rather than only in the dimensions.
fn positional(width: u32, height: u32) -> ImageBuf {
    let spec = ImageSpec::new(width, height, 1, PixelFormat::F32).unwrap();
    let mut image = ImageBuf::new(&spec).unwrap();
    let values: Vec<f32> = (0..height)
        .flat_map(|y| (0..width).map(move |x| (y * width + x) as f32))
        .collect();
    image
        .set_pixels(spec.data_window().unwrap(), &values)
        .unwrap();
    image
}

/// A box filter one pixel wide, so a test can assert exact pixel values
/// rather than whatever a wide filter blended in.
fn box_filter() -> WarpOptions<'static> {
    WarpOptions {
        filter: Some("box"),
        filter_width: Some(1.0),
        ..WarpOptions::default()
    }
}

fn value(image: &ImageBuf, x: i32, y: i32) -> f32 {
    let roi = Roi::new(x..x + 1, y..y + 1, 0..1, 0..1).unwrap();
    let mut one = [0.0_f32; 1];
    image.get_pixels_into(roi, &mut one).unwrap();
    one[0]
}

fn all_values(image: &ImageBuf) -> Vec<f32> {
    let spec = image.spec().unwrap();
    let roi = spec.data_window().unwrap();
    let mut values = vec![0.0_f32; roi.element_count().unwrap()];
    image.get_pixels_into(roi, &mut values).unwrap();
    values
}

#[test]
fn rotate_90_swaps_the_dimensions_and_moves_the_corner() {
    let source = positional(4, 2);
    let mut result = ImageBuf::empty().unwrap();
    algo::rotate_90(&mut result, &source, None).unwrap();

    let spec = result.spec().unwrap();
    assert_eq!(spec.dimensions(), [2, 4, 1], "a quarter turn swaps them");

    // Clockwise: the source's top-left goes to the result's top-right.
    assert_eq!(value(&source, 0, 0), 0.0);
    assert_eq!(value(&result, spec.dimensions()[0] as i32 - 1, 0), 0.0);
}

#[test]
fn two_half_turns_are_the_identity() {
    let source = positional(5, 3);

    let mut once = ImageBuf::empty().unwrap();
    algo::rotate_180(&mut once, &source, None).unwrap();
    let mut twice = ImageBuf::empty().unwrap();
    algo::rotate_180(&mut twice, &once, None).unwrap();

    assert_eq!(twice.spec().unwrap().dimensions(), [5, 3, 1]);
    assert_eq!(
        all_values(&twice),
        all_values(&source),
        "rotating twice by 180 should return the original"
    );
}

#[test]
fn three_quarter_turns_make_a_three_quarter_turn() {
    let source = positional(4, 3);

    let mut step = ImageBuf::empty().unwrap();
    algo::rotate_90(&mut step, &source, None).unwrap();
    let mut step2 = ImageBuf::empty().unwrap();
    algo::rotate_90(&mut step2, &step, None).unwrap();
    let mut step3 = ImageBuf::empty().unwrap();
    algo::rotate_90(&mut step3, &step2, None).unwrap();

    let mut direct = ImageBuf::empty().unwrap();
    algo::rotate_270(&mut direct, &source, None).unwrap();

    assert_eq!(direct.spec().unwrap().dimensions(), [3, 4, 1]);
    assert_eq!(all_values(&direct), all_values(&step3));
}

/// The region names part of the source, so a smaller region gives a smaller
/// result even though nothing about the destination was said.
#[test]
fn the_right_angle_rotations_take_a_source_region() {
    let source = positional(8, 8);
    let quarter = Roi::new(0..4, 0..2, 0..1, 0..1).unwrap();

    let mut result = ImageBuf::empty().unwrap();
    algo::rotate_90(&mut result, &source, Some(quarter)).unwrap();

    assert_eq!(
        result.spec().unwrap().dimensions(),
        [2, 4, 1],
        "the 4x2 source region should rotate into a 2x4 result"
    );
}

#[test]
fn reorient_undoes_the_orientation_attribute() {
    // Orientation 6 is the usual "camera held sideways": stored rotated 90
    // degrees counter-clockwise from how it should be shown.
    let spec = ImageSpec::new(4, 2, 1, PixelFormat::F32)
        .unwrap()
        .with_attribute("Orientation", 6);
    let mut source = ImageBuf::new(&spec).unwrap();
    algo::fill(&mut source, &[0.5], None).unwrap();

    let mut result = ImageBuf::empty().unwrap();
    algo::reorient(&mut result, &source).unwrap();

    assert_eq!(
        result.spec().unwrap().dimensions(),
        [2, 4, 1],
        "orientation 6 turns a 4x2 into a 2x4"
    );

    // Orientation 1 already means "as stored", so nothing moves.
    let upright = ImageSpec::new(4, 2, 1, PixelFormat::F32)
        .unwrap()
        .with_attribute("Orientation", 1);
    let mut plain = ImageBuf::new(&upright).unwrap();
    algo::fill(&mut plain, &[0.5], None).unwrap();
    let mut unchanged = ImageBuf::empty().unwrap();
    algo::reorient(&mut unchanged, &plain).unwrap();
    assert_eq!(unchanged.spec().unwrap().dimensions(), [4, 2, 1]);
}

/// OpenImageIO returns false here without recording anything, so the binding
/// has to supply the reason itself or the caller gets an empty message.
#[test]
fn reorient_reports_an_orientation_it_does_not_know() {
    let spec = ImageSpec::new(4, 2, 1, PixelFormat::F32)
        .unwrap()
        .with_attribute("Orientation", 99);
    let mut source = ImageBuf::new(&spec).unwrap();
    algo::fill(&mut source, &[0.5], None).unwrap();

    let mut result = ImageBuf::empty().unwrap();
    let error = algo::reorient(&mut result, &source).unwrap_err();
    println!("unknown orientation reported as: {error}");
    let message = error.to_string();
    assert!(
        message.contains("99") || message.contains("Orientation"),
        "the message should name the orientation, got {message}"
    );
    assert!(
        !message.contains("did not provide an error message"),
        "OpenImageIO records nothing here, so the binding must; got {message}"
    );
}

#[test]
fn rotating_by_zero_changes_nothing() {
    let source = positional(8, 8);
    let mut result = ImageBuf::empty().unwrap();
    algo::rotate(
        &mut result,
        &source,
        0.0,
        None,
        &WarpOptions::default(),
        None,
    )
    .unwrap();

    let values = all_values(&result);
    let original = all_values(&source);
    assert_eq!(values.len(), original.len());
    for (got, want) in values.iter().zip(&original) {
        assert!(
            (got - want).abs() < 1e-3,
            "a rotation by zero should be the identity"
        );
    }
}

/// A half turn expressed in radians matches the dedicated operation.
#[test]
fn rotating_by_pi_matches_rotate_180() {
    let source = positional(8, 8);

    let mut arbitrary = ImageBuf::empty().unwrap();
    algo::rotate(
        &mut arbitrary,
        &source,
        PI,
        None,
        &WarpOptions {
            filter: Some("box"),
            filter_width: Some(1.0),
            ..WarpOptions::default()
        },
        None,
    )
    .unwrap();

    let mut exact = ImageBuf::empty().unwrap();
    algo::rotate_180(&mut exact, &source, None).unwrap();

    let a = all_values(&arbitrary);
    let b = all_values(&exact);
    assert_eq!(a.len(), b.len());
    let differing = a
        .iter()
        .zip(&b)
        .filter(|(x, y)| (*x - *y).abs() > 0.5)
        .count();
    println!("{differing} of {} pixels differ", a.len());
    assert!(
        differing * 10 < a.len(),
        "a rotation by pi should land on the same pixels as rotate_180, \
         {differing} of {} differ",
        a.len()
    );
}

#[test]
fn warp_with_the_identity_matrix_changes_nothing() {
    let source = positional(8, 8);
    let identity = [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0];

    let mut result = ImageBuf::empty().unwrap();
    algo::warp(&mut result, &source, &identity, &box_filter(), None).unwrap();

    let values = all_values(&result);
    let original = all_values(&source);
    for (got, want) in values.iter().zip(&original) {
        assert!(
            (got - want).abs() < 1e-3,
            "an identity warp should be the identity, got {values:?}"
        );
    }
}

/// The last row of the matrix translates, which is the simplest way to see the
/// transform actually applied.
#[test]
fn warp_can_translate() {
    let source = positional(8, 8);
    let shift_right_by_two = [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 2.0, 0.0, 1.0];

    let mut result = ImageBuf::empty().unwrap();
    algo::warp(
        &mut result,
        &source,
        &shift_right_by_two,
        &box_filter(),
        None,
    )
    .unwrap();

    // What was at x=0 is now at x=2.
    assert!(
        (value(&result, 2, 0) - value(&source, 0, 0)).abs() < 1e-3,
        "expected the source's first pixel at x=2, got {}",
        value(&result, 2, 0)
    );
    assert!(
        (value(&result, 5, 3) - value(&source, 3, 3)).abs() < 1e-3,
        "expected a two-pixel shift throughout"
    );
}

/// A wrap mode reaches the transform. `warp` is the only way to get one:
/// `rotate` is hard-wired to black.
#[test]
fn warp_honours_the_wrap_mode() {
    let mut source = ImageBuf::new(&ImageSpec::new(8, 8, 1, PixelFormat::F32).unwrap()).unwrap();
    algo::fill(&mut source, &[1.0], None).unwrap();

    let shift = [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 4.0, 0.0, 1.0];
    let warped = |wrap: &str| -> f32 {
        let mut result = ImageBuf::empty().unwrap();
        algo::warp(
            &mut result,
            &source,
            &shift,
            &WarpOptions {
                wrap: Some(wrap),
                ..box_filter()
            },
            None,
        )
        .unwrap();
        // A pixel the shift vacated: black leaves nothing, periodic wraps the
        // image back around into it.
        value(&result, 1, 4)
    };

    let black = warped("black");
    let periodic = warped("periodic");
    println!("vacated pixel: black {black}, periodic {periodic}");
    assert_eq!(black, 0.0, "black wrapping should leave nothing behind");
    assert_eq!(
        periodic, 1.0,
        "periodic wrapping should bring the image back"
    );
}

#[test]
fn st_warp_reads_where_the_coordinate_map_points() {
    let source = positional(8, 8);

    // An ST map that points every pixel at itself is the identity.
    let st_spec = ImageSpec::new(8, 8, 2, PixelFormat::F32).unwrap();
    let mut identity_map = ImageBuf::new(&st_spec).unwrap();
    let mut coordinates = Vec::with_capacity(8 * 8 * 2);
    for y in 0..8 {
        for x in 0..8 {
            coordinates.push((x as f32 + 0.5) / 8.0);
            coordinates.push((y as f32 + 0.5) / 8.0);
        }
    }
    identity_map
        .set_pixels(st_spec.data_window().unwrap(), &coordinates)
        .unwrap();

    let mut result = ImageBuf::empty().unwrap();
    algo::st_warp(
        &mut result,
        &source,
        &identity_map,
        [0, 1],
        [false, false],
        &box_filter(),
        None,
    )
    .unwrap();

    for (x, y) in [(0, 0), (3, 5), (7, 7)] {
        let got = value(&result, x, y);
        let want = value(&source, x, y);
        assert!(
            (got - want).abs() < 1e-3,
            "an identity ST map should reproduce the source at {x},{y}: got {got}, want {want}"
        );
    }
}

#[test]
fn st_warp_needs_a_coordinate_map() {
    let source = positional(4, 4);
    let empty = ImageBuf::empty().unwrap();
    let mut result = ImageBuf::empty().unwrap();
    let error = algo::st_warp(
        &mut result,
        &source,
        &empty,
        [0, 1],
        [false, false],
        &WarpOptions::default(),
        None,
    )
    .unwrap_err();
    println!("missing ST map reported as: {error}");
}
