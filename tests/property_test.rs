//! Property tests: throw structurally awkward images and regions at every
//! operation and insist it answers rather than misbehaves.
//!
//! This is the always-on half of the fuzzing story. It runs on stable, in
//! every CI run, and it generates the *shapes* that broke things rather than
//! random bytes: a deep buffer, an empty one, a data window far from the
//! origin, a display window larger than the data window, a region reaching
//! outside the image or starting above channel zero, a destination narrower
//! than the source. Every soundness bug found in this crate so far was one of
//! those shapes rather than an unusual pixel value.
//!
//! `contrib/fuzzing.md` describes the other half — libFuzzer against an
//! address-sanitised OpenImageIO — and why this half cannot replace it: a read
//! just past a heap allocation returns neighbouring memory silently, and
//! nothing here can see that. What this file *can* enforce is that the crate
//! answers every input with `Ok` or `Err` rather than dying, and that an `Ok`
//! never carries the fingerprints of a bad read.

mod common;

use oiio::algo::{FitMode, Operand, WarpOptions};
use oiio::{algo, ImageBuf, ImageSpec, PixelFormat, Roi};
use proptest::prelude::*;

/// The value OpenImageIO leaves behind when a kernel reads a pixel that was
/// never written: `±FLT_MAX`, or something of that order. No operation here
/// promises to produce one, so seeing it in a successful result means a read
/// went somewhere it should not have.
const IMPLAUSIBLE: f32 = 1.0e30;

/// A shape worth testing, rather than a uniformly random one.
#[derive(Debug, Clone)]
struct Shape {
    width: u32,
    height: u32,
    channels: u32,
    format: PixelFormat,
    origin: [i32; 3],
    /// A display window that differs from the data window, which is what an
    /// overscan render looks like and what broke `ifft`.
    overscan: bool,
    deep: bool,
}

fn any_shape() -> impl Strategy<Value = Shape> {
    (
        1_u32..=12,
        1_u32..=12,
        1_u32..=6,
        prop_oneof![
            Just(PixelFormat::F32),
            Just(PixelFormat::F16),
            Just(PixelFormat::U8),
            Just(PixelFormat::U16),
        ],
        prop_oneof![
            Just([0, 0, 0]),
            Just([5, 7, 0]),
            Just([-4, -9, 0]),
            Just([100_000, 100_000, 0]),
        ],
        any::<bool>(),
        any::<bool>(),
    )
        .prop_map(
            |(width, height, channels, format, origin, overscan, deep)| Shape {
                width,
                height,
                channels,
                format,
                origin,
                overscan,
                deep,
            },
        )
}

impl Shape {
    fn build(&self) -> Option<ImageBuf> {
        let mut spec = ImageSpec::new(self.width, self.height, self.channels, self.format)
            .ok()?
            .with_origin(self.origin);
        if self.overscan {
            // A display window twice the size, offset from the data window.
            spec = spec
                .with_full_window(self.origin, [self.width * 2, self.height * 2, 1])
                .ok()?;
        }
        if self.deep {
            spec = spec.as_deep();
        }
        let mut image = ImageBuf::new(&spec).ok()?;
        if !self.deep {
            // Something non-zero, so a bad read is visible against it.
            let values: Vec<f32> = (0..self.channels).map(|c| 0.1 + c as f32 * 0.1).collect();
            algo::fill(&mut image, &values, None).ok()?;
        }
        Some(image)
    }
}

/// Regions that are ordinary, oversized, offset, or channel-shifted.
fn any_region() -> impl Strategy<Value = Option<Roi>> {
    prop_oneof![
        Just(None),
        Just(Roi::new(0..4, 0..4, 0..1, 0..2).ok()),
        Just(Roi::new(-50..50, -50..50, 0..1, 0..4).ok()),
        Just(Roi::new(0..1000, 0..1000, 0..1, 0..3).ok()),
        Just(Roi::new(0..4, 0..4, 0..1, 1..3).ok()),
        Just(Roi::new(0..8, 0..8, 0..1, 0..10_000).ok()),
        Just(Roi::new(-1..1, -1..1, 0..1, 0..1).ok()),
    ]
}

/// Every value of a successful result, or `None` when it holds nothing
/// readable (a deep result has no contiguous pixels).
fn values(image: &ImageBuf) -> Option<Vec<f32>> {
    if image.is_deep() || !image.is_initialized() {
        return None;
    }
    let spec = image.spec().ok()?;
    let roi = spec.data_window().ok()?;
    let count = roi.element_count().ok()?;
    if count == 0 || count > 4_000_000 {
        return None;
    }
    let mut buffer = vec![0.0_f32; count];
    image.get_pixels_into(roi, &mut buffer).ok()?;
    Some(buffer)
}

/// The invariant: an operation that reported success, and that was asked to
/// cover the whole image, must have written the whole image.
///
/// Memory OpenImageIO allocated but never wrote reads back as whatever was
/// there, which is how a partly-written destination shows itself — that is
/// what caught `unpremult` leaving most of its output untouched. The check
/// only applies when the region was `None`: a partial region legitimately
/// leaves the rest of the destination as allocated, and asserting on that
/// would be asserting on the allocator.
///
/// Reaching this function at all is the other half of the test. Every input
/// generated here once had the power to end the process instead.
fn check_result(operation: &str, outcome: oiio::Result<()>, image: &ImageBuf, whole_image: bool) {
    let Ok(()) = outcome else {
        // An error is always an acceptable answer.
        return;
    };
    if !whole_image {
        return;
    }
    let Some(values) = values(image) else { return };
    for (index, value) in values.iter().enumerate() {
        assert!(
            value.is_finite() && value.abs() < IMPLAUSIBLE,
            "{operation} reported success over the whole image but left              {value} at element {index}, which it never wrote"
        );
    }
}

proptest! {
    // Each run explores different shapes, which is the point: this layer is
    // here to find cases nobody thought of. Anything it does find should be
    // pinned as its own test in `soundness_test.rs`, which is where the
    // reproduced cases live, so that the fix has a deterministic guard and
    // this stays free to look elsewhere. `contrib/fuzzing.md` explains why the
    // second layer is proptest rather than cargo-fuzz.
    //
    // Failures are not persisted to a file: proptest wants to write one next
    // to the source, which is neither useful in CI nor wanted in the tree. The
    // failing input is printed, which is what a pinned test is written from.
    #![proptest_config(ProptestConfig {
        cases: 48,
        failure_persistence: None,
        ..ProptestConfig::default()
    })]

    /// The operations that take one source and a region: the largest family,
    /// and the one where a bad region has the most ways in.
    #[test]
    fn single_source_operations_answer_rather_than_misbehave(
        shape in any_shape(),
        region in any_region(),
    ) {
        let Some(source) = shape.build() else { return Ok(()) };
        if std::env::var_os("OIIO_BIND_TRACE").is_some() {
            eprintln!("CASE {shape:?} region {region:?}");
        }

        macro_rules! probe {
            ($name:literal, $call:expr) => {{
                if std::env::var_os("OIIO_BIND_TRACE").is_some() {
                    eprintln!("-> {}", $name);
                }
                let mut dst = ImageBuf::empty().unwrap();
                let outcome = $call(&mut dst);
                check_result($name, outcome, &dst, region.is_none());
            }};
        }

        probe!("flip", |d: &mut ImageBuf| algo::flip(d, &source, region));
        probe!("flop", |d: &mut ImageBuf| algo::flop(d, &source, region));
        probe!("transpose", |d: &mut ImageBuf| algo::transpose(d, &source, region));
        probe!("crop", |d: &mut ImageBuf| algo::crop(d, &source, region));
        probe!("cut", |d: &mut ImageBuf| algo::cut(d, &source, region));
        probe!("rotate_90", |d: &mut ImageBuf| algo::rotate_90(d, &source, region));
        probe!("rotate_180", |d: &mut ImageBuf| algo::rotate_180(d, &source, region));
        probe!("rotate_270", |d: &mut ImageBuf| algo::rotate_270(d, &source, region));
        probe!("reorient", |d: &mut ImageBuf| algo::reorient(d, &source));
        probe!("premult", |d: &mut ImageBuf| algo::premult(d, &source, region));
        probe!("unpremult", |d: &mut ImageBuf| algo::unpremult(d, &source, region));
        probe!("invert", |d: &mut ImageBuf| algo::invert(d, &source, region));
        probe!("abs", |d: &mut ImageBuf| algo::abs(d, &source, region));
        probe!("laplacian", |d: &mut ImageBuf| algo::laplacian(d, &source, region));
        probe!("median_filter", |d: &mut ImageBuf| algo::median_filter(d, &source, 3, None, region));
        probe!("dilate", |d: &mut ImageBuf| algo::dilate(d, &source, 3, None, region));
        probe!("erode", |d: &mut ImageBuf| algo::erode(d, &source, 3, None, region));
        probe!("fft", |d: &mut ImageBuf| algo::fft(d, &source, region));
        probe!("ifft", |d: &mut ImageBuf| algo::ifft(d, &source, region));
        probe!("polar_to_complex", |d: &mut ImageBuf| algo::polar_to_complex(d, &source, region));
        probe!("complex_to_polar", |d: &mut ImageBuf| algo::complex_to_polar(d, &source, region));
        probe!("flatten", |d: &mut ImageBuf| algo::flatten(d, &source, region));
        probe!("deepen", |d: &mut ImageBuf| algo::deepen(d, &source, 1.0, region));
        probe!("resample", |d: &mut ImageBuf| algo::resample(d, &source, true, region));
    }

    /// The measurements, which return rather than fill, and which had three
    /// separate out-of-bounds reads between them.
    #[test]
    fn measurements_answer_rather_than_misbehave(
        shape in any_shape(),
        region in any_region(),
    ) {
        let Some(source) = shape.build() else { return Ok(()) };

        if let Ok(stats) = algo::pixel_stats(&source, region) {
            for (index, value) in stats.min.iter().chain(&stats.max).enumerate() {
                prop_assert!(
                    value.abs() < IMPLAUSIBLE,
                    "pixel_stats reported {value} for channel {index}"
                );
            }
        }
        let _ = algo::histogram(&source, 0, 8, 0.0..1.0, true, region);
        let _ = algo::histogram(&source, 0, 8, 0.0..1.0, false, region);
        if let Ok(Some(colour)) = algo::constant_color(&source, 0.0, region) {
            for value in &colour {
                prop_assert!(value.abs() < IMPLAUSIBLE, "constant_color gave {value}");
            }
        }
        let _ = algo::is_constant_channel(&source, 0, 0.5, 0.0, region);
        let _ = algo::is_monochrome(&source, 0.0, region);
        let _ = algo::nonzero_region(&source, region);
        let _ = algo::pixel_hash_sha1(&source, "", region);
        let _ = algo::compare(&source, &source, 0.0, 0.0, region);
    }

    /// Two sources, where the interesting axis is the pair of channel counts:
    /// several kernels size a buffer from one and fill it from the other.
    #[test]
    fn two_source_operations_answer_rather_than_misbehave(
        a in any_shape(),
        b in any_shape(),
        region in any_region(),
    ) {
        let (Some(first), Some(second)) = (a.build(), b.build()) else { return Ok(()) };

        macro_rules! probe {
            ($name:literal, $call:expr) => {{
                if std::env::var_os("OIIO_BIND_TRACE").is_some() {
                    eprintln!("-> {}", $name);
                }
                let mut dst = ImageBuf::empty().unwrap();
                let outcome = $call(&mut dst);
                check_result($name, outcome, &dst, region.is_none());
            }};
        }

        probe!("add", |d: &mut ImageBuf| algo::add(d, &first, &second, region));
        probe!("sub", |d: &mut ImageBuf| algo::sub(d, &first, &second, region));
        probe!("mul", |d: &mut ImageBuf| algo::mul(d, &first, &second, region));
        probe!("absdiff", |d: &mut ImageBuf| algo::absdiff(d, &first, &second, region));
        probe!("over", |d: &mut ImageBuf| algo::over(d, &first, &second, region));
        probe!("min", |d: &mut ImageBuf| algo::min(d, &first, Operand::Image(&second), region));
        probe!("max", |d: &mut ImageBuf| algo::max(d, &first, Operand::Image(&second), region));
        probe!("mad", |d: &mut ImageBuf| algo::mad(
            d, &first, Operand::Image(&second), Operand::Constant(&[0.5]), region
        ));
        probe!("convolve", |d: &mut ImageBuf| algo::convolve(d, &first, &second, true, region));
        probe!("deep_merge", |d: &mut ImageBuf| algo::deep_merge(d, &first, &second, true, region));
        probe!("deep_holdout", |d: &mut ImageBuf| algo::deep_holdout(d, &first, &second, region));
        {
            // st_warp reads its coordinates from an arbitrary second image, so
            // any output follows from that input rather than from a bad read.
            // Exercise it; do not judge its pixels.
            let mut dst = ImageBuf::empty().unwrap();
            let _ = algo::st_warp(
                &mut dst,
                &first,
                &second,
                [0, 1],
                [false, false],
                &WarpOptions::default(),
                region,
            );
        }
        probe!("paste", |d: &mut ImageBuf| algo::paste(d, [0, 0, 0], 0, &second, region));
    }

    /// A pre-allocated destination is the axis that broke warp, fit, flatten
    /// and unsharp_mask: each sizes something from one buffer and fills it
    /// from the other.
    #[test]
    fn a_mismatched_destination_is_refused_or_handled(
        source_shape in any_shape(),
        dest_shape in any_shape(),
        region in any_region(),
    ) {
        let (Some(source), Some(_)) = (source_shape.build(), dest_shape.build()) else {
            return Ok(())
        };

        macro_rules! probe {
            ($name:literal, $call:expr) => {{
                if std::env::var_os("OIIO_BIND_TRACE").is_some() {
                    eprintln!("-> {} src {:?} dst {:?} region {:?}",
                              $name, source_shape, dest_shape, region);
                }
                let Some(mut dst) = dest_shape.build() else { return Ok(()) };
                let outcome = $call(&mut dst);
                check_result($name, outcome, &dst, region.is_none());
            }};
        }

        let identity = [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0];
        probe!("warp", |d: &mut ImageBuf| algo::warp(
            d, &source, &identity, &WarpOptions::default(), region
        ));
        probe!("rotate", |d: &mut ImageBuf| algo::rotate(
            d, &source, 0.7, None, &WarpOptions::default(), region
        ));
        probe!("fit", |d: &mut ImageBuf| algo::fit(
            d, &source, None, None, FitMode::default(), true, region
        ));
        probe!("resize", |d: &mut ImageBuf| algo::resize(d, &source, None, None, region));
        probe!("flatten", |d: &mut ImageBuf| algo::flatten(d, &source, region));
        probe!("copy", |d: &mut ImageBuf| algo::copy(d, &source, None, region));
        probe!("unsharp_mask", |d: &mut ImageBuf| algo::unsharp_mask(
            d, &source, "gaussian", 3.0, 1.0, 0.0, region
        ));
        probe!("channel_sum", |d: &mut ImageBuf| algo::channel_sum(
            d, &source, &[1.0, 1.0, 1.0], region
        ));
    }

    /// Per-channel constant slices, whose length OpenImageIO pads against a
    /// channel count that is not always the one being indexed.
    #[test]
    fn short_and_long_constant_slices_are_handled(
        shape in any_shape(),
        length in 0_usize..=9,
        region in any_region(),
    ) {
        let Some(source) = shape.build() else { return Ok(()) };
        let constants: Vec<f32> = (0..length).map(|i| 0.1 * i as f32).collect();

        macro_rules! probe {
            ($name:literal, $call:expr) => {{
                if std::env::var_os("OIIO_BIND_TRACE").is_some() {
                    eprintln!("-> {}", $name);
                }
                let mut dst = ImageBuf::empty().unwrap();
                let outcome = $call(&mut dst);
                check_result($name, outcome, &dst, region.is_none());
            }};
        }

        probe!("pow", |d: &mut ImageBuf| algo::pow(d, &source, &constants, region));
        probe!("clamp", |d: &mut ImageBuf| algo::clamp(
            d, &source, &constants, &constants, true, region
        ));
        probe!("channel_sum", |d: &mut ImageBuf| algo::channel_sum(d, &source, &constants, region));
        probe!("min", |d: &mut ImageBuf| algo::min(
            d, &source, Operand::Constant(&constants), region
        ));
        probe!("max", |d: &mut ImageBuf| algo::max(
            d, &source, Operand::Constant(&constants), region
        ));
        probe!("mad", |d: &mut ImageBuf| algo::mad(
            d, &source, Operand::Constant(&constants), Operand::Constant(&constants), region
        ));
        probe!("saturate", |d: &mut ImageBuf| algo::saturate(d, &source, 0.5, 0, region));

        if !constants.is_empty() {
            probe!("fill", |d: &mut ImageBuf| {
                let _ = algo::copy(d, &source, None, None);
                algo::fill(d, &constants, region)
            });
        }
    }

    /// Deep sample access, where the coordinate and index checks live in Rust
    /// because OpenImageIO's answer to a bad one is a null pointer.
    #[test]
    fn deep_accessors_reject_what_they_cannot_reach(
        x in -20_i32..20,
        y in -20_i32..20,
        channel in 0_u32..10,
        sample in 0_u32..10,
        count in 0_u32..6,
    ) {
        let spec = ImageSpec::new(4, 4, 3, PixelFormat::F32).unwrap().as_deep();
        let mut image = ImageBuf::new(&spec).unwrap();

        let inside = (0..4).contains(&x) && (0..4).contains(&y);
        let set = image.set_deep_sample_count(x, y, count);
        prop_assert_eq!(
            set.is_ok(),
            inside,
            "a coordinate is accepted exactly when it is inside the image"
        );

        // Whatever happened, reading agrees with writing about what exists.
        let held = image.deep_sample_count(x, y);
        if inside {
            prop_assert_eq!(held, count);
        } else {
            prop_assert_eq!(held, 0);
        }

        let value = image.deep_value(x, y, channel, sample);
        let reachable = inside && channel < 3 && sample < held;
        prop_assert_eq!(value.is_ok(), reachable);
    }
}
