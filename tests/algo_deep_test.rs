//! Deep compositing, and the `ImageBuf` sample access it needs.

mod common;

use oiio::{algo, ImageBuf, ImageSpec, PixelFormat, Roi};

/// A deep image with `samples` samples in every pixel, each a little further
/// away and a little less opaque than the last.
///
/// Channels are R, G, B, A, Z, which is what the deep operations look for by
/// name.
fn deep_image(width: u32, height: u32, samples: u32) -> ImageBuf {
    let spec = ImageSpec::new(width, height, 5, PixelFormat::F32)
        .unwrap()
        .with_channel_names(["R", "G", "B", "A", "Z"])
        .unwrap()
        .as_deep();
    let mut image = ImageBuf::new(&spec).unwrap();

    for y in 0..height as i32 {
        for x in 0..width as i32 {
            image.set_deep_sample_count(x, y, samples).unwrap();
            for s in 0..samples {
                let depth = 1.0 + s as f32;
                image.set_deep_value(x, y, 0, s, 0.5).unwrap();
                image.set_deep_value(x, y, 1, s, 0.25).unwrap();
                image.set_deep_value(x, y, 2, s, 0.125).unwrap();
                image.set_deep_value(x, y, 3, s, 0.5).unwrap();
                image.set_deep_value(x, y, 4, s, depth).unwrap();
            }
        }
    }
    image
}

fn flat_pixel(image: &ImageBuf, x: i32, y: i32) -> Vec<f32> {
    let channels = image.spec().unwrap().channel_count();
    let roi = Roi::new(x..x + 1, y..y + 1, 0..1, 0..channels).unwrap();
    let mut values = vec![0.0_f32; channels as usize];
    image.get_pixels_into(roi, &mut values).unwrap();
    values
}

#[test]
fn an_image_buffer_can_hold_and_report_deep_samples() {
    let image = deep_image(4, 4, 3);

    assert!(image.is_deep());
    assert_eq!(image.deep_sample_count(0, 0), 3);
    assert_eq!(image.deep_value(0, 0, 4, 0).unwrap(), 1.0);
    assert_eq!(image.deep_value(0, 0, 4, 2).unwrap(), 3.0);
    assert_eq!(image.deep_value(2, 3, 3, 1).unwrap(), 0.5);

    // A pixel nobody wrote to holds nothing.
    let empty =
        ImageBuf::new(&ImageSpec::new(4, 4, 5, PixelFormat::F32).unwrap().as_deep()).unwrap();
    assert_eq!(empty.deep_sample_count(0, 0), 0);
}

/// OpenImageIO answers an out-of-range index with a null pointer and then
/// reads zero or drops the write, saying nothing either way.
#[test]
fn deep_indices_are_checked() {
    let mut image = deep_image(4, 4, 2);

    assert!(
        image.deep_value(0, 0, 9, 0).is_err(),
        "channel out of range"
    );
    assert!(image.deep_value(0, 0, 0, 5).is_err(), "sample out of range");
    assert!(image.set_deep_value(0, 0, 0, 5, 1.0).is_err());

    let error = image.deep_value(0, 0, 0, 5).unwrap_err();
    println!("out-of-range sample reported as: {error}");
}

#[test]
fn a_flat_image_refuses_deep_access() {
    let mut flat = ImageBuf::new(&ImageSpec::new(4, 4, 3, PixelFormat::F32).unwrap()).unwrap();
    assert!(!flat.is_deep());
    assert!(flat.set_deep_sample_count(0, 0, 1).is_err());
    assert!(flat.deep_value(0, 0, 0, 0).is_err());
}

#[test]
fn flatten_composites_samples_into_one_pixel() {
    let deep = deep_image(4, 4, 3);
    let mut flat = ImageBuf::empty().unwrap();
    algo::flatten(&mut flat, &deep, None).unwrap();

    assert!(!flat.is_deep(), "the result of flattening is a flat image");
    let pixel = flat_pixel(&flat, 0, 0);
    println!("three samples at alpha 0.5 flatten to: {pixel:?}");

    // Front to back with alpha 0.5 each: the first contributes fully, the
    // second at half, the third at a quarter. Alpha accumulates towards one.
    assert!(
        pixel[3] > 0.5 && pixel[3] < 1.0,
        "alpha should accumulate above one sample's but stay under one, got {}",
        pixel[3]
    );
    assert!(
        pixel[0] > 0.5,
        "the colour should accumulate past a single sample's, got {}",
        pixel[0]
    );
}

/// The accumulator is sized from the source but indexed up to the wider of the
/// two buffers, so a wider destination reads past the end of it.
#[test]
fn flatten_refuses_a_wider_destination() {
    let deep = deep_image(4, 4, 2);
    let mut wide = ImageBuf::new(&ImageSpec::new(4, 4, 8, PixelFormat::F32).unwrap()).unwrap();

    let error = algo::flatten(&mut wide, &deep, None).unwrap_err();
    println!("wider destination reported as: {error}");
    assert!(error.to_string().contains("channels"), "{error}");
}

#[test]
fn flatten_needs_an_alpha_channel() {
    // R, G, B, Z — no alpha for the compositing to use.
    let spec = ImageSpec::new(4, 4, 4, PixelFormat::F32)
        .unwrap()
        .with_channel_names(["R", "G", "B", "Z"])
        .unwrap()
        .as_deep();
    let mut deep = ImageBuf::new(&spec).unwrap();
    deep.set_deep_sample_count(0, 0, 1).unwrap();

    let mut flat = ImageBuf::empty().unwrap();
    let error = algo::flatten(&mut flat, &deep, None).unwrap_err();
    println!("no alpha reported as: {error}");
}

#[test]
fn deepen_turns_a_flat_image_into_one_sample_per_pixel() {
    let spec = ImageSpec::new(4, 4, 3, PixelFormat::F32)
        .unwrap()
        .with_channel_names(["R", "G", "B"])
        .unwrap();
    let mut flat = ImageBuf::new(&spec).unwrap();
    algo::fill(&mut flat, &[0.5, 0.5, 0.5], None).unwrap();

    let mut deep = ImageBuf::empty().unwrap();
    algo::deepen(&mut deep, &flat, 7.0, None).unwrap();

    assert!(deep.is_deep());
    assert_eq!(deep.deep_sample_count(0, 0), 1);
    assert_eq!(
        deep.spec().unwrap().channel_count(),
        4,
        "a Z channel is appended when the source has none"
    );
    assert_eq!(
        deep.deep_value(0, 0, 3, 0).unwrap(),
        7.0,
        "the depth should be the value asked for"
    );
    assert_eq!(deep.deep_value(0, 0, 0, 0).unwrap(), 0.5);
}

/// A pixel that is zero everywhere gets no sample, which is what makes an
/// empty background stay empty.
#[test]
fn deepen_leaves_black_pixels_empty() {
    let spec = ImageSpec::new(4, 4, 3, PixelFormat::F32)
        .unwrap()
        .with_channel_names(["R", "G", "B"])
        .unwrap();
    let mut flat = ImageBuf::new(&spec).unwrap();
    algo::zero(&mut flat, None).unwrap();
    // One non-black pixel.
    let one = Roi::new(1..2, 1..2, 0..1, 0..3).unwrap();
    algo::fill(&mut flat, &[1.0, 1.0, 1.0], Some(one)).unwrap();

    let mut deep = ImageBuf::empty().unwrap();
    algo::deepen(&mut deep, &flat, 1.0, None).unwrap();

    assert_eq!(
        deep.deep_sample_count(1, 1),
        1,
        "the lit pixel gets a sample"
    );
    assert_eq!(deep.deep_sample_count(0, 0), 0, "the black one does not");
}

/// A pre-allocated destination would keep its own shape and drop the writes
/// that did not fit, without a word.
#[test]
fn deepen_needs_an_empty_destination() {
    let flat = ImageBuf::new(&ImageSpec::new(4, 4, 3, PixelFormat::F32).unwrap()).unwrap();
    let mut already = ImageBuf::new(&ImageSpec::new(4, 4, 3, PixelFormat::F32).unwrap()).unwrap();

    let error = algo::deepen(&mut already, &flat, 1.0, None).unwrap_err();
    println!("pre-allocated destination reported as: {error}");
    assert!(error.to_string().contains("empty"), "{error}");
}

#[test]
fn flatten_and_deepen_round_trip_a_single_sample() {
    let spec = ImageSpec::new(4, 4, 4, PixelFormat::F32)
        .unwrap()
        .with_channel_names(["R", "G", "B", "A"])
        .unwrap();
    let mut flat = ImageBuf::new(&spec).unwrap();
    algo::fill(&mut flat, &[0.4, 0.5, 0.6, 1.0], None).unwrap();

    let mut deep = ImageBuf::empty().unwrap();
    algo::deepen(&mut deep, &flat, 1.0, None).unwrap();

    let mut back = ImageBuf::empty().unwrap();
    algo::flatten(&mut back, &deep, None).unwrap();

    let original = flat_pixel(&flat, 0, 0);
    let returned = flat_pixel(&back, 0, 0);
    println!("{original:?} -> deep -> {returned:?}");
    for channel in 0..4 {
        assert!(
            (original[channel] - returned[channel]).abs() < 1e-4,
            "a single opaque sample should survive the round trip: {original:?} \
             then {returned:?}"
        );
    }
}

#[test]
fn deep_merge_interleaves_two_images_samples() {
    let near = deep_image(2, 2, 1);
    let mut far = deep_image(2, 2, 1);
    // Push the second image's sample behind the first's.
    for y in 0..2 {
        for x in 0..2 {
            far.set_deep_value(x, y, 4, 0, 10.0).unwrap();
        }
    }

    let mut merged = ImageBuf::empty().unwrap();
    algo::deep_merge(&mut merged, &near, &far, false, None).unwrap();

    assert!(merged.is_deep());
    assert_eq!(
        merged.deep_sample_count(0, 0),
        2,
        "one sample from each should survive without culling"
    );

    // Sorted by depth: the nearer one first.
    let first = merged.deep_value(0, 0, 4, 0).unwrap();
    let second = merged.deep_value(0, 0, 4, 1).unwrap();
    println!("merged depths: {first} then {second}");
    assert!(first <= second, "samples should come back sorted by depth");
}

#[test]
fn deep_merge_needs_matching_channels() {
    // A superset rather than a subset, so the count is the ONLY difference.
    // This used to compare the 5-channel R,G,B,A,Z against an R,G,B,A one,
    // which is missing Z as well as being shorter, so the test could not
    // tell which of the two OpenImageIO objected to.
    let five = deep_image(2, 2, 1);
    let spec = ImageSpec::new(2, 2, 6, PixelFormat::F32)
        .unwrap()
        .with_channel_names(["R", "G", "B", "A", "Z", "extra"])
        .unwrap()
        .as_deep();
    let six = ImageBuf::new(&spec).unwrap();

    let mut merged = ImageBuf::empty().unwrap();
    let error = algo::deep_merge(&mut merged, &five, &six, true, None).unwrap_err();
    // Asserting the cause, not just that something failed. The body used to
    // be a println, so it passed for any error at all.
    assert!(
        error
            .to_string()
            .contains("images must have the same number of channels"),
        "unexpected message: {error}"
    );
}

#[test]
fn deep_holdout_removes_what_is_hidden() {
    // A source with a sample far away.
    let mut source = deep_image(2, 2, 1);
    for y in 0..2 {
        for x in 0..2 {
            source.set_deep_value(x, y, 4, 0, 100.0).unwrap();
        }
    }

    // A holdout that is opaque close to the camera.
    let mut holdout = deep_image(2, 2, 1);
    for y in 0..2 {
        for x in 0..2 {
            holdout.set_deep_value(x, y, 3, 0, 1.0).unwrap(); // fully opaque
            holdout.set_deep_value(x, y, 4, 0, 1.0).unwrap(); // right in front
        }
    }

    let mut held = ImageBuf::empty().unwrap();
    algo::deep_holdout(&mut held, &source, &holdout, None).unwrap();

    assert_eq!(
        held.deep_sample_count(0, 0),
        0,
        "a sample behind an opaque holdout should be culled"
    );

    // With the holdout further away than the source, nothing is hidden.
    let mut behind = deep_image(2, 2, 1);
    for y in 0..2 {
        for x in 0..2 {
            behind.set_deep_value(x, y, 3, 0, 1.0).unwrap();
            behind.set_deep_value(x, y, 4, 0, 1000.0).unwrap();
        }
    }
    let mut kept = ImageBuf::empty().unwrap();
    algo::deep_holdout(&mut kept, &source, &behind, None).unwrap();
    assert_eq!(
        kept.deep_sample_count(0, 0),
        1,
        "a sample in front of the holdout should survive"
    );
}

#[test]
fn the_deep_operations_refuse_flat_images() {
    let flat = ImageBuf::new(&ImageSpec::new(4, 4, 4, PixelFormat::F32).unwrap()).unwrap();
    let deep = deep_image(4, 4, 1);

    // require_deep's own wording, not just is_err. OpenImageIO refuses the
    // same shape itself, so a bare is_err passed with the guard removed and
    // pinned nothing.
    let mut result = ImageBuf::empty().unwrap();
    let error = algo::deep_merge(&mut result, &flat, &deep, true, None)
        .expect_err("deep_merge needs a deep first image");
    assert!(
        error.to_string().contains("is not a deep image"),
        "unexpected message: {error}"
    );

    let mut result = ImageBuf::empty().unwrap();
    let error = algo::deep_holdout(&mut result, &flat, &deep, None)
        .expect_err("deep_holdout needs a deep source");
    assert!(
        error.to_string().contains("is not a deep image"),
        "unexpected message: {error}"
    );
}

/// Pasting a deep source into an empty destination works, and must do so
/// without tripping OpenImageIO's own assertion on the way.
///
/// `deep_paste_` opens with `OIIO_ASSERT(dst.deep() && src.deep())` and then,
/// twelve lines down, handles an uninitialized destination itself with
/// `dst.reset(src.spec())`. So the assertion rejects a shape the function is
/// written to support. A build with assertions compiled out prints and
/// carries on; conda-forge ships one with them live.
///
/// This was invisible because it is not an error: the call returns Ok and the
/// result is correct. It showed up as 25 assertion lines in a CI run that
/// passed. The shim now does OpenImageIO's own reset first.
#[test]
fn pasting_a_deep_source_into_an_empty_destination_works() {
    let src = deep_image(4, 4, 2);
    let mut dst = ImageBuf::empty().unwrap();

    algo::paste(&mut dst, [0, 0, 0], 0, &src, None)
        .expect("a deep source pastes into an empty destination");

    assert!(dst.is_deep(), "the destination should have become deep");
    assert_eq!(dst.deep_sample_count(0, 0), 2);
    assert_eq!(dst.deep_sample_count(3, 3), 2);
}

/// A deep image whose samples have extent: each spans `Z` to `ZBack` rather
/// than sitting at a point.
///
/// Nothing in either public corpus is like this. Every deep file in the
/// OpenEXR and OpenImageIO corpora is `R G B A Z`, and the two production
/// renders I have carry a `ZBack` channel in which every sample has
/// `ZBack == Z`. `DeepData::Zback_channel()` returns `Z_channel()` when the
/// channel is absent, so the whole suite has been exercising that fallback
/// and the comparisons `deep_merge` makes have been between a value and
/// itself. This builds the other case.
///
/// `front` and `back` are the extent of the single sample in every pixel.
fn volumetric_image(width: u32, height: u32, front: f32, back: f32) -> ImageBuf {
    let spec = ImageSpec::new(width, height, 6, PixelFormat::F32)
        .unwrap()
        .with_channel_names(["R", "G", "B", "A", "Z", "ZBack"])
        .unwrap()
        .as_deep();
    let mut image = ImageBuf::new(&spec).unwrap();
    for y in 0..height as i32 {
        for x in 0..width as i32 {
            image.set_deep_sample_count(x, y, 1).unwrap();
            image.set_deep_value(x, y, 0, 0, 0.5).unwrap();
            image.set_deep_value(x, y, 1, 0, 0.25).unwrap();
            image.set_deep_value(x, y, 2, 0, 0.125).unwrap();
            image.set_deep_value(x, y, 3, 0, 0.5).unwrap();
            image.set_deep_value(x, y, 4, 0, front).unwrap();
            image.set_deep_value(x, y, 5, 0, back).unwrap();
        }
    }
    image
}

fn samples_in(image: &ImageBuf) -> u32 {
    let window = image.spec().unwrap().data_window().unwrap();
    window
        .y()
        .flat_map(|y| window.x().map(move |x| (x, y)))
        .map(|(x, y)| image.deep_sample_count(x, y))
        .sum()
}

/// A deep paste at an offset has to land where a flat one lands.
///
/// It did not. `ImageBufAlgo::paste` sends a deep source to `deep_paste_`,
/// which is the one path in the function that never calls `IBAprep`: instead
/// of sizing an uninitialized destination to the destination region, it does
/// `dst.reset(src.spec())` and takes the source's window. It then writes at
/// `x + offset` through `ImageBuf::set_deep_samples` and
/// `ImageBuf::copy_deep_pixel`, and both of those call `pixelindex()` with
/// `check_range` defaulting to `false`, so a coordinate past the destination
/// is not clipped. It becomes a linear index that wraps into the next
/// scanline.
///
/// An 8x8 source with two samples a pixel, pasted at x=10 into an empty
/// destination, came back on an 0..8 window holding 108 of its 128 samples,
/// shifted one row and two columns, reported as success. The flat path has
/// none of this, because `paste_` iterates and tests `d.exists()`.
///
/// This is written as a comparison rather than a pair of expectations so
/// that it keeps holding the two paths to each other.
#[test]
fn a_deep_paste_at_an_offset_lands_where_a_flat_paste_does() {
    let offset = [10, 3, 0];

    let flat_src = ImageBuf::new(&ImageSpec::new(8, 8, 5, PixelFormat::F32).unwrap()).unwrap();
    let mut flat_dst = ImageBuf::empty().unwrap();
    algo::paste(&mut flat_dst, offset, 0, &flat_src, None).expect("a flat paste at an offset");
    let flat_window = flat_dst.spec().unwrap().data_window().unwrap();

    let deep_src = deep_image(8, 8, 2);
    let mut deep_dst = ImageBuf::empty().unwrap();
    algo::paste(&mut deep_dst, offset, 0, &deep_src, None).expect("a deep paste at an offset");
    let deep_window = deep_dst.spec().unwrap().data_window().unwrap();

    assert_eq!(
        (deep_window.x(), deep_window.y()),
        (flat_window.x(), flat_window.y()),
        "the deep and flat paths must put the source in the same place"
    );
    assert_eq!(deep_window.x(), 10..18);
}

/// The samples themselves, after the same paste.
#[test]
fn a_deep_paste_at_an_offset_keeps_every_sample() {
    let src = deep_image(8, 8, 2);
    let offset = [10, 3, 0];
    let before = samples_in(&src);
    assert_eq!(before, 8 * 8 * 2);

    let mut dst = ImageBuf::empty().unwrap();
    algo::paste(&mut dst, offset, 0, &src, None).expect("a deep paste at an offset");

    assert_eq!(samples_in(&dst), before, "an offset paste lost samples");
    for y in 0..8 {
        for x in 0..8 {
            assert_eq!(
                dst.deep_sample_count(x + offset[0], y + offset[1]),
                src.deep_sample_count(x, y),
                "the sample at {x},{y} should be at {},{}",
                x + offset[0],
                y + offset[1]
            );
            // And the depth, which a shift would carry along with it and so
            // would not catch on its own.
            assert_eq!(
                dst.deep_value(x + offset[0], y + offset[1], 4, 1).unwrap(),
                src.deep_value(x, y, 4, 1).unwrap()
            );
        }
    }
}

/// A paste that hangs over the edge of an allocated destination is clipped,
/// not wrapped.
///
/// This is the same defect seen from the other side. The destination here is
/// already allocated, so there is no reset to get wrong, but the writes still
/// go through an unchecked `pixelindex`: a negative offset indexes backwards
/// out of the row. 128 samples became 122, scattered.
///
/// What should happen is what `paste` documents and what the flat path does:
/// the part of the source that falls outside the destination is dropped and
/// the rest lands correctly.
#[test]
fn a_deep_paste_that_overhangs_its_destination_is_clipped_not_wrapped() {
    let src = deep_image(8, 8, 2);
    let spec = ImageSpec::new(8, 8, 5, PixelFormat::F32)
        .unwrap()
        .with_channel_names(["R", "G", "B", "A", "Z"])
        .unwrap()
        .as_deep();
    let mut dst = ImageBuf::new(&spec).unwrap();

    // Three columns off the left edge: source columns 3..8 land at 0..5.
    algo::paste(&mut dst, [-3, 0, 0], 0, &src, None).expect("an overhanging deep paste");

    assert_eq!(
        samples_in(&dst),
        5 * 8 * 2,
        "five of the eight columns should have landed"
    );
    for y in 0..8 {
        for x in 0..5 {
            assert_eq!(
                dst.deep_sample_count(x, y),
                2,
                "{x},{y} should hold a sample"
            );
        }
        for x in 5..8 {
            assert_eq!(
                dst.deep_sample_count(x, y),
                0,
                "{x},{y} is past the pasted region and must be empty"
            );
        }
    }
}

/// And a paste that misses the destination altogether is refused rather than
/// scrambled.
///
/// With the writes unchecked there is no offset large enough to miss: an
/// 8-wide destination and an offset of 10 wrapped into the next row and
/// reported success. Since nothing can be pasted here, the honest answer is
/// an error.
#[test]
fn a_deep_paste_that_misses_its_destination_is_refused() {
    let src = deep_image(8, 8, 2);
    let spec = ImageSpec::new(8, 8, 5, PixelFormat::F32)
        .unwrap()
        .with_channel_names(["R", "G", "B", "A", "Z"])
        .unwrap()
        .as_deep();
    let mut dst = ImageBuf::new(&spec).unwrap();

    let error = algo::paste(&mut dst, [10, 0, 0], 0, &src, None)
        .expect_err("a deep paste with nothing landing must be refused");
    assert!(
        error.to_string().contains("no sample would be pasted"),
        "unexpected message: {error}"
    );
    assert_eq!(
        samples_in(&dst),
        0,
        "a refused paste must not have written anything"
    );
}

/// `first_channel` is ignored on the deep path, so it is refused.
///
/// `deep_paste_` takes a destination region whose channel range carries
/// `chbegin`, and then never looks at it: the copy is `copy_deep_pixel`,
/// which is whole-pixel. A flat paste with a first channel of 2 widens the
/// destination to seven channels and puts the source at channel 2. The deep
/// one returns five channels with the source at channel 0 and reports
/// success, which is a different image from the one that was asked for.
#[test]
fn a_deep_paste_refuses_a_first_channel_openimageio_would_drop() {
    let src = deep_image(4, 4, 2);

    // The flat path honours it, which is what makes the deep answer wrong
    // rather than merely unsupported.
    let flat_src = ImageBuf::new(&ImageSpec::new(4, 4, 5, PixelFormat::F32).unwrap()).unwrap();
    let mut flat_dst = ImageBuf::empty().unwrap();
    algo::paste(&mut flat_dst, [0, 0, 0], 2, &flat_src, None).expect("a flat paste at channel 2");
    assert_eq!(
        flat_dst.channel_count(),
        7,
        "the flat path widens the destination to hold the source at channel 2"
    );

    let mut dst = ImageBuf::empty().unwrap();
    let error = algo::paste(&mut dst, [0, 0, 0], 2, &src, None)
        .expect_err("a deep paste cannot honour a first channel");
    assert!(
        error.to_string().contains("ignores the first channel"),
        "unexpected message: {error}"
    );
}

/// `deep_merge` on samples that have extent, which is the arithmetic the
/// corpora cannot reach.
///
/// Two samples that overlap in depth are not interleaved, they are split.
/// `DeepData::merge_deep_pixels` concatenates, sorts, splits every sample at
/// every other's `Z` and `ZBack`, sorts again and folds what then coincides.
/// One sample spanning 1 to 3 merged with one spanning 2 to 4 comes out as
/// three: 1 to 2, 2 to 3, and 3 to 4.
///
/// Every other deep test in this crate has `ZBack` absent or equal to `Z`,
/// where `Zback_channel()` falls back to `Z_channel()` and none of those
/// comparisons can be true. So this is the only test that reaches the split.
#[test]
fn deep_merge_splits_samples_that_overlap_in_depth() {
    let near = volumetric_image(2, 2, 1.0, 3.0);
    let far = volumetric_image(2, 2, 2.0, 4.0);

    let spec = near.spec().unwrap();
    assert_eq!(spec.channel_names()[5], "ZBack");
    assert_eq!(near.deep_value(0, 0, 4, 0).unwrap(), 1.0);
    assert_eq!(near.deep_value(0, 0, 5, 0).unwrap(), 3.0);

    let mut merged = ImageBuf::empty().unwrap();
    algo::deep_merge(&mut merged, &near, &far, false, None).expect("merging two volumetric images");

    assert!(merged.is_deep());
    assert_eq!(
        merged.deep_sample_count(0, 0),
        3,
        "two overlapping spans split into three, so this is the test that would \
         still pass at 2 if ZBack were being ignored"
    );

    // The three spans, in order, with no gap and no overlap left.
    let mut spans = Vec::new();
    for sample in 0..merged.deep_sample_count(0, 0) {
        spans.push((
            merged.deep_value(0, 0, 4, sample).unwrap(),
            merged.deep_value(0, 0, 5, sample).unwrap(),
        ));
    }
    assert_eq!(spans, vec![(1.0, 2.0), (2.0, 3.0), (3.0, 4.0)], "{spans:?}");

    // And two samples at the same depth with no extent do not split, which
    // is the control: it is the same call on the shape the corpora have.
    let a = volumetric_image(2, 2, 1.0, 1.0);
    let b = volumetric_image(2, 2, 2.0, 2.0);
    let mut merged = ImageBuf::empty().unwrap();
    algo::deep_merge(&mut merged, &a, &b, false, None).expect("merging two point-sample images");
    assert_eq!(
        merged.deep_sample_count(0, 0),
        2,
        "points at different depths are interleaved, not split"
    );
}
