//! The guards, against files this crate did not write.
//!
//! `guard_coverage_test` pins every refusal with an 8x8 buffer of F32 zeroes
//! whose data window starts at the origin and whose deep variant holds no
//! samples at all. Real files differ from that in exactly the three ways the
//! guards are made of:
//!
//! - their data windows are not the display window and do not start at 0,0,
//!   and on an overscan render they start at a negative coordinate;
//! - their channels do not share one format, so a deep EXR is four halves
//!   and a float;
//! - their deep pixels hold samples, hundreds of thousands of them.
//!
//! A guard that got the window arithmetic wrong, that keyed off a format, or
//! that answered on a sample count rather than on `deep()` would pass every
//! test in that file. So these run the same guards against the corpora, and
//! they assert in both directions. One direction is the one already covered:
//! the refusals still happen, on real windows. The other is new, and it is
//! the one a too-strict guard fails: a real four-element composite, with the
//! data windows a renderer actually produced, has to go through.
//!
//! Opt-in the same way as `corpus_test`:
//!
//! ```text
//! OIIO_BIND_TEST_IMAGES=.../OpenImageIO-images
//! OIIO_BIND_TEST_EXR_IMAGES=.../openexr-images/exr-images
//! OIIO_BIND_TEST_DEEP_IMAGES=<a directory of volumetric deep EXRs>
//! ```
//!
//! The third may name several directories, in the platform's `PATH` form,
//! because the deep fixtures worth having do not live in one place and none
//! of them is in the two corpora above. Those two hold deep files that are
//! all `R G B A Z`, every channel half except a float `Z`, every sample a
//! point. What is missing from them, and what the third variable is for:
//!
//! - `ZBack`. Absent everywhere in both corpora, and
//!   `DeepData::Zback_channel()` silently returns `Z_channel()` when it is
//!   absent, so every deep comparison the suite has ever made was between a
//!   value and itself. A production render carries the channel.
//! - `uint32` channels. OpenEXR's own deep test set, which lives under
//!   `src/test/bin/test_images` in the library repository rather than in
//!   `openexr-images`, has `id` and `particleid` as `uint32` beside half
//!   colour channels, and a half `Z` rather than a float one.
//!
//! Volumetric samples, where `ZBack` is genuinely beyond `Z`, are in neither
//! place: the production renders I have carry the channel with `ZBack == Z`
//! throughout. That arithmetic is pinned in `algo_deep_test` instead, on an
//! image written for it.

mod common;

use std::path::PathBuf;

use oiio::{algo, ImageBuf, ImageSpec, PixelFormat, Roi};

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

/// Report that a test is skipping, or fail if the run says it must not.
///
/// A missing variable and a variable pointing somewhere wrong are the same
/// thing to every helper here: both skip, both print a line, both pass. That
/// is right for a developer who has not downloaded 700MB of images, and wrong
/// for a run that is supposed to be exercising them, where a renamed fixture
/// or a typo in a path reads as a clean suite.
///
/// So `OIIO_BIND_REQUIRE_CORPUS` turns every skip in this file into a
/// failure. CI sets it alongside the paths, which is what keeps a broken
/// fetch from looking like a green run.
fn skip(reason: &str) {
    if required() {
        panic!("{reason} (OIIO_BIND_REQUIRE_CORPUS is set, so this is a failure)");
    }
    eprintln!("skipping: {reason}");
}

fn required() -> bool {
    std::env::var_os("OIIO_BIND_REQUIRE_CORPUS").is_some_and(|v| !v.is_empty())
}

/// Report a skip that `OIIO_BIND_REQUIRE_CORPUS` must not turn into a
/// failure, because no public corpus carries the fixture.
///
/// There is exactly one: a deep render with a real `ZBack` channel. Neither
/// `openexr-images` nor OpenEXR's own test set has one, so the test runs
/// where someone has a production render and nowhere else. Saying so here is
/// the difference between a fixture that is missing and a fixture that does
/// not exist to fetch.
fn skip_not_public(reason: &str) {
    eprintln!("skipping: {reason} (no public corpus carries this fixture)");
}

fn corpus_root(variable: &str) -> Option<PathBuf> {
    let path = PathBuf::from(std::env::var_os(variable)?);
    path.is_dir().then_some(path)
}

/// Every directory named by `variable`, which may hold a list in the
/// platform's `PATH` form.
///
/// The deep fixtures do not come from one place. OpenEXR keeps its own small
/// deep set under `src/test/bin/test_images` in the library repository rather
/// than in `openexr-images`, and a production render comes from wherever the
/// render came from, so one directory per variable would mean a variable per
/// download.
fn deep_roots() -> Vec<PathBuf> {
    match std::env::var_os(DEEP_IMAGES) {
        Some(value) => std::env::split_paths(&value)
            .filter(|path| path.is_dir())
            .collect(),
        None => Vec::new(),
    }
}

/// The first of `candidates` found in any directory named by
/// `OIIO_BIND_TEST_DEEP_IMAGES`, read and ready.
fn deep_fixture(candidates: &[&str], public: bool) -> Option<(String, ImageBuf)> {
    let report = if public { skip } else { skip_not_public };
    let roots = deep_roots();
    if roots.is_empty() {
        report(&format!(
            "set {DEEP_IMAGES} to one or more directories of deep EXRs"
        ));
        return None;
    }
    for name in candidates {
        for root in &roots {
            let path = root.join(name);
            if path.is_file() {
                let mut image = ImageBuf::from_path(&path).expect("could not open the fixture");
                image
                    .read()
                    .unwrap_or_else(|error| panic!("could not read {}: {error}", path.display()));
                return Some(((*name).to_string(), image));
            }
        }
    }
    report(&format!("none of {candidates:?} are under {DEEP_IMAGES}"));
    None
}

/// Load a corpus file, or report the skip and return `None`.
///
/// Every test here names the file it needs rather than sweeping, so a
/// corpus that is present but missing one file skips that test alone.
fn load(variable: &str, relative: &str) -> Option<ImageBuf> {
    let Some(root) = corpus_root(variable) else {
        skip(&format!("set {variable}"));
        return None;
    };
    let path = root.join(relative);
    if !path.is_file() {
        skip(&format!("{} is not in the corpus", path.display()));
        return None;
    }
    let mut image = ImageBuf::from_path(&path).expect("could not open the corpus file");
    image
        .read()
        .unwrap_or_else(|error| panic!("could not read {}: {error}", path.display()));
    Some(image)
}

/// The first of `candidates` that is present, with its name.
fn load_any(variable: &str, candidates: &[&str]) -> Option<(String, ImageBuf)> {
    let Some(root) = corpus_root(variable) else {
        skip(&format!("set {variable}"));
        return None;
    };
    for relative in candidates {
        if root.join(relative).is_file() {
            let image = load(variable, relative)?;
            return Some(((*relative).to_string(), image));
        }
    }
    skip(&format!("none of {candidates:?} are under {variable}"));
    None
}

const OIIO_IMAGES: &str = "OIIO_BIND_TEST_IMAGES";
const EXR_IMAGES: &str = "OIIO_BIND_TEST_EXR_IMAGES";
const DEEP_IMAGES: &str = "OIIO_BIND_TEST_DEEP_IMAGES";

/// One of the four real deep render elements, low resolution so that a test
/// can walk every pixel without the walk dominating the run.
///
/// `Balls` is the interesting one: 764x406 of data at 131,170 inside a
/// 1024x576 display window, which is the shape a caller hits when they
/// compute a region from the format and hand it to an image that is a crop.
fn deep_element(name: &str) -> Option<ImageBuf> {
    load(EXR_IMAGES, &format!("v2/LowResLeftView/{name}.exr"))
}

/// A deep render carrying a `ZBack` channel, which neither public corpus has.
fn deep_with_zback() -> Option<(String, ImageBuf)> {
    deep_fixture(&["deepHelicopter.exr", "deepGround.exr"], false)
}

/// A deep image with `uint32` identifier channels beside its half colour
/// channels, from OpenEXR's own test set.
fn deep_with_identifiers() -> Option<(String, ImageBuf)> {
    deep_fixture(
        &[
            "objectid.deep.exr",
            "multivariate.deep.exr",
            "11.deep.exr",
            "42.deep.exr",
            "64.deep.exr",
        ],
        true,
    )
}

/// The corner a region starts at, as `ImageSpec::with_origin` wants it.
fn origin_of(region: &Roi) -> [i32; 3] {
    [region.x().start, region.y().start, region.z().start]
}

fn data_window(image: &ImageBuf) -> Roi {
    image
        .spec()
        .expect("an initialized image has a spec")
        .data_window()
        .expect("a read image has a data window")
}

/// The region a caller gets by asking for the display window, which is the
/// format the image claims to be and not necessarily where its pixels are.
fn display_window(image: &ImageBuf) -> Roi {
    let spec = image.spec().expect("an initialized image has a spec");
    let [x, y, z] = spec.full_origin();
    let [width, height, depth] = spec.full_dimensions();
    Roi::new(
        x..x + width as i32,
        y..y + height as i32,
        z..z + depth.max(1) as i32,
        0..spec.channel_count(),
    )
    .expect("a display window is a valid region")
}

/// Walk every pixel of a deep image and total its samples.
fn total_samples(image: &ImageBuf) -> u64 {
    let window = data_window(image);
    let mut total = 0;
    for y in window.y() {
        for x in window.x() {
            total += u64::from(image.deep_sample_count(x, y));
        }
    }
    total
}

// ---------------------------------------------------------------------------
// Window arithmetic, on windows a renderer produced
// ---------------------------------------------------------------------------

/// `refuse_empty_region`, on the shape that makes it worth having.
///
/// `DisplayWindow/t11.exr` holds 400x300 of pixels at 0,0 and declares a
/// display window of 400x200 at 0,300: the format it claims and the pixels it
/// has do not overlap at all. A caller who sizes a region from the display
/// window, which is the ordinary thing to do, names no source pixels.
///
/// Every synthetic fixture has its data window equal to its display window at
/// the origin, so this distinction cannot be made with one. The guard has
/// never been shown to compute the intersection against the data window
/// rather than the declared format.
#[test]
fn a_region_taken_from_the_display_window_can_miss_every_pixel() {
    let Some(src) = load(EXR_IMAGES, "DisplayWindow/t11.exr") else {
        return;
    };

    let data = data_window(&src);
    let display = display_window(&src);
    assert!(
        data.intersection(&display).is_none(),
        "t11 is the fixture because its windows are disjoint: data {:?}, display {:?}",
        data.y(),
        display.y()
    );

    let mut dst = ImageBuf::empty().unwrap();
    let error = algo::copy(&mut dst, &src, None, Some(display))
        .expect_err("a region with no source pixels in it must be refused");
    assert!(
        error.to_string().contains("does not overlap the source"),
        "unexpected message: {error}"
    );
}

/// The other side of the same arithmetic, and the one a too-strict guard
/// fails.
///
/// `t14.exr` holds 400x300 at 0,0 and declares 101x101 at -100,-100, so the
/// display window reaches exactly one pixel of data, at 0,0. The intersection
/// is 1x1: non-empty, and so this must go through. An off-by-one in either
/// bound, or a comparison that treated a one-pixel overlap as nothing, turns
/// a legal call into a refusal, and no synthetic test can tell the difference
/// because none of them has an overlap narrower than the whole image.
#[test]
fn a_display_window_that_reaches_one_pixel_of_data_is_allowed() {
    let Some(src) = load(EXR_IMAGES, "DisplayWindow/t14.exr") else {
        return;
    };

    let overlap = data_window(&src)
        .intersection(&display_window(&src))
        .expect("t14's windows overlap");
    assert_eq!(
        (overlap.width(), overlap.height()),
        (1, 1),
        "t14 is the fixture because the overlap is a single pixel"
    );

    let mut dst = ImageBuf::empty().unwrap();
    algo::copy(&mut dst, &src, None, Some(display_window(&src)))
        .expect("a region that reaches one real pixel must not be refused");

    // The destination covers the region that was asked for, not the overlap:
    // OpenImageIO sizes it from the request and leaves the rest black. So the
    // assertion that distinguishes real work from an empty success is that
    // the one overlapping pixel carries the source's value and a pixel
    // outside the source's data window does not.
    let out = data_window(&dst);
    assert_eq!(
        (out.width(), out.height()),
        (display_window(&src).width(), display_window(&src).height())
    );

    let mut source_pixel = vec![0.0_f32; src.channel_count() as usize];
    src.pixel_at_into(0, 0, oiio::Wrap::Black, &mut source_pixel)
        .unwrap();
    let mut copied = vec![0.0_f32; dst.channel_count() as usize];
    dst.pixel_at_into(0, 0, oiio::Wrap::Black, &mut copied)
        .unwrap();
    assert_eq!(
        copied[..source_pixel.len()],
        source_pixel[..],
        "the overlapping pixel should have been copied"
    );
    assert!(
        source_pixel.iter().any(|value| *value != 0.0),
        "t14's pixel 0,0 has to be non-black for the comparison above to mean anything: \
         {source_pixel:?}"
    );

    let mut outside = vec![0.0_f32; dst.channel_count() as usize];
    dst.pixel_at_into(-50, -50, oiio::Wrap::Black, &mut outside)
        .unwrap();
    assert!(
        outside.iter().all(|value| *value == 0.0),
        "a pixel the source does not reach should be black: {outside:?}"
    );
}

/// A region in negative coordinates, which is where an overscan render keeps
/// most of its pixels.
///
/// `grid-overscan.exr` is 1500x1500 of data at -250,-250 behind a 1000x1000
/// display window. Half its pixels have a negative x or y. The guard
/// intersects regions and compares bounds, and every fixture it has ever been
/// given had both windows at or above zero, so nothing has shown that it
/// handles the sign. A region wholly in the negative quadrant is a legal
/// request here and has to be answered.
#[test]
fn a_region_in_the_negative_quadrant_is_allowed_on_an_overscan_render() {
    let Some(src) = load(OIIO_IMAGES, "grid-overscan.exr") else {
        return;
    };

    let data = data_window(&src);
    assert!(
        data.x().start < 0 && data.y().start < 0,
        "grid-overscan is the fixture because its data window starts negative: {:?}",
        (data.x(), data.y())
    );

    // Wholly inside the data window and wholly outside the display window.
    let negative = Roi::new(-200..-100, -200..-100, 0..1, 0..3).unwrap();
    let mut dst = ImageBuf::empty().unwrap();
    algo::copy(&mut dst, &src, None, Some(negative))
        .expect("a region of real pixels at negative coordinates must not be refused");

    let out = data_window(&dst);
    assert_eq!(out.x(), negative.x());
    assert_eq!(out.y(), negative.y());
}

/// And the refusal still arrives on the far side of an overscan window, which
/// is the half the synthetic test already covers but at coordinates that are
/// only reachable because the data window is 1500 wide behind a 1000 wide
/// format. A guard that clipped to the display window would refuse the test
/// above and accept this one, getting both backwards.
#[test]
fn a_region_past_an_overscan_data_window_is_still_refused() {
    let Some(src) = load(OIIO_IMAGES, "grid-overscan.exr") else {
        return;
    };

    let data = data_window(&src);
    let past = Roi::new(
        data.x().end + 10..data.x().end + 20,
        data.y().end + 10..data.y().end + 20,
        0..1,
        0..3,
    )
    .unwrap();

    let mut dst = ImageBuf::empty().unwrap();
    let error = algo::copy(&mut dst, &src, None, Some(past))
        .expect_err("a region past the data window must be refused");
    assert!(
        error.to_string().contains("does not overlap the source"),
        "unexpected message: {error}"
    );

    // The display window ends at 1000 and the data window at 1250, so the
    // test above and this one straddle a boundary only a real overscan file
    // has.
    let display = display_window(&src);
    assert!(
        data.x().end > display.x().end,
        "the data window should extend past the format: {:?} vs {:?}",
        data.x(),
        display.x()
    );
}

/// `copy`'s own refusal, on two windows out of one real file.
///
/// This is the mistake the file is shaped to provoke. `t11.exr` holds its
/// pixels at 0,0 to 400,300 and declares a display window of 0,300 to
/// 400,500, so a caller who allocates the destination on the format and then
/// copies the image into it has allocated somewhere the image does not reach.
/// OpenImageIO copies nothing and reports success, leaving the destination
/// holding whatever it was allocated with.
///
/// Both windows come from the same file, so neither number is invented.
#[test]
fn copying_between_two_real_windows_that_do_not_meet_is_refused() {
    let Some(src) = load(EXR_IMAGES, "DisplayWindow/t11.exr") else {
        return;
    };

    // The destination allocated on the file's own declared format.
    let format = display_window(&src);
    let spec = ImageSpec::new(
        format.width() as u32,
        format.height() as u32,
        src.channel_count(),
        PixelFormat::F32,
    )
    .unwrap()
    .with_origin(origin_of(&format));
    let mut dst = ImageBuf::new(&spec).unwrap();

    assert!(
        data_window(&src).intersection(&data_window(&dst)).is_none(),
        "the two windows have to miss each other for this to be the guard that answers: \
         data {:?}, format {:?}",
        data_window(&src).y(),
        format.y()
    );

    let error = algo::copy(&mut dst, &src, None, None)
        .expect_err("a destination the source cannot reach must be refused");
    assert!(
        error.to_string().contains("does not reach the allocated"),
        "unexpected message: {error}"
    );
}

// ---------------------------------------------------------------------------
// The direction nothing tested: real work has to go through
// ---------------------------------------------------------------------------

/// `refuse_distant_pair` against the data windows of a real composite.
///
/// The guard is a heuristic with a threshold I chose: it refuses a pair whose
/// union spans more than sixteen times the pixels they hold plus a thousand.
/// Every test of it so far has put an image a million pixels away, which says
/// the threshold is not infinite and nothing else. What it does not say is
/// that the threshold clears real work, and a guard that refuses a legal
/// composite is worse than no guard, because it cannot be worked around.
///
/// The fixture is the four elements of one shot as a renderer wrote them:
/// 1024x576 of format holding data windows at 0,0, 0,141, 0,180 and 131,170.
/// Compositing them pairwise is what the operation is for. Every pair has to
/// clear the threshold, and the margin is printed so that a later change to
/// the constant can be judged against real numbers rather than invented ones.
#[test]
fn the_distance_guard_clears_a_real_four_element_composite() {
    let Some(root) = corpus_root(EXR_IMAGES) else {
        skip(&format!("set {EXR_IMAGES}"));
        return;
    };
    if !root.join("v2/LowResLeftView/Balls.exr").is_file() {
        skip("the v2 render elements are not in the corpus");
        return;
    }

    // Flat, because the arithmetic operations are flat; flattened from the
    // deep originals, so the data windows are the ones the renderer wrote
    // rather than ones this test made up.
    let names = ["Balls", "Ground", "Leaves", "Trunks"];
    let mut elements = Vec::new();
    for name in names {
        let Some(deep) = deep_element(name) else {
            return;
        };
        let mut flat = ImageBuf::empty().unwrap();
        algo::flatten(&mut flat, &deep, None)
            .unwrap_or_else(|error| panic!("could not flatten {name}: {error}"));
        elements.push((name, flat));
    }

    for (i, (left_name, left)) in elements.iter().enumerate() {
        for (right_name, right) in &elements[i + 1..] {
            let a = data_window(left);
            let b = data_window(right);
            let held = (a.width() * a.height() + b.width() * b.height()) as u64;
            let union = a.union(&b).expect("two real windows have a union");
            let spanned = (union.width() * union.height()) as u64;

            let mut dst = ImageBuf::empty().unwrap();
            algo::over(&mut dst, left, right, None).unwrap_or_else(|error| {
                panic!(
                    "the distance guard refused a real composite of {left_name} over \
                     {right_name}: {error}"
                )
            });

            println!(
                "{left_name} over {right_name}: {spanned} spanned for {held} held, \
                 {:.2}x the pixels (the guard refuses above 16x)",
                spanned as f64 / held as f64
            );
            assert!(
                spanned * 16 < held * 16 * 16,
                "{left_name}/{right_name} should be nowhere near the threshold"
            );
        }
    }
}

/// The complement of `guard_coverage_test` as a whole.
///
/// That file proves each guard refuses. Nothing proves the guards let real
/// work through, and a refusal is the cheapest bug to introduce: tighten a
/// comparison by one and every call starts failing, with a test suite full of
/// `expect_err` that goes right on passing.
///
/// So this runs each guarded operation once, with arguments that are correct,
/// against a real photograph and a real render, and demands success.
#[test]
fn every_guarded_operation_accepts_a_real_image() {
    let Some((name, src)) = load_any(
        EXR_IMAGES,
        &[
            "ScanLines/Desk.exr",
            "ScanLines/Cannon.exr",
            "TestImages/SquaresSwirls.exr",
        ],
    ) else {
        return;
    };

    // Smaller, so the filters below do not dominate the run.
    let mut small = ImageBuf::empty().unwrap();
    let window = data_window(&src);
    let crop = Roi::new(
        window.x().start..window.x().start + 64,
        window.y().start..window.y().start + 64,
        0..1,
        0..src.channel_count(),
    )
    .unwrap();
    algo::copy(&mut small, &src, None, Some(crop)).expect("cropping a real image must work");

    println!(
        "{name}: {} channels, {:?}",
        small.channel_count(),
        data_window(&small).x()
    );

    let channels = small.channel_count();
    let identity3 = [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0_f32];
    let mut identity4 = [0.0_f32; 16];
    for i in 0..4 {
        identity4[i * 4 + i] = 1.0;
    }

    // refuse_empty_region, on a region that does overlap.
    let mut dst = ImageBuf::empty().unwrap();
    algo::copy(&mut dst, &small, None, Some(data_window(&small))).expect("copy");

    // refuse_deep / reject_deep, on a flat image.
    let mut dst = ImageBuf::empty().unwrap();
    algo::transpose(&mut dst, &small, None).expect("transpose");
    algo::constant_color(&small, 0.0, None).expect("constant_color");
    algo::color_range_check(
        &small,
        &vec![0.0; channels as usize],
        &vec![1.0; channels as usize],
        None,
    )
    .expect("color_range_check");

    // refuse_deep_mismatch and refuse_distant_pair, on two flat images that
    // share a window.
    let mut dst = ImageBuf::empty().unwrap();
    algo::add(&mut dst, &small, &small, None).expect("add");

    // refuse_wide_destination, on a destination whose channel count matches.
    let mut dst = ImageBuf::new(
        &ImageSpec::new(32, 32, channels, PixelFormat::F32)
            .unwrap()
            .with_origin(origin_of(&data_window(&small))),
    )
    .unwrap();
    algo::resize(&mut dst, &small, None, None, None).expect("resize");

    // refuse_narrow_destination, likewise.
    let mut dst = ImageBuf::empty().unwrap();
    algo::warp(
        &mut dst,
        &small,
        &identity3,
        &algo::WarpOptions::default(),
        None,
    )
    .expect("warp");

    // refuse_bad_chan_reduce, into an empty destination.
    let mut dst = ImageBuf::empty().unwrap();
    algo::maxchan(&mut dst, &small, None).expect("maxchan");

    // check_window, with a real window rather than None for the height.
    let mut dst = ImageBuf::empty().unwrap();
    algo::median_filter(&mut dst, &small, 3, Some(3), None).expect("median_filter");

    // color_map's knot floor, with two knots.
    let mut dst = ImageBuf::empty().unwrap();
    algo::color_map(
        &mut dst,
        &small,
        Some(0),
        2,
        3,
        &[0.0, 0.0, 0.0, 1.0, 1.0, 1.0],
        None,
    )
    .expect("color_map");

    // color_region, on a region that starts at channel zero.
    let mut dst = ImageBuf::empty().unwrap();
    algo::color_matrix_transform(
        &mut dst,
        &small,
        &identity4,
        false,
        Some(data_window(&small)),
    )
    .expect("color_matrix_transform");

    // fillholes_pushpull, on a flat image with an alpha channel.
    let with_alpha = ImageSpec::new(32, 32, 4, PixelFormat::F32)
        .unwrap()
        .with_channel_names(["R", "G", "B", "A"])
        .unwrap();
    let mut rgba = ImageBuf::new(&with_alpha).unwrap();
    algo::resize(&mut rgba, &small, None, None, None).ok();
    let mut dst = ImageBuf::empty().unwrap();
    algo::fillholes_pushpull(&mut dst, &rgba).expect("fillholes_pushpull");
}

// ---------------------------------------------------------------------------
// Mixed per-channel formats
// ---------------------------------------------------------------------------

/// The channel-count guards, against a source whose channels are not all one
/// format.
///
/// `ScanLines/Blobbies.exr` is `R G B A` as halves and `Z` as a float: five
/// channels, two pixel formats, flat, and read without conversion so the
/// formats survive into the buffer. `Tiles/Spirals.exr` is the same shape
/// tiled, and either will do.
///
/// `resize_` takes the channel count from the destination spec and runs it
/// over a source iterator; `warp_` sizes a per-pixel stack buffer from the
/// destination and fills it from the source. Both compute a per-channel
/// offset that a mixed-format source works out differently from a uniform
/// one, and every test of either guard so far passed an all-F32 source where
/// that offset is a single multiplication. Flattening a deep element does not
/// get here: `flatten` writes F32 for every channel.
#[test]
fn the_channel_count_guards_see_a_mixed_format_source() {
    let Some((name, src)) = load_any(EXR_IMAGES, &["ScanLines/Blobbies.exr", "Tiles/Spirals.exr"])
    else {
        return;
    };

    let spec = src.spec().unwrap();
    let formats: Vec<_> = (0..spec.channel_count())
        .map(|c| spec.channel_format(c))
        .collect();
    println!(
        "{name}: channels {:?}, formats {formats:?}",
        spec.channel_names()
    );
    assert!(
        formats.iter().any(|format| *format != formats[0]),
        "the whole point of this fixture is that its channels differ in format: {formats:?}"
    );

    let channels = src.channel_count();
    assert!(channels >= 5, "the fixture has R G B A Z");

    let origin = origin_of(&data_window(&src));

    // Wider than the source: refused by resize.
    let wide = ImageSpec::new(64, 64, channels + 1, PixelFormat::F32)
        .unwrap()
        .with_origin(origin);
    let mut dst = ImageBuf::new(&wide).unwrap();
    let error = algo::resize(&mut dst, &src, None, None, None)
        .expect_err("a wider destination must be refused on a real source too");
    let text = error.to_string();
    assert!(
        text.contains(&(channels + 1).to_string()) && text.contains(&channels.to_string()),
        "the error should name both counts: {text}"
    );

    // Narrower than the source: refused by warp.
    let narrow = ImageSpec::new(
        data_window(&src).width() as u32,
        data_window(&src).height() as u32,
        channels - 1,
        PixelFormat::F32,
    )
    .unwrap()
    .with_origin(origin);
    let mut dst = ImageBuf::new(&narrow).unwrap();
    let identity = [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0_f32];
    let error = algo::warp(
        &mut dst,
        &src,
        &identity,
        &algo::WarpOptions::default(),
        None,
    )
    .expect_err("a narrower destination must be refused on a real source too");
    assert!(
        error.to_string().contains("per-pixel buffer"),
        "unexpected message: {error}"
    );

    // And the matching count goes through, which is the half that would
    // break if the guard counted channels in bytes rather than in channels.
    let matching = ImageSpec::new(64, 64, channels, PixelFormat::F32)
        .unwrap()
        .with_origin(origin);
    let mut dst = ImageBuf::new(&matching).unwrap();
    algo::resize(&mut dst, &src, None, None, None)
        .expect("a matching channel count must go through on a mixed-format source");
}

// ---------------------------------------------------------------------------
// The deep guards, against samples
// ---------------------------------------------------------------------------

/// Every deep refusal, re-run on a deep image that holds samples.
///
/// `guard_coverage_test::deep` builds a 4x4 and never calls
/// `set_deep_sample_count`, so it has zero samples everywhere. That is a
/// legal deep image and it is the only one those seven tests have seen. A
/// guard written as `deepdata()->samples() == 0` rather than `deep()` would
/// pass all seven and let a real render through into the kernels they exist
/// to keep it out of.
#[test]
fn the_deep_guards_hold_on_a_render_with_real_samples() {
    let Some(src) = deep_element("Balls") else {
        return;
    };
    assert!(src.is_deep(), "the fixture must be deep");

    let samples = total_samples(&src);
    println!(
        "Balls.exr: {samples} samples across {:?}",
        data_window(&src).x()
    );
    assert!(
        samples > 100_000,
        "the point of this fixture is that it has samples: {samples}"
    );

    // refuse_deep: an operation that walks contiguous pixels.
    let mut dst = ImageBuf::empty().unwrap();
    let error = algo::transpose(&mut dst, &src, None).expect_err("transpose cannot read deep");
    assert!(
        error
            .to_string()
            .contains("the contiguous pixels a deep image does not have"),
        "unexpected message: {error}"
    );

    // refuse_deep_mismatch: one deep operand and one flat.
    let flat = ImageBuf::new(&ImageSpec::new(4, 4, 1, PixelFormat::F32).unwrap()).unwrap();
    let mut dst = ImageBuf::empty().unwrap();
    let error = algo::add(&mut dst, &flat, &src, None).expect_err("deep plus flat is refused");
    assert!(
        error
            .to_string()
            .contains("flatten them first, or use deep_merge or deep_holdout"),
        "unexpected message: {error}"
    );

    // reject_deep: the statistics calls.
    let error = algo::constant_color(&src, 0.0, None).expect_err("a deep image cannot be measured");
    assert!(
        error
            .to_string()
            .contains("deep images have no contiguous pixels to measure"),
        "unexpected message: {error}"
    );

    // color_map's own deep refusal.
    let mut dst = ImageBuf::empty().unwrap();
    let error = algo::color_map(
        &mut dst,
        &src,
        Some(0),
        2,
        3,
        &[0.0, 0.0, 0.0, 1.0, 1.0, 1.0],
        None,
    )
    .expect_err("color_map cannot read deep");
    assert!(
        error.to_string().contains("deep images are not supported"),
        "unexpected message: {error}"
    );

    // color_range_check's own.
    let error = algo::color_range_check(&src, &[0.0], &[1.0], None)
        .expect_err("color_range_check cannot read deep");
    assert!(
        error.to_string().contains("deep images are not supported"),
        "unexpected message: {error}"
    );

    // fillholes_pushpull's own.
    let mut dst = ImageBuf::empty().unwrap();
    let error = algo::fillholes_pushpull(&mut dst, &src).expect_err("fillholes cannot read deep");
    assert!(
        error
            .to_string()
            .contains("fillholes_pushpull: deep images are not supported"),
        "unexpected message: {error}"
    );

    // refuse_flat_destination, with a real deep source on both sides.
    let mut flat_dst = ImageBuf::new(&ImageSpec::new(4, 4, 1, PixelFormat::F32).unwrap()).unwrap();
    let error = algo::deep_merge(&mut flat_dst, &src, &src, true, None)
        .expect_err("deep_merge needs somewhere to put samples");
    assert!(
        error
            .to_string()
            .contains("pass an empty one or a deep one"),
        "unexpected message: {error}"
    );

    // And the source survived all of that unchanged, which is the assertion
    // that a guard writing an error into a shared buffer would fail.
    assert_eq!(total_samples(&src), samples);
}

/// The `deep_paste_` fix, against a real render.
///
/// OpenImageIO's `deep_paste_` asserts `dst.deep() && src.deep()` and then,
/// twelve lines later, resets an uninitialized destination from the source
/// spec: it rejects the one shape it goes on to support. The crate now does
/// that reset first. The test that pinned the fix pasted a 4x4 holding one
/// sample per pixel, which proves the assertion is gone and nothing about
/// whether the samples arrived.
///
/// This pastes 300,000 of them and checks every pixel's count, so the fix has
/// an oracle rather than a smoke test.
#[test]
fn pasting_a_real_deep_render_into_an_empty_destination_keeps_every_sample() {
    let Some(src) = deep_element("Balls") else {
        return;
    };

    let window = data_window(&src);
    let expected = total_samples(&src);
    assert!(expected > 100_000, "the fixture should be a real render");

    // Offset zero: `position` is an offset applied to the source's own
    // coordinates, not a destination coordinate, so zero is the identity
    // paste and the destination should come out on the source's window.
    let mut dst = ImageBuf::empty().unwrap();
    algo::paste(&mut dst, [0, 0, 0], 0, &src, None)
        .expect("pasting a deep source into an empty destination is a supported call");

    assert!(dst.is_deep(), "the destination should have come out deep");
    assert_eq!(dst.channel_count(), src.channel_count());
    assert_eq!(data_window(&dst).x(), window.x());
    assert_eq!(data_window(&dst).y(), window.y());

    // Every pixel, not a sample of them: a paste that dropped a scanline or
    // shifted by one would still total correctly often enough to pass.
    let mut mismatches = 0;
    let mut checked = 0;
    for y in window.y() {
        for x in window.x() {
            let want = src.deep_sample_count(x, y);
            let got = dst.deep_sample_count(x, y);
            if want != got {
                if mismatches < 5 {
                    eprintln!("{x},{y}: expected {want} samples, got {got}");
                }
                mismatches += 1;
            }
            checked += 1;
        }
    }
    assert_eq!(mismatches, 0, "{mismatches} of {checked} pixels disagree");
    assert_eq!(total_samples(&dst), expected);

    // And the values, at the deepest pixel, where a per-sample stride error
    // has the most room to show.
    let (deepest, count) = window
        .y()
        .flat_map(|y| window.x().map(move |x| (x, y)))
        .map(|(x, y)| ((x, y), src.deep_sample_count(x, y)))
        .max_by_key(|(_, count)| *count)
        .expect("the window is not empty");
    println!("deepest pixel {deepest:?} holds {count} samples");
    assert!(count > 1, "a real render has a pixel with several samples");

    let z = src
        .spec()
        .unwrap()
        .z_channel()
        .expect("a deep render has a Z channel");
    for sample in 0..count {
        let want = src.deep_value(deepest.0, deepest.1, z, sample).unwrap();
        let got = dst.deep_value(deepest.0, deepest.1, z, sample).unwrap();
        assert_eq!(
            want, got,
            "sample {sample} of {deepest:?} changed: {want} became {got}"
        );
    }
}

/// `deep_merge` on two real render elements, which is what the operation is
/// for and what no test has asked it to do.
///
/// The existing deep tests merge images this crate wrote, four pixels wide,
/// one sample each. These are two elements of one shot with different data
/// windows, and the result has an arithmetic oracle rather than a plausible
/// one.
///
/// Not a plain sum, though. `DeepData::merge_deep_pixels` concatenates, then
/// sorts, then splits every sample against every other at its own `Z` and
/// `ZBack`, then calls `merge_overlaps`, which folds samples occupying the
/// same depth interval into one. Neither of these files has a `ZBack`
/// channel, so `Zback_channel()` falls back to `Z_channel()` and every sample
/// is a point: no sample can be split, but two samples at exactly the same
/// float `Z` do get folded. Measured on this pair, 13 of 212,029 are, which
/// is why the sum is an upper bound and not the answer.
///
/// What is exact is the per-pixel case where only one element has samples:
/// there is nothing to fold against, so the count has to come through
/// unchanged. That covers most of the frame and it is an equality.
#[test]
fn deep_merge_keeps_every_sample_from_two_real_elements() {
    let Some(a) = deep_element("Balls") else {
        return;
    };
    let Some(b) = deep_element("Trunks") else {
        return;
    };

    let a_window = data_window(&a);
    let b_window = data_window(&b);
    assert_ne!(
        (a_window.x().start, a_window.y().start),
        (b_window.x().start, b_window.y().start),
        "the fixtures should have different data windows"
    );

    let a_samples = total_samples(&a);
    let b_samples = total_samples(&b);

    let mut merged = ImageBuf::empty().unwrap();
    // Occlusion culling off, so the sample count is conserved and this has
    // an arithmetic answer instead of a plausible one.
    algo::deep_merge(&mut merged, &a, &b, false, None).expect("merging two real elements");

    assert!(merged.is_deep());
    let union = a_window.union(&b_window).unwrap();
    assert_eq!(data_window(&merged).x(), union.x());
    assert_eq!(data_window(&merged).y(), union.y());

    let total = total_samples(&merged);
    println!(
        "Balls {a_samples} + Trunks {b_samples} merged to {total} over {:?} x {:?}",
        union.x(),
        union.y()
    );
    assert!(
        total <= a_samples + b_samples,
        "a merge may fold coincident samples but may not invent any: \
         {total} from {a_samples} + {b_samples}"
    );
    assert!(
        total >= a_samples.max(b_samples),
        "a merge may not lose a whole element: {total} from {a_samples} + {b_samples}"
    );

    // The exact half. Where only one element has samples at a pixel, nothing
    // can be folded, so the merged count must equal that element's.
    let mut exact = 0u64;
    let mut both = 0u64;
    let mut folded = 0u64;
    for y in union.y() {
        for x in union.x() {
            let left = a.deep_sample_count(x, y);
            let right = b.deep_sample_count(x, y);
            let got = merged.deep_sample_count(x, y);
            if left == 0 || right == 0 {
                assert_eq!(
                    got,
                    left + right,
                    "only one element has samples at {x},{y}, so {} should have come \
                     through unchanged",
                    left + right
                );
                exact += 1;
            } else {
                both += 1;
                assert!(
                    got <= left + right && got >= left.max(right),
                    "{x},{y}: {left} and {right} merged to {got}"
                );
                folded += u64::from(left + right - got);
            }
        }
    }
    println!(
        "{exact} pixels checked exactly, {both} held samples from both elements, \
         {folded} samples folded as coincident"
    );
    assert!(
        exact > both,
        "most of the frame should be the exactly checkable case"
    );
}

/// A deep render that carries a `ZBack` channel.
///
/// Neither public corpus has one. Every deep file in both is `R G B A Z`, and
/// `DeepData::Zback_channel()` returns `Z_channel()` when `ZBack` is absent,
/// so every deep test in this crate has exercised the fallback and none of
/// them the real channel. `deep_merge` reads `Zback_channel()` four times per
/// sample pair to decide where to split, and with the fallback those
/// comparisons are between a value and itself.
///
/// The fixture is a six-channel Nuke render: `R G B A Z ZBack`, four halves
/// and two floats. It also prints whether its samples have extent, because
/// carrying the channel and being volumetric are different things and the
/// renders I have are the former: every sample has `ZBack == Z`. So this
/// pins the channel surviving a paste, and the volume arithmetic is pinned
/// in `algo_deep_test`, on an image written for it.
#[test]
fn a_deep_render_with_a_zback_channel_survives_a_paste() {
    let Some((name, src)) = deep_with_zback() else {
        return;
    };
    assert!(src.is_deep(), "{name} must be deep");

    let spec = src.spec().unwrap();
    let names = spec.channel_names().to_vec();
    let formats: Vec<_> = (0..spec.channel_count())
        .map(|c| spec.channel_format(c))
        .collect();
    let Some(zback) = names.iter().position(|n| n == "ZBack") else {
        skip_not_public(&format!("{name} has no ZBack channel"));
        return;
    };
    let zback = zback as u32;
    let z = spec.z_channel().expect("a deep render has Z");
    assert_ne!(
        z, zback,
        "the point of this fixture is that Z and ZBack are different channels"
    );
    println!("{name}: channels {names:?}, formats {formats:?}");

    // The pixel with the most samples, and a count of how many of them have
    // extent, so the file's character is in the log rather than assumed.
    let window = data_window(&src);
    let mut deepest = ((window.x().start, window.y().start), 0u32);
    let mut with_extent = 0u64;
    let mut sampled = 0u64;
    for y in window.y().step_by(3) {
        for x in window.x().step_by(3) {
            let count = src.deep_sample_count(x, y);
            if count > deepest.1 {
                deepest = ((x, y), count);
            }
            for sample in 0..count {
                let front = src.deep_value(x, y, z, sample).unwrap();
                let back = src.deep_value(x, y, zback, sample).unwrap();
                sampled += 1;
                if back > front {
                    with_extent += 1;
                }
            }
        }
    }
    let ((x, y), count) = deepest;
    println!(
        "{name}: {with_extent} of {sampled} sampled samples have extent; \
         deepest pixel {x},{y} holds {count}"
    );
    assert!(count > 1, "a real render has a pixel with several samples");

    let mut dst = ImageBuf::empty().unwrap();
    algo::paste(&mut dst, [0, 0, 0], 0, &src, None)
        .expect("pasting a six-channel deep render into an empty destination");

    assert!(dst.is_deep());
    assert_eq!(
        dst.spec().unwrap().channel_names(),
        names.as_slice(),
        "the rebuilt spec must keep ZBack, and keep it in place"
    );
    assert_eq!(
        dst.spec().unwrap().z_channel(),
        Some(z),
        "the rebuilt spec must keep the Z channel index"
    );
    let rebuilt: Vec<_> = (0..dst.spec().unwrap().channel_count())
        .map(|c| dst.spec().unwrap().channel_format(c))
        .collect();
    assert_eq!(
        rebuilt, formats,
        "the per-channel formats must survive: four halves and two floats"
    );

    assert_eq!(dst.deep_sample_count(x, y), count);
    for sample in 0..count {
        for channel in [z, zback] {
            let want = src.deep_value(x, y, channel, sample).unwrap();
            let got = dst.deep_value(x, y, channel, sample).unwrap();
            assert_eq!(
                want, got,
                "channel {channel} sample {sample} of {x},{y} changed: {want} became {got}"
            );
        }
    }
}

/// A real deep render pasted at an offset, which is where OpenImageIO loses
/// the samples.
///
/// `deep_paste_` never calls `IBAprep`. Where the flat path sizes an
/// uninitialized destination to the destination region, the deep path does
/// `dst.reset(src.spec())`, which takes the *source's* window, and then
/// writes at `x + offset` through `ImageBuf::set_deep_samples` and
/// `copy_deep_pixel`. Both of those call `pixelindex()` with range checking
/// defaulting to off, so a coordinate past the destination is not clipped: it
/// becomes a linear index that wraps into the next scanline. The result is
/// the source shifted by one row and a couple of columns, missing whatever
/// ran off the end, returned as success.
///
/// On this element that was 138,536 of 310,184 pixels coming back empty while
/// `paste` returned `Ok`. The crate now sizes the destination from the
/// destination region, as `IBAprep` does, and clips when it cannot. This
/// checks the whole frame, because the failure mode is a shift, and a shift
/// keeps the totals nearly right.
#[test]
fn a_real_deep_render_pasted_at_an_offset_lands_where_it_was_asked() {
    let Some(src) = deep_element("Balls") else {
        return;
    };

    let window = data_window(&src);
    let offset = [37, 11, 0];
    let expected = total_samples(&src);

    let mut dst = ImageBuf::empty().unwrap();
    algo::paste(&mut dst, offset, 0, &src, None).expect("an offset deep paste is a valid call");

    // The destination is the source's window moved by the offset, which is
    // what the flat path produces for the same call.
    let moved = data_window(&dst);
    assert_eq!(
        moved.x(),
        window.x().start + offset[0]..window.x().end + offset[0],
        "the destination should cover the region the source was asked to land on"
    );
    assert_eq!(
        moved.y(),
        window.y().start + offset[1]..window.y().end + offset[1]
    );

    // Not one sample lost, and every one at the coordinate it was asked for.
    assert_eq!(
        total_samples(&dst),
        expected,
        "an offset paste lost samples"
    );
    let mut misplaced = 0;
    for y in window.y() {
        for x in window.x() {
            let want = src.deep_sample_count(x, y);
            let got = dst.deep_sample_count(x + offset[0], y + offset[1]);
            if want != got {
                if misplaced < 5 {
                    eprintln!(
                        "{x},{y} -> {},{}: expected {want} samples, got {got}",
                        x + offset[0],
                        y + offset[1]
                    );
                }
                misplaced += 1;
            }
        }
    }
    assert_eq!(misplaced, 0, "{misplaced} pixels landed somewhere else");
}

/// An identifier channel read as a float is not a rounded identifier.
///
/// OpenEXR's own deep test images carry `id` and `particleid` as `uint32`
/// beside half `R G B A Z`. `DeepData::deep_value` converts through
/// `ConstDataArrayProxy<unsigned int, float>`, which is `convert_type`, which
/// *normalises*: it divides by `u32::MAX`. So the id 2,056,745,377 comes back
/// from `deep_value` as 0.47887334.
///
/// That is the part worth a test. The number is not out of range, not
/// infinite, and not obviously wrong: it is a plausible colour value in 0..1,
/// and a caller walking every channel of a deep pixel with `deep_value` reads
/// the five half channels correctly and scales the two identifiers into
/// nothing without a word. `deep_value_uint` is the accessor that answers.
///
/// Nothing in either corpus has an integer channel, so this could not be
/// written before. It also happens to be the only deep fixture here whose `Z`
/// is half rather than float.
#[test]
fn an_integer_deep_channel_reads_as_an_identifier_and_not_as_a_colour() {
    let Some((name, src)) = deep_with_identifiers() else {
        return;
    };

    let spec = src.spec().unwrap();
    let names = spec.channel_names().to_vec();
    let formats: Vec<_> = (0..spec.channel_count())
        .map(|c| spec.channel_format(c))
        .collect();
    println!("{name}: {names:?} {formats:?}");

    let id = names
        .iter()
        .position(|n| n == "id" || n == "modelid" || n == "particleid")
        .map(|index| index as u32)
        .expect("the fixture is chosen for its identifier channel");
    assert_eq!(
        spec.channel_format(id),
        Some(PixelFormat::U32),
        "the identifier channel must be the unsigned one"
    );

    // Also the first deep fixture whose Z is not a float, which is its own
    // reason to run the guards over this file.
    let z = spec.z_channel().expect("a deep render has Z");
    println!("  Z is channel {z}, format {:?}", spec.channel_format(z));

    // Find a sample whose identifier is large enough for the difference to be
    // unmistakable.
    let window = data_window(&src);
    let mut found = None;
    'search: for y in window.y() {
        for x in window.x() {
            for sample in 0..src.deep_sample_count(x, y) {
                let value = src.deep_value_uint(x, y, id, sample).unwrap();
                if value > 1 << 24 {
                    found = Some((x, y, sample, value));
                    break 'search;
                }
            }
        }
    }
    let (x, y, sample, identifier) =
        found.expect("an id channel should hold identifiers, not small integers");

    let as_float = src.deep_value(x, y, id, sample).unwrap();
    println!("  {x},{y} sample {sample}: id {identifier}, as a float {as_float}");

    // Normalised, not rounded: the float is the identifier over u32::MAX.
    let normalised = identifier as f64 / f64::from(u32::MAX);
    assert!(
        (f64::from(as_float) - normalised).abs() < 1e-6,
        "deep_value should return {normalised} for an id of {identifier}, got {as_float}"
    );
    assert!(
        (0.0..=1.0).contains(&as_float),
        "and the result is in colour range, which is what hides it: {as_float}"
    );

    // The integer accessor is exact, which is the whole reason it exists.
    assert_eq!(src.deep_value_uint(x, y, id, sample).unwrap(), identifier);
    assert!(
        identifier > as_float as u32,
        "reading the id through the float accessor and casting back loses it entirely: \
         {identifier} became {}",
        as_float as u32
    );
}

/// The guards, over a deep image whose channels are not all floating point.
///
/// Every deep guard test, synthetic or corpus, has run on an image of halves
/// and floats. These images mix half and `uint32`, and a guard that reasoned
/// about channel formats rather than about `deep()` would answer differently
/// here. `Z` is half as well, so `Zback_channel()`'s fallback is comparing
/// halves rather than floats.
#[test]
fn the_deep_guards_hold_on_an_image_with_integer_channels() {
    let Some((name, src)) = deep_with_identifiers() else {
        return;
    };
    assert!(src.is_deep(), "{name} must be deep");
    let samples = total_samples(&src);
    println!("{name}: {samples} samples");
    assert!(samples > 0, "the fixture should hold samples");

    let mut dst = ImageBuf::empty().unwrap();
    let error = algo::transpose(&mut dst, &src, None).expect_err("transpose cannot read deep");
    assert!(
        error
            .to_string()
            .contains("the contiguous pixels a deep image does not have"),
        "unexpected message: {error}"
    );

    let error = algo::constant_color(&src, 0.0, None).expect_err("a deep image cannot be measured");
    assert!(
        error
            .to_string()
            .contains("deep images have no contiguous pixels to measure"),
        "unexpected message: {error}"
    );

    let mut dst = ImageBuf::empty().unwrap();
    let error = algo::fillholes_pushpull(&mut dst, &src).expect_err("fillholes cannot read deep");
    assert!(
        error
            .to_string()
            .contains("fillholes_pushpull: deep images are not supported"),
        "unexpected message: {error}"
    );

    let mut flat_dst = ImageBuf::new(&ImageSpec::new(4, 4, 1, PixelFormat::F32).unwrap()).unwrap();
    let error = algo::deep_merge(&mut flat_dst, &src, &src, true, None)
        .expect_err("deep_merge needs somewhere to put samples");
    assert!(
        error
            .to_string()
            .contains("pass an empty one or a deep one"),
        "unexpected message: {error}"
    );
}

/// A paste of a deep image with integer channels keeps the integers.
///
/// `DeepData::copy_deep_sample` has a branch for this: where both sides are
/// `UINT32` it copies through `deep_value_uint`, and otherwise through
/// `deep_value`, which would normalise. The branch depends on the
/// destination's channel type matching, so it depends on the spec the paste
/// built for the destination being the source's and not a default. That is
/// precisely what this crate's paste fix rebuilds, so it is worth checking
/// that the identifiers come through rather than arriving as 0.478.
#[test]
fn pasting_a_deep_image_with_integer_channels_keeps_the_identifiers() {
    let Some((name, src)) = deep_with_identifiers() else {
        return;
    };

    let spec = src.spec().unwrap();
    let names = spec.channel_names().to_vec();
    let integer_channels: Vec<u32> = (0..spec.channel_count())
        .filter(|c| spec.channel_format(*c) == Some(PixelFormat::U32))
        .collect();
    assert!(
        !integer_channels.is_empty(),
        "{name} is chosen for its integer channels"
    );

    let window = data_window(&src);
    let expected = total_samples(&src);

    let offset = [5, 7, 0];
    let mut dst = ImageBuf::empty().unwrap();
    algo::paste(&mut dst, offset, 0, &src, None).expect("pasting a deep image with identifiers");

    assert_eq!(dst.spec().unwrap().channel_names(), names.as_slice());
    for channel in &integer_channels {
        assert_eq!(
            dst.spec().unwrap().channel_format(*channel),
            Some(PixelFormat::U32),
            "channel {channel} must still be unsigned in the destination"
        );
    }
    assert_eq!(total_samples(&dst), expected, "the paste lost samples");

    // Every identifier, at every sample, at the offset coordinate.
    let mut compared = 0u64;
    for y in window.y() {
        for x in window.x() {
            let count = src.deep_sample_count(x, y);
            assert_eq!(
                dst.deep_sample_count(x + offset[0], y + offset[1]),
                count,
                "{x},{y} did not land at {},{}",
                x + offset[0],
                y + offset[1]
            );
            for sample in 0..count {
                for channel in &integer_channels {
                    let want = src.deep_value_uint(x, y, *channel, sample).unwrap();
                    let got = dst
                        .deep_value_uint(x + offset[0], y + offset[1], *channel, sample)
                        .unwrap();
                    assert_eq!(
                        want, got,
                        "identifier in channel {channel} at {x},{y} sample {sample} \
                         changed: {want} became {got}"
                    );
                    compared += 1;
                }
            }
        }
    }
    println!("{name}: {compared} identifiers came through a paste at {offset:?}");
    assert!(
        compared > 0,
        "the fixture should have identifiers to compare"
    );
}

// ---------------------------------------------------------------------------
// Mip levels, on a pyramid built to be told apart
// ---------------------------------------------------------------------------

/// Reading a mip level, on a file where reading the wrong one is visible.
///
/// `MultiResolution/ColorCodedLevels.exr` describes itself as "a mip-map image
/// with color-coded levels", which is exactly the fixture this needs: level 0
/// is grey, level 1 cyan, level 2 magenta, level 3 blue. Every other mip test
/// in this crate uses a ramp or a checker, where the levels are plausible
/// resamplings of each other and reading the wrong one looks like filtering.
///
/// This is how `ImageBuf::read` was found to be discarding the level it was
/// attached to. `from_path_at(path, 0, 3)` reported a 64 by 64 spec and a
/// `mip_level` of 3, and then `read` returned the 512 by 512 grey base image
/// with `mip_level` back at 0, reporting success. On a ramp that is a bigger
/// array of similar numbers. Here it is grey where it should be blue.
#[test]
fn every_mip_level_of_a_colour_coded_pyramid_reads_as_itself() {
    let Some(root) = corpus_root(EXR_IMAGES) else {
        skip(&format!("set {EXR_IMAGES}"));
        return;
    };
    let path = root.join("MultiResolution/ColorCodedLevels.exr");
    if !path.is_file() {
        skip(&format!("{} is not in the corpus", path.display()));
        return;
    }

    // The level count is always available from a cache. Whether it is also
    // available from the buffer depends on the library version.
    //
    // Up to and including 3.2.1.3, `ImageBufImpl::init_spec` filled
    // `m_nmiplevels` only on its cache-backed branch, and the `ImageInput`
    // branch left it at zero while setting `m_nsubimages` beside it, so
    // `ImageBuf::mip_level_count` answered 0 for every file. PR #5437 gave the
    // `ImageInput` branch a real value from `oiio:miplevels`. It landed on main
    // as `ee1c64138` and was backported onto the 3.2 line as `76fca2633`, which
    // ships in **v3.2.2.0**, so the change is in a release rather than only in
    // 3.3. Both behaviours are asserted rather than one being pinned, so this
    // keeps testing the asymmetry on every supported OpenImageIO instead of
    // failing on the ones that answer.
    //
    // Note for anyone re-deriving the boundary: asking whether `ee1c64138` is
    // an ancestor of `v3.2.2.0` says no, because the released fix is the
    // backport. Read the file at the tag instead.
    let cache = oiio::ImageCache::new().expect("could not make a cache");
    let count = cache
        .mip_level_count(&path, 0)
        .expect("the cache should report the pyramid");
    assert!(
        count > 3,
        "the fixture should have a pyramid: {count} levels"
    );

    let base = ImageBuf::from_path(&path).expect("could not open the pyramid");
    let fills_nmiplevels_without_a_cache =
        oiio_sys::imageio::openimageio_version() >= 30_202;
    let expected = if fills_nmiplevels_without_a_cache {
        i32::try_from(count).expect("level count should fit an i32")
    } else {
        0
    };
    assert_eq!(
        base.mip_level_count(),
        expected,
        "ImageBuf::mip_level_count on a buffer with no cache: expected \
         {expected} on OpenImageIO {}, where the ImageInput branch {} fill \
         m_nmiplevels",
        oiio_sys::imageio::openimageio_version(),
        if fills_nmiplevels_without_a_cache {
            "does"
        } else {
            "does not"
        }
    );
    assert_eq!(
        base.subimage_count(),
        1,
        "subimage_count, from the same function, is correct on every version"
    );

    let mut colours = Vec::new();
    let mut previous: Option<[u32; 3]> = None;
    for level in 0..4u32 {
        let mut image =
            ImageBuf::from_path_at(&path, 0, level).expect("could not open a mip level");

        // What it says before reading, which is where this used to agree and
        // then diverge.
        assert_eq!(image.mip_level(), level);
        let announced = image.spec().unwrap().dimensions();

        // `read` with no arguments has to keep it.
        image.read().expect("could not read a mip level");
        assert_eq!(
            image.spec().unwrap().dimensions(),
            announced,
            "read() changed the level from {level} to {}",
            image.mip_level()
        );
        assert_eq!(image.mip_level(), level);

        // Each level is half the last, which is what makes it a pyramid.
        if let Some(last) = previous {
            assert_eq!(
                announced[0],
                last[0] / 2,
                "level {level} should be half of level {}",
                level - 1
            );
        }
        previous = Some(announced);

        let mut pixel = vec![0.0_f32; image.channel_count() as usize];
        image
            .pixel_at_into(
                (announced[0] / 2) as i32,
                (announced[1] / 2) as i32,
                oiio::Wrap::Black,
                &mut pixel,
            )
            .unwrap();
        println!(
            "level {level}: {}x{} centre [{:.3}, {:.3}, {:.3}]",
            announced[0], announced[1], pixel[0], pixel[1], pixel[2]
        );
        colours.push([pixel[0], pixel[1], pixel[2]]);
    }

    // The colour coding, which is the assertion a ramp cannot support: no two
    // of these levels look alike, so a read that answered from the wrong one
    // could not pass.
    for (level, colour) in colours.iter().enumerate() {
        for (other, against) in colours.iter().enumerate().skip(level + 1) {
            let distance: f32 = colour
                .iter()
                .zip(against)
                .map(|(a, b)| (a - b) * (a - b))
                .sum();
            assert!(
                distance > 0.01,
                "levels {level} and {other} should be coded differently: \
                 {colour:?} against {against:?}"
            );
        }
    }
}
