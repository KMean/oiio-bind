//! Every refusal this crate makes on OpenImageIO's behalf is pinned here.
//!
//! The guards are the product. Each one stands between a caller and an
//! argument that OpenImageIO accepts and then reads or writes out of bounds
//! for, and each carries a comment in `ffi_imagebufalgo.cpp` naming the
//! upstream mechanism it is avoiding.
//!
//! They were almost entirely untested. Disabling `refuse_wide_destination`,
//! `refuse_deep`, `refuse_empty_region` and most of the others left the whole
//! suite passing, which means a refactor could have removed any of them and
//! CI would have stayed green. This file exists so that cannot happen: each
//! test fails if its guard stops refusing.
//!
//! These are deliberately one representative call per guard rather than every
//! call site. The guard is the thing being pinned, not the wiring.

mod common;

use oiio::{algo, ImageBuf, ImageSpec, PixelFormat, Roi};

fn flat(width: u32, height: u32, channels: u32) -> ImageBuf {
    let spec = ImageSpec::new(width, height, channels, PixelFormat::F32).unwrap();
    ImageBuf::new(&spec).unwrap()
}

fn deep(width: u32, height: u32) -> ImageBuf {
    let spec = ImageSpec::new(width, height, 1, PixelFormat::F32)
        .unwrap()
        .with_channel_names(["Z"])
        .unwrap()
        .as_deep();
    ImageBuf::new(&spec).unwrap()
}

/// `refuse_empty_region`, which covers 41 operations. A region that does not
/// overlap the source names no pixels; OpenImageIO sizes the destination from
/// it, gets a buffer with no storage, and then asserts inside its own
/// zeroing, which ends the process rather than returning false.
///
/// The helper has two refusals and they say different things, so each test
/// names its own. Asserting only "does not overlap" passed on either, which
/// left the destination half below with no cover of its own.
#[test]
fn a_region_that_misses_the_source_is_refused() {
    let src = flat(8, 8, 3);
    let mut dst = ImageBuf::empty().unwrap();
    let elsewhere = Roi::new(1000..1008, 1000..1008, 0..1, 0..3).unwrap();

    let error = algo::copy(&mut dst, &src, None, Some(elsewhere))
        .expect_err("a region outside the source must be refused");
    assert!(
        error.to_string().contains("does not overlap the source"),
        "unexpected message: {error}"
    );
}

/// The other half of `refuse_empty_region`: a region that does reach the
/// source but misses a destination that is already allocated. OpenImageIO
/// intersects the region with the destination and hands the kernel the empty
/// result.
///
/// This one is not politeness either. With the branch disabled, the property
/// test does not fail, it dies: 0xc0000409, the stack-cookie check, from
/// inside `resize`, and on another run from the assertion in `deep_paste_`.
/// It was reaching those kernels only through random inputs, though, so this
/// test exists to name the shape.
///
/// `invert` rather than `copy`, because `copy` carries a second guard of its
/// own for an unreachable destination and would answer with that instead.
#[test]
fn a_region_that_misses_an_allocated_destination_is_refused() {
    let src = flat(8, 8, 3);

    // Allocated a long way from the source, so the region below overlaps the
    // source and not the destination.
    let far = ImageSpec::new(8, 8, 3, PixelFormat::F32)
        .unwrap()
        .with_origin([500, 500, 0]);
    let mut dst = ImageBuf::new(&far).unwrap();
    let over_the_source = Roi::new(0..8, 0..8, 0..1, 0..3).unwrap();

    let error = algo::invert(&mut dst, &src, Some(over_the_source))
        .expect_err("a region outside the allocated destination must be refused");
    assert!(
        error
            .to_string()
            .contains("does not overlap the destination"),
        "unexpected message: {error}"
    );
}

/// `refuse_deep`, on an operation that reads flat pixels. A deep image has
/// samples rather than a pixel array to walk.
#[test]
fn a_deep_source_is_refused_where_flat_pixels_are_read() {
    let mut dst = ImageBuf::empty().unwrap();
    let error = algo::transpose(&mut dst, &deep(4, 4), None)
        .expect_err("transpose cannot read a deep image");
    // The guard's own wording. Without the guard this call segfaults, so a
    // looser assertion would pass on OpenImageIO's message where this one
    // cannot pass at all.
    assert!(
        error
            .to_string()
            .contains("the contiguous pixels a deep image does not have"),
        "unexpected message: {error}"
    );
}

/// `refuse_deep_mismatch`, which covers eleven binary operations. One deep
/// operand and one flat is not a combination OpenImageIO handles.
#[test]
fn one_deep_and_one_flat_operand_is_refused() {
    let mut dst = ImageBuf::empty().unwrap();
    let error = algo::add(&mut dst, &flat(4, 4, 1), &deep(4, 4), None)
        .expect_err("a deep and a flat operand cannot be added");
    // OpenImageIO refuses this too, with a worse message, so the test has to
    // name the guard's wording or it passes without the guard.
    assert!(
        error
            .to_string()
            .contains("flatten them first, or use deep_merge or deep_holdout"),
        "unexpected message: {error}"
    );
}

/// `refuse_mixed_deep`, the same idea for `copy` and `paste`, which have
/// their own path.
#[test]
fn copying_across_the_deep_boundary_is_refused() {
    let mut dst = deep(4, 4);
    let error = algo::copy(&mut dst, &flat(4, 4, 1), None, None)
        .expect_err("a flat source cannot be copied into a deep destination");
    assert!(!error.to_string().is_empty());
}

/// `refuse_flat_destination`, for the two deep compositing operations. They
/// need somewhere to put samples.
#[test]
fn a_flat_destination_is_refused_for_a_deep_composite() {
    let mut dst = flat(4, 4, 1);
    let error = algo::deep_merge(&mut dst, &deep(4, 4), &deep(4, 4), true, None)
        .expect_err("deep_merge needs a deep or empty destination");
    // Without the guard OpenImageIO complains about channel names instead,
    // which is true and useless, so the wording is what gets pinned.
    assert!(
        error
            .to_string()
            .contains("pass an empty one or a deep one"),
        "unexpected message: {error}"
    );
}

/// `refuse_wide_destination`, for `resize` and `fit`. `resize_` takes the
/// channel count from the destination spec and runs it over the source
/// iterator, which reads past the source pixel when the destination is wider.
#[test]
fn a_destination_wider_than_the_source_is_refused_by_resize() {
    let src = flat(8, 8, 3);
    let mut dst = flat(4, 4, 4);

    let error = algo::resize(&mut dst, &src, None, None, None)
        .expect_err("a 4 channel destination from a 3 channel source must be refused");
    let text = error.to_string();
    assert!(
        text.contains('4') && text.contains('3'),
        "the error should name both counts: {text}"
    );
}

/// `refuse_narrow_destination`, for `warp` and `rotate`. `warp_` sizes its
/// per-pixel scratch buffer from the destination and then has
/// `filtered_sample` fill it from the source, so a narrower destination is
/// written past the end of a stack allocation.
#[test]
fn a_destination_narrower_than_the_source_is_refused_by_warp() {
    let src = flat(8, 8, 4);
    let mut dst = flat(8, 8, 3);
    let matrix = [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0_f32];

    let error = algo::warp(&mut dst, &src, &matrix, &algo::WarpOptions::default(), None)
        .expect_err("a narrower destination must be refused");
    assert!(
        error.to_string().contains("per-pixel buffer"),
        "the error should name the mechanism: {error}"
    );
}

/// `refuse_bad_chan_reduce`, for `maxchan` and `minchan`. These reduce every
/// channel to one, so a destination that already has more than one has
/// nowhere for the answer to go.
///
/// Note the shape. A channel range starting past the source also reaches
/// this guard in principle, but the crate's own `Roi` validation refuses
/// that first, so a test written that way passes without ever entering C++.
/// A pre-allocated multi-channel destination is the condition only this
/// guard catches.
#[test]
fn a_multi_channel_destination_is_refused_by_maxchan() {
    let src = flat(8, 8, 3);
    let mut dst = flat(8, 8, 3);

    let error = algo::maxchan(&mut dst, &src, None)
        .expect_err("maxchan writes one channel, so a 3 channel destination is wrong");
    assert!(
        error.to_string().contains("one channel"),
        "unexpected message: {error}"
    );
}

/// `refuse_distant_pair`, for twelve binary operations. Two images far enough
/// apart that the union of their data windows dwarfs the pixels they hold
/// would have OpenImageIO allocate the span between them.
#[test]
fn two_images_too_far_apart_to_combine_are_refused() {
    let a = flat(4, 4, 3);

    // The same size, but its data window starts a long way off.
    let far_spec = ImageSpec::new(4, 4, 3, PixelFormat::F32)
        .unwrap()
        .with_origin([1_000_000, 1_000_000, 0]);
    let b = ImageBuf::new(&far_spec).unwrap();

    let mut dst = ImageBuf::empty().unwrap();
    let error = algo::add(&mut dst, &a, &b, None)
        .expect_err("two distant images must be refused rather than allocating the span");
    // Without the guard this call dies with an integer divide by zero, so
    // again the specific wording is the only assertion that distinguishes
    // the guard from its absence.
    assert!(
        error.to_string().contains("sit too far apart to combine"),
        "unexpected message: {error}"
    );
}

/// `reject_deep`, for the five statistics and introspection calls. A deep
/// image has samples rather than a pixel array, so there is nothing
/// contiguous for them to measure.
#[test]
fn a_deep_image_is_refused_by_the_statistics_calls() {
    let image = deep(4, 4);
    let error = algo::constant_color(&image, 0.0, None)
        .expect_err("a deep image has no contiguous pixels to measure");
    assert!(
        error
            .to_string()
            .contains("deep images have no contiguous pixels to measure"),
        "unexpected message: {error}"
    );
}

// The three operations below each carry their own deep-image refusal rather
// than going through a shared helper, and nothing reached any of them. They
// are exposed, so a caller can.

/// `color_map` refuses a deep source: it maps pixel values through knots and
/// a deep image has samples instead.
#[test]
fn color_map_refuses_a_deep_source() {
    let mut dst = ImageBuf::empty().unwrap();
    let knots = [0.0, 0.0, 0.0, 1.0, 1.0, 1.0];
    let error = algo::color_map(&mut dst, &deep(4, 4), Some(0), 2, 3, &knots, None)
        .expect_err("color_map cannot read a deep image");
    assert!(
        error.to_string().contains("deep images are not supported"),
        "unexpected message: {error}"
    );
}

/// `color_range_check` refuses a deep source for the same reason.
#[test]
fn color_range_check_refuses_a_deep_source() {
    let error = algo::color_range_check(&deep(4, 4), &[0.0], &[1.0], None)
        .expect_err("color_range_check cannot read a deep image");
    assert!(
        error.to_string().contains("deep images are not supported"),
        "unexpected message: {error}"
    );
}

/// `fillholes_pushpull` refuses a deep source.
///
/// This asserted only that the message was non-empty, which no `Error` ever
/// is: `Error::operation` substitutes a sentence of its own when OpenImageIO
/// says nothing. OpenImageIO refuses a deep source here too, through
/// IBAprep, so the assertion held with the guard deleted and pinned nothing.
#[test]
fn fillholes_refuses_a_deep_source() {
    let mut dst = ImageBuf::empty().unwrap();
    let error = algo::fillholes_pushpull(&mut dst, &deep(4, 4))
        .expect_err("fillholes cannot read a deep image");
    assert!(
        error
            .to_string()
            .contains("fillholes_pushpull: deep images are not supported"),
        "unexpected message: {error}"
    );
}

/// `copy` refuses a pre-allocated destination whose data window the source
/// never reaches: nothing would be copied and the destination would be left
/// holding whatever it already had, reported as success.
#[test]
fn copy_into_a_destination_the_source_cannot_reach_is_refused() {
    let src = flat(4, 4, 3);

    // A destination allocated a long way from the source.
    let far = ImageSpec::new(4, 4, 3, PixelFormat::F32)
        .unwrap()
        .with_origin([500, 500, 0]);
    let mut dst = ImageBuf::new(&far).unwrap();

    let error = algo::copy(&mut dst, &src, None, None)
        .expect_err("a destination the source cannot reach must be refused");
    assert!(
        error.to_string().contains("does not reach the allocated"),
        "unexpected message: {error}"
    );
}

// The three below are guards that nothing reached. Each was found by
// disabling it and watching all 464 tests stay green, and each is reachable
// from safe Rust: the Rust layer validates plenty, but not these.

/// `check_window`'s height refusal, for `median_filter`, `dilate` and
/// `erode`. Its width twin is tested; the height one never was, because every
/// call in the suite passed `None` for height, which becomes -1 and skips the
/// branch entirely.
///
/// A window of one is not a filter. OpenImageIO computes `w_2 = max(1, 0)`,
/// giving the half-open window `[y - 1, y)`, and returns the image shifted a
/// row under a success return.
#[test]
fn a_window_one_pixel_high_is_refused() {
    let src = flat(8, 8, 1);
    let mut dst = ImageBuf::empty().unwrap();

    let error = algo::median_filter(&mut dst, &src, 3, Some(1), None)
        .expect_err("a height of one is not a filter window");
    assert!(
        error.to_string().contains("at least 2 high"),
        "unexpected message: {error}"
    );
}

/// `color_map`'s knot-count floor. It has two length refusals and only the
/// later one, that the knots slice matches knot_count * channels, was ever
/// reached: both existing assertions pass a product that disagrees with the
/// slice, so they stop there. This one needs a product that agrees.
#[test]
fn a_single_knot_is_refused_by_color_map() {
    let src = flat(8, 8, 3);
    let mut dst = ImageBuf::empty().unwrap();

    // 1 knot x 3 channels == 3 values, so the slice is the right length and
    // only the floor can refuse this.
    let error = algo::color_map(&mut dst, &src, Some(0), 1, 3, &[0.0, 0.5, 1.0], None)
        .expect_err("one knot cannot be interpolated between");
    assert!(
        error.to_string().contains("at least two knots"),
        "unexpected message: {error}"
    );
}

/// `color_region`, the single gate for all six colour shims. A region that
/// starts above channel zero would have the colour engine read its first
/// channel from somewhere the region does not name.
///
/// The destination has to be pre-allocated for this to be the guard that
/// answers: into an empty destination the crate's own `region_in` refuses a
/// non-zero channel start first, with a message so similar that the only test
/// aimed at this guard never entered C++ at all.
#[test]
fn a_colour_region_above_channel_zero_is_refused() {
    let src = flat(8, 8, 6);
    let mut dst = flat(8, 8, 6);
    let mut identity = [0.0_f32; 16];
    for i in 0..4 {
        identity[i * 4 + i] = 1.0;
    }
    let upper = Roi::new(0..8, 0..8, 0..1, 3..6).unwrap();

    let error = algo::color_matrix_transform(&mut dst, &src, &identity, false, Some(upper))
        .expect_err("a colour region must begin at channel zero");
    assert!(
        error.to_string().contains("channel zero"),
        "unexpected message: {error}"
    );
}
