//! Measuring an image rather than changing one.
//!
//! None of these fills a destination buffer, and several of OpenImageIO's
//! versions read past the end of something when handed a region the rest of
//! `ImageBufAlgo` would accept. The guards are asserted here alongside the
//! measurements themselves.

mod common;

use oiio::{algo, ImageBuf, ImageSpec, PixelFormat, Roi};

fn flat(width: u32, height: u32, values: &[f32]) -> ImageBuf {
    let spec = ImageSpec::new(width, height, values.len() as u32, PixelFormat::F32).unwrap();
    let mut image = ImageBuf::new(&spec).unwrap();
    algo::fill(&mut image, values, None).unwrap();
    image
}

#[test]
fn color_range_check_counts_and_refuses_bad_ranges() {
    // 4x4, one channel: every pixel holds 0.5.
    let image = flat(4, 4, &[0.5]);

    let counts = algo::color_range_check(&image, &[0.0], &[1.0], None).unwrap();
    assert_eq!(counts.in_range, 16);
    assert_eq!(counts.low + counts.high, 0);

    // Bounds that exclude the value: everything is high.
    let counts = algo::color_range_check(&image, &[0.0], &[0.25], None).unwrap();
    assert_eq!(counts.high, 16);

    // Empty bound slices mean no bound on that side.
    let counts = algo::color_range_check(&image, &[], &[], None).unwrap();
    assert_eq!(counts.in_range, 16);

    // A channel range naming no channel is an error, not zeroes-and-success.
    let bad = Roi::new(0..4, 0..4, 0..1, 5..8).unwrap();
    assert!(algo::color_range_check(&image, &[0.0], &[1.0], Some(bad)).is_err());
}

#[test]
fn pixel_stats_reports_range_mean_and_spread() {
    // A ramp from 0 to 1 across 100 pixels, in one channel.
    let spec = ImageSpec::new(100, 1, 1, PixelFormat::F32).unwrap();
    let mut image = ImageBuf::new(&spec).unwrap();
    let values: Vec<f32> = (0..100).map(|x| x as f32 / 99.0).collect();
    image
        .set_pixels(spec.data_window().unwrap(), &values)
        .unwrap();

    let stats = algo::pixel_stats(&image, None).unwrap();
    assert_eq!(stats.min.len(), 1);
    assert!((stats.min[0] - 0.0).abs() < 1e-6, "{:?}", stats.min);
    assert!((stats.max[0] - 1.0).abs() < 1e-6, "{:?}", stats.max);
    assert!(
        (stats.average[0] - 0.5).abs() < 1e-3,
        "a uniform ramp averages a half, got {:?}",
        stats.average
    );
    assert_eq!(stats.finite_count[0], 100);
    assert_eq!(stats.nan_count[0], 0);
    assert_eq!(stats.infinite_count[0], 0);
    assert!(stats.standard_deviation[0] > 0.0);
}

/// Values that are not finite are counted, not folded into the average, so a
/// few bad pixels do not destroy the range.
#[test]
fn pixel_stats_counts_nan_and_infinity_separately() {
    let spec = ImageSpec::new(4, 1, 1, PixelFormat::F32).unwrap();
    let mut image = ImageBuf::new(&spec).unwrap();
    let values = [1.0_f32, 3.0, f32::NAN, f32::INFINITY];
    image
        .set_pixels(spec.data_window().unwrap(), &values)
        .unwrap();

    let stats = algo::pixel_stats(&image, None).unwrap();
    assert_eq!(stats.nan_count[0], 1);
    assert_eq!(stats.infinite_count[0], 1);
    assert_eq!(stats.finite_count[0], 2);
    assert_eq!(stats.min[0], 1.0);
    assert_eq!(stats.max[0], 3.0, "infinity should not become the maximum");
    assert_eq!(stats.average[0], 2.0);
}

/// Every vector is as long as the image has channels, whatever the region
/// asked for, and a channel the region skipped is told apart by its count
/// rather than by its value.
#[test]
fn pixel_stats_reports_every_channel_even_outside_the_region() {
    let image = flat(4, 4, &[0.25, 0.5, 0.75]);
    let two_channels = Roi::new(0..4, 0..4, 0..1, 0..2).unwrap();

    let stats = algo::pixel_stats(&image, Some(two_channels)).unwrap();
    assert_eq!(stats.min.len(), 3, "all three channels should be reported");
    assert_eq!(stats.finite_count[0], 16);
    assert_eq!(stats.finite_count[1], 16);
    assert_eq!(
        stats.finite_count[2], 0,
        "the third channel was outside the region, and its zero average means \
         'not measured' rather than 'measured as zero'"
    );
    assert_eq!(stats.average[2], 0.0);
}

#[test]
fn histogram_buckets_a_channel() {
    let spec = ImageSpec::new(100, 1, 1, PixelFormat::F32).unwrap();
    let mut image = ImageBuf::new(&spec).unwrap();
    let values: Vec<f32> = (0..100).map(|x| x as f32 / 100.0).collect();
    image
        .set_pixels(spec.data_window().unwrap(), &values)
        .unwrap();

    let counts = algo::histogram(&image, 0, 4, 0.0..1.0, false, None).unwrap();
    assert_eq!(counts.len(), 4);
    assert_eq!(counts.iter().sum::<u64>(), 100);
    assert_eq!(counts, vec![25, 25, 25, 25], "a uniform ramp fills evenly");
}

/// Out-of-range values are counted in the nearest bucket rather than dropped,
/// so the totals still add up.
#[test]
fn histogram_clamps_rather_than_discarding() {
    let spec = ImageSpec::new(4, 1, 1, PixelFormat::F32).unwrap();
    let mut image = ImageBuf::new(&spec).unwrap();
    image
        .set_pixels(spec.data_window().unwrap(), &[-5.0_f32, 0.1, 0.9, 5.0])
        .unwrap();

    let counts = algo::histogram(&image, 0, 2, 0.0..1.0, false, None).unwrap();
    assert_eq!(counts.iter().sum::<u64>(), 4, "nothing should be discarded");
    assert_eq!(counts, vec![2, 2]);
}

/// OpenImageIO's histogram is alone among the statistics in never clamping
/// the channel range, and with `ignore_empty` its inner loop reads every
/// channel up to it. A `Roi` can name far more channels than the image has —
/// nothing stops it — so the binding has to clamp.
///
/// The default region is not the dangerous one: `ROI()` carries `chend = 0`.
/// The hazard needs a channel range asked for explicitly, which is what this
/// passes.
#[test]
fn histogram_clamps_a_channel_range_beyond_the_image() {
    let image = flat(8, 8, &[0.5, 0.5, 0.5]);
    let far_too_many = Roi::new(0..8, 0..8, 0..1, 0..10_000).unwrap();

    let counts = algo::histogram(&image, 0, 4, 0.0..1.0, true, Some(far_too_many)).unwrap();
    assert_eq!(
        counts.iter().sum::<u64>(),
        64,
        "every pixel should be counted exactly once"
    );

    // And the default region, which is the ordinary call.
    let counts = algo::histogram(&image, 0, 4, 0.0..1.0, true, None).unwrap();
    assert_eq!(counts.iter().sum::<u64>(), 64);
}

#[test]
fn histogram_rejects_a_channel_that_is_not_there() {
    let image = flat(4, 4, &[0.5]);
    let error = algo::histogram(&image, 7, 4, 0.0..1.0, false, None).unwrap_err();
    println!("missing channel reported as: {error}");
}

#[test]
fn constant_color_finds_the_shared_colour() {
    let image = flat(8, 8, &[0.25, 0.5, 0.75]);
    let color = algo::constant_color(&image, 0.0, None).unwrap();
    assert_eq!(color, Some(vec![0.25, 0.5, 0.75]));

    // Change one pixel and it is no longer constant.
    let mut varied = flat(8, 8, &[0.25, 0.5, 0.75]);
    let corner = Roi::new(0..1, 0..1, 0..1, 0..3).unwrap();
    algo::fill(&mut varied, &[1.0, 1.0, 1.0], Some(corner)).unwrap();
    assert_eq!(algo::constant_color(&varied, 0.0, None).unwrap(), None);
}

/// OpenImageIO sizes its reference buffer to the region's channel count but
/// fills it by absolute channel number, so a region above channel zero writes
/// past the end of a heap allocation. Refusing it is the only safe answer.
#[test]
fn constant_color_refuses_a_region_above_channel_zero() {
    let image = flat(8, 8, &[0.25, 0.5, 0.75]);
    let upper = Roi::new(0..8, 0..8, 0..1, 1..3).unwrap();

    let error = algo::constant_color(&image, 0.0, Some(upper)).unwrap_err();
    println!("offset channel range rejected as: {error}");
    assert!(
        error.to_string().contains("channel zero"),
        "unexpected error {error}"
    );

    // Starting at zero is fine, including a narrowed end.
    let lower = Roi::new(0..8, 0..8, 0..1, 0..2).unwrap();
    assert!(algo::constant_color(&image, 0.0, Some(lower))
        .unwrap()
        .is_some());
}

#[test]
fn is_constant_channel_tests_one_channel() {
    let image = flat(8, 8, &[0.25, 0.5, 0.75]);
    assert!(algo::is_constant_channel(&image, 1, 0.5, 0.0, None).unwrap());
    assert!(!algo::is_constant_channel(&image, 1, 0.6, 0.0, None).unwrap());
    // Within a threshold it counts as constant.
    assert!(algo::is_constant_channel(&image, 1, 0.6, 0.2, None).unwrap());
}

/// A channel index that does not exist is an error, not a `false`. OpenImageIO
/// answers both with the same bool and records nothing.
#[test]
fn is_constant_channel_separates_a_bad_index_from_a_false_answer() {
    let image = flat(4, 4, &[0.5, 0.5]);
    let error = algo::is_constant_channel(&image, 9, 0.5, 0.0, None).unwrap_err();
    println!("out-of-range channel reported as: {error}");
    assert!(
        error.to_string().contains('9'),
        "the message should name the channel, got {error}"
    );
}

#[test]
fn is_monochrome_compares_channels_within_a_pixel() {
    let grey = flat(8, 8, &[0.4, 0.4, 0.4]);
    assert!(algo::is_monochrome(&grey, 0.0, None).unwrap());

    let coloured = flat(8, 8, &[0.4, 0.5, 0.4]);
    assert!(!algo::is_monochrome(&coloured, 0.0, None).unwrap());
}

/// Alpha is compared along with the colours, so an opaque grey image is not
/// monochrome unless the region excludes alpha.
#[test]
fn is_monochrome_counts_alpha_unless_the_region_excludes_it() {
    let opaque_grey = flat(8, 8, &[0.4, 0.4, 0.4, 1.0]);
    assert!(
        !algo::is_monochrome(&opaque_grey, 0.0, None).unwrap(),
        "alpha of 1 differs from a colour of 0.4, so the whole pixel is not \
         monochrome"
    );

    let colours_only = Roi::new(0..8, 0..8, 0..1, 0..3).unwrap();
    assert!(algo::is_monochrome(&opaque_grey, 0.0, Some(colours_only)).unwrap());
}

#[test]
fn nonzero_region_shrink_wraps_the_content() {
    let mut image = flat(16, 16, &[0.0, 0.0, 0.0]);
    let content = Roi::new(4..9, 6..10, 0..1, 0..3).unwrap();
    algo::fill(&mut image, &[1.0, 1.0, 1.0], Some(content)).unwrap();

    let found = algo::nonzero_region(&image, None).unwrap().unwrap();
    assert_eq!(found.x(), 4..9);
    assert_eq!(found.y(), 6..10);
}

/// An entirely black image has no nonzero region, which OpenImageIO spells two
/// different ways depending on the path; both mean the same thing here.
#[test]
fn nonzero_region_of_a_black_image_is_nothing() {
    let black = flat(8, 8, &[0.0, 0.0, 0.0]);
    assert_eq!(algo::nonzero_region(&black, None).unwrap(), None);
}

#[test]
fn nonzero_region_refuses_a_region_above_channel_zero() {
    let image = flat(8, 8, &[0.0, 1.0, 0.0]);
    let upper = Roi::new(0..8, 0..8, 0..1, 1..3).unwrap();
    let error = algo::nonzero_region(&image, Some(upper)).unwrap_err();
    println!("offset channel range rejected as: {error}");
}

#[test]
fn pixel_hash_distinguishes_different_pixels() {
    let a = flat(8, 8, &[0.25, 0.5, 0.75]);
    let b = flat(8, 8, &[0.25, 0.5, 0.76]);

    let hash_a = algo::pixel_hash_sha1(&a, "", None).unwrap();
    let hash_b = algo::pixel_hash_sha1(&b, "", None).unwrap();
    println!("{hash_a}\n{hash_b}");

    assert_eq!(hash_a.len(), 40, "a SHA-1 digest is 40 hex characters");
    assert!(hash_a.chars().all(|c| c.is_ascii_hexdigit()));
    assert_ne!(hash_a, hash_b);

    // The same pixels hash the same way.
    let again = flat(8, 8, &[0.25, 0.5, 0.75]);
    assert_eq!(algo::pixel_hash_sha1(&again, "", None).unwrap(), hash_a);

    // Extra information is mixed in.
    assert_ne!(algo::pixel_hash_sha1(&a, "take 2", None).unwrap(), hash_a);
}

/// Every measurement refuses a deep image rather than dereferencing the null
/// pixel pointer a deep buffer reports.
#[test]
fn the_measurements_refuse_deep_images() {
    let spec = ImageSpec::new(8, 8, 3, PixelFormat::F32).unwrap().as_deep();
    let deep = ImageBuf::new(&spec).unwrap();

    assert!(algo::pixel_stats(&deep, None).is_err());
    assert!(algo::histogram(&deep, 0, 4, 0.0..1.0, false, None).is_err());
    assert!(algo::constant_color(&deep, 0.0, None).is_err());
    assert!(algo::is_constant_channel(&deep, 0, 0.0, 0.0, None).is_err());
    assert!(algo::is_monochrome(&deep, 0.0, None).is_err());
    assert!(algo::pixel_hash_sha1(&deep, "", None).is_err());
}

/// Counting colors: matches within the default tolerance, one count per
/// color row, and the mis-shaped or oversized requests are refused.
#[test]
fn color_count_counts_each_color_and_validates_its_arrays() {
    let mut image = flat(4, 4, &[0.25, 0.5, 0.75]);
    image.set_pixel_at(0, 0, &[1.0, 0.0, 0.0]).unwrap();
    image.set_pixel_at(3, 3, &[1.0, 0.0, 0.0]).unwrap();

    let counts = algo::color_count(
        &image,
        &[1.0, 0.0, 0.0, 0.25, 0.5, 0.75, 0.5, 0.5, 0.5],
        &[],
        None,
    )
    .unwrap();
    assert_eq!(counts, vec![2, 14, 0]);

    // A tolerance wide enough to catch everything.
    let counts = algo::color_count(&image, &[0.5, 0.25, 0.4], &[1.0], None).unwrap();
    assert_eq!(counts, vec![16]);

    // A colors array that is not a whole number of colors.
    let error = algo::color_count(&image, &[1.0, 0.0], &[], None).unwrap_err();
    assert!(
        error.to_string().contains("whole number"),
        "unexpected error: {error}"
    );

    // More colors than the stack-scratch bound.
    let too_many = vec![0.0_f32; 3 * 32769];
    let error = algo::color_count(&image, &too_many, &[], None).unwrap_err();
    assert!(
        error.to_string().contains("32768"),
        "unexpected error: {error}"
    );
}

/// Yee's perceptual metric: identical images pass, blatantly different ones
/// fail, and the parameter and region guards hold.
#[test]
fn compare_yee_sees_identical_and_blatant_differences() {
    let a = flat(32, 32, &[0.2, 0.4, 0.6]);
    let same = algo::compare_yee(&a, &a, 100.0, 45.0, None).unwrap();
    assert!(
        same.perceptually_equal(),
        "an image equals itself: {same:?}"
    );
    assert_eq!(same.failures, 0);

    let b = flat(32, 32, &[0.9, 0.1, 0.1]);
    let different = algo::compare_yee(&a, &b, 100.0, 45.0, None).unwrap();
    assert!(
        different.failures > 0,
        "wildly different colors should be visible: {different:?}"
    );
    assert!(!different.perceptually_equal());

    // Nonsense viewing parameters are refused, not folded into NaN.
    assert!(algo::compare_yee(&a, &b, 0.0, 45.0, None).is_err());
    assert!(algo::compare_yee(&a, &b, 100.0, 180.0, None).is_err());
    assert!(algo::compare_yee(&a, &b, f32::NAN, 45.0, None).is_err());

    // A region beyond both images would compare pixels neither has.
    let outside = Roi::new(0..64, 0..64, 0..1, 0..3).unwrap();
    assert!(algo::compare_yee(&a, &b, 100.0, 45.0, Some(outside)).is_err());
}

/// A volumetric comparison would silently drop every slice past the first
/// inside OpenImageIO, so it is refused.
#[test]
fn compare_yee_refuses_volumetric_images() {
    let spec = ImageSpec::new(8, 8, 3, PixelFormat::F32)
        .unwrap()
        .with_depth(2)
        .unwrap();
    let mut a = ImageBuf::new(&spec).unwrap();
    algo::fill(&mut a, &[0.5, 0.5, 0.5], None).unwrap();
    let error = algo::compare_yee(&a, &a, 100.0, 45.0, None).unwrap_err();
    assert!(
        error.to_string().contains("two-dimensional"),
        "unexpected error: {error}"
    );
}
