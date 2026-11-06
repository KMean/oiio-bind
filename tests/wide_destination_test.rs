//! No operation may write a channel the source does not have.
//!
//! OpenImageIO's kernels take their channel range from the destination and
//! index the source with it unchecked, so an operation handed a destination
//! wider than its source reads past each source pixel and, at the end of the
//! buffer, past the allocation. The class has produced defects in `mad`,
//! `copy`, `channel_sum`, `circular_shift` and, through the no-alpha copy
//! fallback, `premult` and `unpremult`. Three are filed upstream, the last as
//! [#5661](https://github.com/AcademySoftwareFoundation/OpenImageIO/issues/5661).
//!
//! **Why this file exists rather than relying on the suites that already run.**
//! Neither of the other two mechanisms can see this class:
//!
//! - `property_test` asks whether a value is plausible, `is_finite()` and
//!   `abs() < 1e30`. An over-read lands on the source's *next pixel*, so it
//!   returns a small finite number and passes. The `premult` defect produced
//!   `0 1 2 3 4` for a one-channel source, every value plausible.
//! - `differential_algo_test` compares the safe layer against the raw sys
//!   bridge, and both go through the same C++ shim, as its own header says. A
//!   defect inside a shim is invisible to it, and that is where `premult`'s
//!   was.
//!
//! The invariant here needs no reference implementation. Give the source
//! values that are all distinct and all non-zero, hand the operation a
//! pre-allocated destination with more channels than the source, and require
//! that every channel the source lacks comes back either untouched or zeroed.
//! A constant-filled source would hide the defect, because an over-read into
//! the next pixel would return the same constant; the ramp is what makes it
//! visible.
//!
//! An operation is free to refuse. `Err` is a pass: the crate's guards exist
//! precisely to refuse these shapes, and this file pins that they either
//! refuse or answer correctly, never answer wrongly.

mod common;

use oiio::algo::Operand;
use oiio::{algo, ImageBuf, ImageSpec, PixelFormat, Roi};

const WIDTH: u32 = 4;
const HEIGHT: u32 = 4;
/// Channels the source has.
const SRC_CHANNELS: u32 = 2;
/// Channels the destination has. The gap is where the over-read lands.
const DST_CHANNELS: u32 = 6;
/// Left in the destination's extra channels. Negative, so it cannot be
/// confused with a source value or with a deliberate zero.
const SENTINEL: f32 = -1.0;

/// A source whose every value is distinct and non-zero: channel `c` of pixel
/// `(x, y)` holds `1 + c + SRC_CHANNELS * (y * WIDTH + x)`, so 1 through 32.
///
/// Distinctness is the whole mechanism. Non-zero matters too, because zero is
/// an allowed answer in an extra channel and a source full of zeroes would
/// make a real over-read indistinguishable from a deliberate fill.
fn ramp_source() -> ImageBuf {
    let spec = ImageSpec::new(WIDTH, HEIGHT, SRC_CHANNELS, PixelFormat::F32).unwrap();
    let mut image = ImageBuf::new(&spec).unwrap();
    algo::zero(&mut image, None).unwrap();
    for y in 0..HEIGHT as i32 {
        for x in 0..WIDTH as i32 {
            let base = 1 + SRC_CHANNELS as i32 * (y * WIDTH as i32 + x);
            let values: Vec<f32> = (0..SRC_CHANNELS)
                .map(|c| (base + c as i32) as f32)
                .collect();
            image.set_pixel_at(x, y, &values).unwrap();
        }
    }
    image
}

fn wide_destination() -> ImageBuf {
    let spec = ImageSpec::new(WIDTH, HEIGHT, DST_CHANNELS, PixelFormat::F32).unwrap();
    let mut image = ImageBuf::new(&spec).unwrap();
    algo::fill(&mut image, &[SENTINEL; DST_CHANNELS as usize], None).unwrap();
    image
}

/// Every value the source holds, for deciding whether a stray value was read
/// out of it or out of whatever followed it in memory.
fn source_values() -> Vec<f32> {
    (1..=(SRC_CHANNELS * WIDTH * HEIGHT) as i32)
        .map(|v| v as f32)
        .collect()
}

/// Describes one channel that should not have been written.
struct Violation {
    x: i32,
    y: i32,
    channel: u32,
    value: f32,
    from_the_source: bool,
}

/// Checks the channels the source does not have. Collects every violation
/// rather than stopping at the first, so one run reports the whole picture.
///
/// The destination is read through its own spec rather than the shape it was
/// built with, because several of these operations reshape it: `transpose` and
/// the odd rotations swap the axes, `crop` and `cut` move the window, and an
/// operation is free to reset it outright. A fixed region would read out of
/// range and panic, which says nothing about the invariant.
fn channels_the_source_lacks(dst: &ImageBuf) -> Vec<Violation> {
    let known = source_values();
    let mut found = Vec::new();

    let Ok(spec) = dst.spec() else {
        return found;
    };
    let channels = spec.channel_count();
    // The operation replaced the destination with one no wider than the
    // source, so it kept none of our allocation and there is no extra channel
    // to inspect. Nothing to assert.
    if channels <= SRC_CHANNELS {
        return found;
    }
    let Ok(window) = spec.data_window() else {
        return found;
    };

    for y in window.y() {
        for x in window.x() {
            let roi = Roi::new(x..x + 1, y..y + 1, 0..1, 0..channels).unwrap();
            let mut pixel = vec![0.0_f32; channels as usize];
            if dst.get_pixels_into(roi, &mut pixel).is_err() {
                continue;
            }
            for (channel, &value) in pixel
                .iter()
                .enumerate()
                .skip(SRC_CHANNELS as usize)
                .map(|(c, v)| (c as u32, v))
            {
                // Untouched, or deliberately zeroed. Both are defensible.
                if value == SENTINEL || value == 0.0 {
                    continue;
                }
                found.push(Violation {
                    x,
                    y,
                    channel,
                    value,
                    from_the_source: known.contains(&value),
                });
            }
        }
    }
    found
}

fn report(operation: &str, outcome: Result<(), oiio::Error>, dst: &ImageBuf) -> Vec<String> {
    match outcome {
        // A refusal is a pass. The guards exist to produce exactly this.
        Err(_) => Vec::new(),
        Ok(()) => channels_the_source_lacks(dst)
            .into_iter()
            .map(|v| {
                let origin = if v.from_the_source {
                    "a value from the source, so read past its pixel"
                } else {
                    "not a value in the source, so read past its allocation"
                };
                format!(
                    "{operation}: ({},{}) channel {} came back {}; the source has \
                     {SRC_CHANNELS} channels, and this is {origin}",
                    v.x, v.y, v.channel, v.value
                )
            })
            .collect(),
    }
}

/// The operations sharing `fn(&mut ImageBuf, &ImageBuf, Option<Roi>)`.
/// `flatten` is excluded because it needs a deep source, which cannot be
/// handed a wider flat destination in the first place.
#[allow(clippy::type_complexity)]
fn uniform_operations() -> Vec<(
    &'static str,
    fn(&mut ImageBuf, &ImageBuf, Option<Roi>) -> Result<(), oiio::Error>,
)> {
    vec![
        ("laplacian", algo::laplacian),
        ("fft", algo::fft),
        ("ifft", algo::ifft),
        ("polar_to_complex", algo::polar_to_complex),
        ("complex_to_polar", algo::complex_to_polar),
        ("cut", algo::cut),
        ("crop", algo::crop),
        ("flip", algo::flip),
        ("flop", algo::flop),
        ("transpose", algo::transpose),
        ("premult", algo::premult),
        ("unpremult", algo::unpremult),
        ("repremult", algo::repremult),
        ("maxchan", algo::maxchan),
        ("minchan", algo::minchan),
        ("rotate_90", algo::rotate_90),
        ("rotate_180", algo::rotate_180),
        ("rotate_270", algo::rotate_270),
    ]
}

#[test]
fn no_operation_writes_a_channel_the_source_lacks() {
    let src = ramp_source();
    let mut complaints = Vec::new();

    for (name, operation) in uniform_operations() {
        let mut dst = wide_destination();
        let outcome = operation(&mut dst, &src, None);
        complaints.extend(report(name, outcome, &dst));
    }

    // The operations whose signatures differ, listed individually. Each of
    // these has produced a defect of this class in OpenImageIO.
    {
        let mut dst = wide_destination();
        let outcome = algo::copy(&mut dst, &src, None, None);
        complaints.extend(report("copy", outcome, &dst));
    }
    {
        let mut dst = wide_destination();
        let outcome = algo::circular_shift(&mut dst, &src, [0, 0, 0], None);
        complaints.extend(report("circular_shift", outcome, &dst));
    }
    {
        let mut dst = wide_destination();
        let outcome = algo::paste(&mut dst, [0, 0, 0], 0, &src, None);
        complaints.extend(report("paste", outcome, &dst));
    }
    // All four `mad` operand combinations, because the shim is four separate
    // functions and only the ones taking two images shared a guard. The
    // image-and-constant forms reach the same kernel.
    let constant = vec![2.0_f32; SRC_CHANNELS as usize];
    for (label, b, c) in [
        (
            "mad(image, image)",
            Operand::Image(&src),
            Operand::Image(&src),
        ),
        (
            "mad(image, constant)",
            Operand::Image(&src),
            Operand::Constant(&constant),
        ),
        (
            "mad(constant, image)",
            Operand::Constant(&constant),
            Operand::Image(&src),
        ),
        (
            "mad(constant, constant)",
            Operand::Constant(&constant),
            Operand::Constant(&constant),
        ),
    ] {
        let mut dst = wide_destination();
        let outcome = algo::mad(&mut dst, &src, b, c, None);
        complaints.extend(report(label, outcome, &dst));
    }
    {
        // zover shares `mad`'s source-against-source guard, so it is worth
        // asking the same question of it. It may refuse for wanting a Z
        // channel, which is a pass.
        let mut dst = wide_destination();
        let outcome = algo::zover(&mut dst, &src, &src, false, None);
        complaints.extend(report("zover", outcome, &dst));
    }
    {
        let mut dst = wide_destination();
        let weights = vec![1.0_f32; SRC_CHANNELS as usize];
        let outcome = algo::channel_sum(&mut dst, &src, &weights, None);
        complaints.extend(report("channel_sum", outcome, &dst));
    }
    {
        let mut dst = wide_destination();
        let outcome = algo::add(&mut dst, &src, &src, None);
        complaints.extend(report("add", outcome, &dst));
    }

    assert!(
        complaints.is_empty(),
        "{} operation(s) wrote a channel the source does not have:\n  {}",
        complaints.len(),
        complaints.join("\n  ")
    );
}

/// The ramp is load-bearing: this pins that the fixture really does hold
/// distinct non-zero values, so a passing run above means something.
#[test]
fn the_fixture_makes_an_over_read_visible() {
    let src = ramp_source();
    let mut seen = Vec::new();
    for y in 0..HEIGHT as i32 {
        for x in 0..WIDTH as i32 {
            let roi = Roi::new(x..x + 1, y..y + 1, 0..1, 0..SRC_CHANNELS).unwrap();
            let mut pixel = [0.0_f32; SRC_CHANNELS as usize];
            src.get_pixels_into(roi, &mut pixel).unwrap();
            for &value in &pixel {
                assert_ne!(value, 0.0, "a zero in the source would hide an over-read");
                assert_ne!(
                    value, SENTINEL,
                    "a sentinel value in the source would hide an over-read"
                );
                assert!(
                    !seen.contains(&value),
                    "{value} appears twice in the source; distinct values are what \
                     make the over-read identifiable"
                );
                seen.push(value);
            }
        }
    }
    assert_eq!(
        seen.len(),
        (SRC_CHANNELS * WIDTH * HEIGHT) as usize,
        "every source value should have been visited exactly once"
    );
    assert_eq!(
        seen.len(),
        source_values().len(),
        "source_values() must agree with the ramp it describes"
    );
}
