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
#[test]
fn a_region_that_misses_the_source_is_refused() {
    let src = flat(8, 8, 3);
    let mut dst = ImageBuf::empty().unwrap();
    let elsewhere = Roi::new(1000..1008, 1000..1008, 0..1, 0..3).unwrap();

    let error = algo::copy(&mut dst, &src, None, Some(elsewhere))
        .expect_err("a region outside the source must be refused");
    assert!(
        error.to_string().contains("does not overlap"),
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
