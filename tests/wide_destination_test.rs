//! No operation may write a channel the source does not have.
//!
//! OpenImageIO's kernels take their channel range from the destination and
//! index the source with it unchecked, so an operation handed a destination
//! wider than its source reads past each source pixel and, at the end of the
//! buffer, past the allocation. The class has produced defects in `mad`,
//! `copy`, `channel_sum`, `circular_shift` and, through the no-alpha copy
//! fallback, `premult` and `unpremult`. Several are filed upstream, the copy
//! one as
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
//! The invariant needs no reference implementation. Give the source values
//! that are all distinct and all non-zero, hand the operation a pre-allocated
//! destination with more channels than the source, and require that every
//! channel the source lacks comes back either untouched or zeroed. A
//! constant-filled source would hide the defect, because an over-read into the
//! next pixel would return the same constant; the ramp is what makes it
//! visible.
//!
//! An operation is free to refuse. `Err` is a pass: the crate's guards exist
//! precisely to refuse these shapes, and this file pins that they either
//! refuse or answer correctly, never answer wrongly.
//!
//! **On pixel formats.** Only the float formats are probed, which still gives
//! three different strides, 2, 4 and 8 bytes, and therefore three different
//! typed kernels. The integer formats are deliberately left out: `set_pixel_at`
//! normalises a float into them, so a ramp of 1 through 32 would clamp to the
//! same maximum in every channel and lose the distinctness the whole oracle
//! rests on, and `-1` cannot be stored at all so an untouched channel would be
//! indistinguishable from a deliberately zeroed one. Testing them with a
//! blunted oracle would read as coverage without being any.

mod common;

use oiio::algo::Operand;
use oiio::{algo, ImageBuf, ImageSpec, PixelFormat, Roi};

/// Left in the destination's extra channels. Negative, so it cannot be
/// confused with a source value or with a deliberate zero. Exactly
/// representable in all three probed formats.
const SENTINEL: f32 = -1.0;

/// One shape to ask the invariant of.
///
/// The shape is parameterised because the first version of this file probed
/// exactly one, F32 at the origin with 2 channels against 6, and that one shape
/// was enough to find the `mad` defect. A shape that has not been probed is an
/// unknown one, not a safe one: the pixel format decides the stride and which
/// typed kernel runs, and the data windows decide whether the region guards
/// refuse before the kernel is reached at all.
#[derive(Clone, Copy)]
struct Shape {
    name: &'static str,
    width: u32,
    height: u32,
    src_channels: u32,
    dst_channels: u32,
    src_format: PixelFormat,
    dst_format: PixelFormat,
    src_origin: [i32; 3],
    dst_origin: [i32; 3],
}

impl Shape {
    const fn base(name: &'static str) -> Self {
        Self {
            name,
            width: 4,
            height: 4,
            src_channels: 2,
            dst_channels: 6,
            src_format: PixelFormat::F32,
            dst_format: PixelFormat::F32,
            src_origin: [0, 0, 0],
            dst_origin: [0, 0, 0],
        }
    }

    /// A source whose every value is distinct and non-zero: channel `c` of
    /// pixel `(x, y)` holds `1 + c + src_channels * (y * width + x)`.
    ///
    /// Distinctness is the whole mechanism, because it is what identifies a
    /// returned value as having come from a neighbouring pixel. Non-zero
    /// matters too: zero is an allowed answer in an extra channel, so a source
    /// containing zeroes would make a real over-read indistinguishable from a
    /// deliberate fill.
    fn source(&self) -> ImageBuf {
        let spec = ImageSpec::new(self.width, self.height, self.src_channels, self.src_format)
            .unwrap()
            .with_origin(self.src_origin);
        let mut image = ImageBuf::new(&spec).unwrap();
        algo::zero(&mut image, None).unwrap();
        let [origin_x, origin_y, _] = self.src_origin;
        for row in 0..self.height as i32 {
            for column in 0..self.width as i32 {
                let base = 1 + self.src_channels as i32 * (row * self.width as i32 + column);
                let values: Vec<f32> = (0..self.src_channels)
                    .map(|c| (base + c as i32) as f32)
                    .collect();
                image
                    .set_pixel_at(origin_x + column, origin_y + row, &values)
                    .unwrap();
            }
        }
        image
    }

    fn destination(&self) -> ImageBuf {
        let spec = ImageSpec::new(self.width, self.height, self.dst_channels, self.dst_format)
            .unwrap()
            .with_origin(self.dst_origin);
        let mut image = ImageBuf::new(&spec).unwrap();
        let fill = vec![SENTINEL; self.dst_channels as usize];
        algo::fill(&mut image, &fill, None).unwrap();
        image
    }

    /// Every value the source holds, for deciding whether a stray value was
    /// read out of it or out of whatever followed it in memory.
    fn source_values(&self) -> Vec<f32> {
        (1..=(self.src_channels * self.width * self.height) as i32)
            .map(|v| v as f32)
            .collect()
    }
}

/// The shapes probed. Each line is one question; adding a line is the whole
/// cost of covering a new shape.
fn shapes() -> Vec<Shape> {
    vec![
        // The shape that found `mad`.
        Shape::base("f32 2->6"),
        // Narrower and wider strides, which select different typed kernels.
        Shape {
            src_format: PixelFormat::F16,
            dst_format: PixelFormat::F16,
            ..Shape::base("f16 2->6")
        },
        Shape {
            src_format: PixelFormat::F64,
            dst_format: PixelFormat::F64,
            ..Shape::base("f64 2->6")
        },
        // Mismatched strides in both directions, which is what sends `copy`
        // down its conversion path rather than its copy path.
        Shape {
            src_format: PixelFormat::F16,
            dst_format: PixelFormat::F32,
            ..Shape::base("f16 src -> f32 dst")
        },
        Shape {
            src_format: PixelFormat::F32,
            dst_format: PixelFormat::F16,
            ..Shape::base("f32 src -> f16 dst")
        },
        // One channel against five is the `premult` and `copy` shape.
        Shape {
            src_channels: 1,
            dst_channels: 5,
            ..Shape::base("f32 1->5")
        },
        // A wide gap, so an over-read runs further past each pixel.
        Shape {
            src_channels: 1,
            dst_channels: 8,
            ..Shape::base("f32 1->8")
        },
        // A single channel of headroom, the alpha-shaped case.
        Shape {
            src_channels: 3,
            dst_channels: 4,
            ..Shape::base("f32 3->4")
        },
        // Non-square, so a row-stride error cannot hide behind a square.
        Shape {
            width: 7,
            height: 3,
            ..Shape::base("f32 7x3 2->6")
        },
        // Data windows away from the origin, separately and together. The
        // region guards are what should decide these, and a refusal is a pass.
        Shape {
            dst_origin: [500, 500, 0],
            ..Shape::base("destination at 500,500")
        },
        Shape {
            src_origin: [500, 500, 0],
            ..Shape::base("source at 500,500")
        },
        Shape {
            src_origin: [-250, -250, 0],
            ..Shape::base("source at -250,-250, as an overscan render")
        },
        // The direction of the original CI sighting: a destination far away
        // from its source. `refuse_distant_pair` should stop most operations
        // here, which is exactly what this asks.
        Shape {
            dst_origin: [1_000_000, 1_000_000, 0],
            ..Shape::base("destination a million pixels away")
        },
    ]
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
fn channels_the_source_lacks(shape: &Shape, dst: &ImageBuf) -> Vec<Violation> {
    let known = shape.source_values();
    let mut found = Vec::new();

    let Ok(spec) = dst.spec() else {
        return found;
    };
    let channels = spec.channel_count();
    // The operation replaced the destination with one no wider than the
    // source, so it kept none of our allocation and there is no extra channel
    // to inspect. Nothing to assert.
    if channels <= shape.src_channels {
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
                .skip(shape.src_channels as usize)
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

fn report(
    shape: &Shape,
    operation: &str,
    outcome: Result<(), oiio::Error>,
    dst: &ImageBuf,
) -> Vec<String> {
    match outcome {
        // A refusal is a pass. The guards exist to produce exactly this.
        Err(_) => Vec::new(),
        Ok(()) => channels_the_source_lacks(shape, dst)
            .into_iter()
            .map(|v| {
                let origin = if v.from_the_source {
                    "a value from the source, so read past its pixel"
                } else {
                    "not a value in the source, so read past its allocation"
                };
                format!(
                    "[{}] {operation}: ({},{}) channel {} came back {}; the source \
                     has {} channels, and this is {origin}",
                    shape.name, v.x, v.y, v.channel, v.value, shape.src_channels
                )
            })
            .collect(),
    }
}

/// The operations sharing `fn(&mut ImageBuf, &ImageBuf, Option<Roi>)`.
///
/// Two are deliberately absent.
///
/// `flatten` needs a deep source, which cannot be handed a wider flat
/// destination in the first place.
///
/// `fft` cannot be asked this question at all, because its output channel
/// count is defined by the transform rather than inherited from the
/// destination: it always produces a two-channel complex image, real part then
/// imaginary, whatever the destination held before. Against a one-channel
/// source it therefore writes channel 1 legitimately, and the widened shapes
/// below caught it doing so and reported it as an over-read. The coefficients
/// even looked the part, coming back as 2, -2, 8 and -8 where 2 and 8 are
/// genuinely values in the source. That was a flaw in this file's oracle, not
/// a defect in the crate, and it is worth recording because it is the one
/// shape of false positive this invariant can produce: an operation that
/// resets the destination's channel count.
///
/// `ifft` and the polar pair stay, because they require two channels on both
/// sides and so refuse the narrow shapes rather than resetting them, and a
/// refusal is a pass.
#[allow(clippy::type_complexity)]
fn uniform_operations() -> Vec<(
    &'static str,
    fn(&mut ImageBuf, &ImageBuf, Option<Roi>) -> Result<(), oiio::Error>,
)> {
    vec![
        ("laplacian", algo::laplacian),
        // OpenImageIO implements invert as mad(A, -1.0, 1.0), so it reaches
        // the kernel the four mad shims guard. imagebufalgo_invert calls the
        // library directly rather than routing through imagebufalgo_mad_icc,
        // so it does not inherit that guard: the same bypass shape as the
        // no-alpha copy fallback in premult.
        ("invert", algo::invert),
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

/// Asks every operation the invariant's question, for one shape.
fn probe(shape: &Shape) -> Vec<String> {
    let src = shape.source();
    let mut complaints = Vec::new();

    for (name, operation) in uniform_operations() {
        let mut dst = shape.destination();
        let outcome = operation(&mut dst, &src, None);
        complaints.extend(report(shape, name, outcome, &dst));
    }

    // The operations whose signatures differ, listed individually. Each of
    // these has produced a defect of this class, here or upstream.
    {
        let mut dst = shape.destination();
        let outcome = algo::copy(&mut dst, &src, None, None);
        complaints.extend(report(shape, "copy", outcome, &dst));
    }
    {
        let mut dst = shape.destination();
        let outcome = algo::circular_shift(&mut dst, &src, [0, 0, 0], None);
        complaints.extend(report(shape, "circular_shift", outcome, &dst));
    }
    {
        let mut dst = shape.destination();
        let outcome = algo::paste(&mut dst, [0, 0, 0], 0, &src, None);
        complaints.extend(report(shape, "paste", outcome, &dst));
    }
    // All four `mad` operand combinations, because the shim is four separate
    // functions and only the ones taking two images shared a guard. The
    // image-and-constant forms reach the same kernel.
    let constant = vec![2.0_f32; shape.src_channels as usize];
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
        let mut dst = shape.destination();
        let outcome = algo::mad(&mut dst, &src, b, c, None);
        complaints.extend(report(shape, label, outcome, &dst));
    }
    {
        // zover shares `mad`'s source-against-source guard, so it is worth
        // asking the same question of it. It may refuse for wanting a Z
        // channel, which is a pass.
        let mut dst = shape.destination();
        let outcome = algo::zover(&mut dst, &src, &src, false, None);
        complaints.extend(report(shape, "zover", outcome, &dst));
    }
    {
        let mut dst = shape.destination();
        let weights = vec![1.0_f32; shape.src_channels as usize];
        let outcome = algo::channel_sum(&mut dst, &src, &weights, None);
        complaints.extend(report(shape, "channel_sum", outcome, &dst));
    }
    {
        let mut dst = shape.destination();
        let outcome = algo::add(&mut dst, &src, &src, None);
        complaints.extend(report(shape, "add", outcome, &dst));
    }
    {
        // Empty min and max are -FLT_MAX and +FLT_MAX, so the clamp is a pass
        // through and anything read past the source pixel lands verbatim.
        // clamp_alpha_to_unit stays false, because it writes the alpha channel
        // separately and would muddy the mechanism.
        let mut dst = shape.destination();
        let outcome = algo::clamp(&mut dst, &src, &[], &[], false, None);
        complaints.extend(report(shape, "clamp", outcome, &dst));
    }
    {
        // Every slice empty is the documented identity remap, so this too
        // writes the value it read and nothing else.
        let mut dst = shape.destination();
        let outcome = algo::contrast_remap(&mut dst, &src, &Default::default(), None);
        complaints.extend(report(shape, "contrast_remap", outcome, &dst));
    }
    {
        // st_warp needs its coordinate map in 0..1, so it cannot be the ramp:
        // the ramp's values clamp to the window edge and the sampler then
        // reads outside the data window, where a buffer answers black from its
        // own scratch rather than from a source pixel, which would mask the
        // defect rather than show it. A constant half samples the middle.
        let coordinates = {
            let spec = ImageSpec::new(shape.width, shape.height, 2, PixelFormat::F32)
                .unwrap()
                .with_origin(shape.src_origin);
            let mut map = ImageBuf::new(&spec).unwrap();
            algo::fill(&mut map, &[0.5, 0.5], None).unwrap();
            map
        };
        let mut dst = shape.destination();
        let outcome = algo::st_warp(
            &mut dst,
            &src,
            &coordinates,
            [0, 1],
            [false, false],
            &Default::default(),
            None,
        );
        complaints.extend(report(shape, "st_warp", outcome, &dst));
    }

    // An EXPLICIT region whose channel range spans the destination. Everything
    // above passes None, and the two are not equivalent: IBAprep shrink-wraps
    // a defined ROI to the DESTINATION rather than to the source, and `flip`
    // copies the region's channel range straight through where `transpose`
    // clamps it to the source. So a defined chend is its own question.
    if let Ok(region) = Roi::new(
        shape.src_origin[0]..shape.src_origin[0] + shape.width as i32,
        shape.src_origin[1]..shape.src_origin[1] + shape.height as i32,
        0..1,
        0..shape.dst_channels,
    ) {
        for (name, operation) in uniform_operations() {
            let mut dst = shape.destination();
            let outcome = operation(&mut dst, &src, Some(region));
            complaints.extend(report(
                shape,
                &format!("{name} with an explicit full-width region"),
                outcome,
                &dst,
            ));
        }
        {
            let mut dst = shape.destination();
            let outcome = algo::copy(&mut dst, &src, None, Some(region));
            complaints.extend(report(
                shape,
                "copy with an explicit full-width region",
                outcome,
                &dst,
            ));
        }
    }

    complaints
}

#[test]
fn no_operation_writes_a_channel_the_source_lacks() {
    let mut complaints = Vec::new();
    for shape in shapes() {
        complaints.extend(probe(&shape));
    }
    assert!(
        complaints.is_empty(),
        "{} case(s) wrote a channel the source does not have:\n  {}",
        complaints.len(),
        complaints.join("\n  ")
    );
}

/// The ramp is load-bearing: this pins that every probed shape really does
/// build a source of distinct non-zero values, so a passing run above means
/// something. A shape whose fixture collapsed would pass the invariant for the
/// wrong reason.
#[test]
fn every_shape_makes_an_over_read_visible() {
    for shape in shapes() {
        let src = shape.source();
        let [origin_x, origin_y, _] = shape.src_origin;
        let mut seen: Vec<f32> = Vec::new();
        for row in 0..shape.height as i32 {
            for column in 0..shape.width as i32 {
                let x = origin_x + column;
                let y = origin_y + row;
                let roi = Roi::new(x..x + 1, y..y + 1, 0..1, 0..shape.src_channels).unwrap();
                let mut pixel = vec![0.0_f32; shape.src_channels as usize];
                src.get_pixels_into(roi, &mut pixel).unwrap();
                for &value in &pixel {
                    assert_ne!(
                        value, 0.0,
                        "[{}] a zero in the source would hide an over-read",
                        shape.name
                    );
                    assert_ne!(
                        value, SENTINEL,
                        "[{}] a sentinel value in the source would hide an over-read",
                        shape.name
                    );
                    assert!(
                        !seen.contains(&value),
                        "[{}] {value} appears twice in the source; distinct values \
                         are what make the over-read identifiable",
                        shape.name
                    );
                    seen.push(value);
                }
            }
        }
        assert_eq!(
            seen.len(),
            (shape.src_channels * shape.width * shape.height) as usize,
            "[{}] every source value should have been visited exactly once",
            shape.name
        );
        assert_eq!(
            seen.len(),
            shape.source_values().len(),
            "[{}] source_values() must agree with the ramp it describes",
            shape.name
        );
    }
}
