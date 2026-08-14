//! Operations on [`ImageBuf`], mirroring OpenImageIO's `ImageBufAlgo`.
//!
//! Most operations write into a destination buffer and take an optional
//! region; `None` means the whole image, which is OpenImageIO's own default.
//! Because the destination is `&mut` and the sources are `&`, Rust rejects at
//! compile time the aliasing cases these functions are not written for.
//!
//! The measurements ([`pixel_stats`], [`histogram`], [`constant_color`],
//! [`is_constant_channel`], [`is_monochrome`], [`nonzero_region`],
//! [`pixel_hash_sha1`] and [`compare`]) return what they found rather than
//! filling a buffer, [`make_kernel`] and [`text_size`] return a new value, and
//! [`reorient`] takes no region because it always works on the whole image.
//!
//! # Which region, whose
//!
//! A region normally names part of the **destination**. Seven operations read
//! it as part of the **source** instead: [`paste`], the three right-angle
//! rotations, and [`flip`], [`flop`] and [`transpose`], each of which derives
//! where the result lands from where the region sat. This list is the
//! authority — the parameter is spelled `src_roi` on `paste` and the
//! rotations, while `flip`, `flop` and `transpose` inherited OpenImageIO's
//! plain `roi` name and read it as source all the same.
//!
//! ```no_run
//! use oiio::{algo, ImageBuf, ImageSpec, PixelFormat};
//!
//! # fn main() -> oiio::Result<()> {
//! let spec = ImageSpec::new(64, 64, 3, PixelFormat::F32)?;
//! let mut background = ImageBuf::new(&spec)?;
//! algo::fill(&mut background, &[0.2, 0.4, 0.6], None)?;
//!
//! let foreground = ImageBuf::new(&spec)?;
//! let mut combined = ImageBuf::new(&spec)?;
//! algo::add(&mut combined, &background, &foreground, None)?;
//! # Ok(())
//! # }
//! ```

use crate::{sys, Error, ImageBuf, Result, Roi};

pub use sys::imagebufalgo::CompareSummary;

/// Use every thread OpenImageIO is configured for.
const ALL_THREADS: i32 = 0;

// SAFETY, for every `unsafe` block in this module.
//
// The `imagebufalgo_*` shims are declared `unsafe fn` because they hand their
// arguments to OpenImageIO unchecked, and OpenImageIO trusts them: a region
// whose channel range starts past the destination's last channel comes back
// inverted out of `IBAprep`'s intersection and is then used as an unsigned
// length. See `region_in`, which is what makes these calls sound and which
// every one of them goes through. The other arguments are plain values, slices
// whose lengths the shims re-derive, and `ImageBuf` references that cannot be
// null because they come from `&`/`&mut` here.

fn region(roi: Option<Roi>) -> sys::imageio::ROI {
    // An undefined ROI is how OpenImageIO spells "the whole image".
    roi.map_or_else(sys::imageio::roi_default, Roi::to_sys)
}

/// A region validated against the image the operation will write into.
///
/// `IBAprep` starts every operation with
/// `roi = roi_intersection(roi, get_roi(dst->spec()))`, and `roi_intersection`
/// takes the larger begin and the smaller end. A channel range that starts past
/// the destination's last channel therefore comes back INVERTED -- 5..8 against
/// 0..3 gives chbegin 5, chend 3 -- and `ROI::nchannels()` is then -2. The
/// kernels turn that straight into an unsigned length: `zero` reaches
/// `memcpy(.., nchannels * sizeof(T))` with `(size_t)-8`, from an address
/// already five floats into a three float pixel.
///
/// The rule is the one that cannot invert: the channel range must exist in the
/// destination when the destination is already allocated, and must start at
/// channel zero when it is not, since `IBAprep` then builds the destination
/// from this very region and intersects against what it just built.
fn region_in(roi: Option<Roi>, dst: &ImageBuf) -> Result<sys::imageio::ROI> {
    if let Some(roi) = roi {
        let channels = roi.channels();
        let available = dst.channel_count();
        if dst.is_initialized() && available >= 0 {
            if channels.start >= available.unsigned_abs() {
                return Err(Error::InvalidRoi(format!(
                    "the region starts at channel {} and the destination has {available}",
                    channels.start
                )));
            }
        } else if channels.start != 0 {
            return Err(Error::InvalidRoi(format!(
                "the region starts at channel {}; a destination that is not                  allocated yet is built from the region itself, so the region                  must begin at channel zero",
                channels.start
            )));
        }
    }
    Ok(region(roi))
}

fn finish(dst: &mut ImageBuf, operation: &'static str, succeeded: bool) -> Result<()> {
    if succeeded {
        Ok(())
    } else {
        Err(Error::operation(operation, dst.take_error()))
    }
}

/// Set every channel in the region to zero.
pub fn zero(dst: &mut ImageBuf, roi: Option<Roi>) -> Result<()> {
    let roi = region_in(roi, dst)?;
    let succeeded =
        unsafe { sys::imagebufalgo::imagebufalgo_zero(dst.inner_mut(), &roi, ALL_THREADS) };
    finish(dst, "zero", succeeded)
}

/// Fill the region with one value per channel.
///
/// Fewer values than the image has channels repeats the last one across the
/// rest — so filling an RGBA image with three values sets alpha to the blue
/// value, not to nothing. Narrow the region's channel range to fill only some
/// channels.
pub fn fill(dst: &mut ImageBuf, values: &[f32], roi: Option<Roi>) -> Result<()> {
    if values.is_empty() {
        return Err(Error::InvalidImageSpec(
            "fill needs at least one channel value".to_owned(),
        ));
    }
    let roi = region_in(roi, dst)?;
    let succeeded =
        unsafe { sys::imagebufalgo::imagebufalgo_fill(dst.inner_mut(), values, &roi, ALL_THREADS) };
    finish(dst, "fill", succeeded)
}

macro_rules! binary_operation {
    ($name:ident, $constant_name:ident, $images:path, $constants:path, $label:literal) => {
        #[doc = concat!("Compute `a ", $label, " b` into `dst`.")]
        pub fn $name(
            dst: &mut ImageBuf,
            a: &ImageBuf,
            b: &ImageBuf,
            roi: Option<Roi>,
        ) -> Result<()> {
            let roi = region_in(roi, dst)?;
            let succeeded =
                unsafe { $images(dst.inner_mut(), a.inner(), b.inner(), &roi, ALL_THREADS) };
            finish(dst, $label, succeeded)
        }

        #[doc = concat!("Compute `a ", $label, " values` into `dst`, one value per channel.")]
        pub fn $constant_name(
            dst: &mut ImageBuf,
            a: &ImageBuf,
            values: &[f32],
            roi: Option<Roi>,
        ) -> Result<()> {
            if values.is_empty() {
                return Err(Error::InvalidImageSpec(
                    "this operation needs at least one channel value".to_owned(),
                ));
            }
            let roi = region_in(roi, dst)?;
            let succeeded =
                unsafe { $constants(dst.inner_mut(), a.inner(), values, &roi, ALL_THREADS) };
            finish(dst, $label, succeeded)
        }
    };
}

binary_operation!(
    add,
    add_constant,
    sys::imagebufalgo::imagebufalgo_add_images,
    sys::imagebufalgo::imagebufalgo_add_constant,
    "add"
);
binary_operation!(
    sub,
    sub_constant,
    sys::imagebufalgo::imagebufalgo_sub_images,
    sys::imagebufalgo::imagebufalgo_sub_constant,
    "subtract"
);
binary_operation!(
    mul,
    mul_constant,
    sys::imagebufalgo::imagebufalgo_mul_images,
    sys::imagebufalgo::imagebufalgo_mul_constant,
    "multiply"
);
binary_operation!(
    div,
    div_constant,
    sys::imagebufalgo::imagebufalgo_div_images,
    sys::imagebufalgo::imagebufalgo_div_constant,
    "divide"
);

/// Fill the region with a vertical gradient.
///
/// The ramp runs over the *region*, not the image, so filling part of an image
/// gives a complete gradient inside that part rather than a slice of a longer
/// one. A region one pixel tall is entirely `top`.
pub fn fill_gradient(
    dst: &mut ImageBuf,
    top: &[f32],
    bottom: &[f32],
    roi: Option<Roi>,
) -> Result<()> {
    let roi = region_in(roi, dst)?;
    let succeeded = unsafe {
        sys::imagebufalgo::imagebufalgo_fill_vertical(
            dst.inner_mut(),
            top,
            bottom,
            &roi,
            ALL_THREADS,
        )
    };
    finish(dst, "fill gradient", succeeded)
}

/// Fill the region by interpolating between four corner colours.
pub fn fill_corners(
    dst: &mut ImageBuf,
    top_left: &[f32],
    top_right: &[f32],
    bottom_left: &[f32],
    bottom_right: &[f32],
    roi: Option<Roi>,
) -> Result<()> {
    let roi = region_in(roi, dst)?;
    let succeeded = unsafe {
        sys::imagebufalgo::imagebufalgo_fill_corners(
            dst.inner_mut(),
            top_left,
            top_right,
            bottom_left,
            bottom_right,
            &roi,
            ALL_THREADS,
        )
    };
    finish(dst, "fill corners", succeeded)
}

/// Fill the region with a checkerboard.
///
/// `size` is one square's width, height and depth; each must be at least one,
/// because OpenImageIO divides the coordinate by them without checking.
/// `offset` shifts the pattern.
pub fn checker(
    dst: &mut ImageBuf,
    size: [u32; 3],
    color1: &[f32],
    color2: &[f32],
    offset: [i32; 3],
    roi: Option<Roi>,
) -> Result<()> {
    let dimension = |name: &'static str, value: u32| {
        i32::try_from(value)
            .map_err(|_| Error::InvalidImageSpec(format!("checker {name} exceeds i32::MAX")))
    };
    let roi = region_in(roi, dst)?;
    let succeeded = unsafe {
        sys::imagebufalgo::imagebufalgo_checker(
            dst.inner_mut(),
            dimension("width", size[0])?,
            dimension("height", size[1])?,
            dimension("depth", size[2])?,
            color1,
            color2,
            offset[0],
            offset[1],
            offset[2],
            &roi,
            ALL_THREADS,
        )
    };
    finish(dst, "checker", succeeded)
}

/// Which kind of noise [`noise`] should add, and its parameters.
///
/// Each variant names what OpenImageIO's two anonymous floats mean for it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Noise {
    /// Normally distributed, about a mean and by a standard deviation.
    Gaussian {
        /// The centre of the distribution.
        mean: f32,
        /// Its spread.
        standard_deviation: f32,
    },
    /// Evenly distributed between two values.
    Uniform {
        /// The smallest value.
        min: f32,
        /// The largest.
        max: f32,
    },
    /// Evenly distributed, but spread out in space so nearby pixels differ —
    /// which looks less clumpy than uniform noise at the same amplitude.
    Blue {
        /// The smallest value.
        min: f32,
        /// The largest.
        max: f32,
    },
    /// Set a portion of the pixels to one value, and leave the rest alone.
    ///
    /// This is the one kind that assigns rather than adds.
    Salt {
        /// The value written.
        value: f32,
        /// The fraction of pixels to write, from 0 to 1.
        portion: f32,
    },
}

impl Noise {
    fn parts(self) -> (&'static str, f32, f32) {
        match self {
            Self::Gaussian {
                mean,
                standard_deviation,
            } => ("gaussian", mean, standard_deviation),
            Self::Uniform { min, max } => ("uniform", min, max),
            Self::Blue { min, max } => ("blue", min, max),
            Self::Salt { value, portion } => ("salt", value, portion),
        }
    }
}

/// Add noise to the region.
///
/// This **adds** to what the destination already holds — every kind but
/// [`Noise::Salt`], which assigns. To generate noise rather than dirty an
/// existing image, start from [`zero`].
///
/// `mono` draws one value per pixel rather than one per channel, so the noise
/// is grey rather than coloured. `seed` makes the result repeatable.
pub fn noise(
    dst: &mut ImageBuf,
    kind: Noise,
    mono: bool,
    seed: i32,
    roi: Option<Roi>,
) -> Result<()> {
    let (name, a, b) = kind.parts();
    let roi = region_in(roi, dst)?;
    let succeeded = unsafe {
        sys::imagebufalgo::imagebufalgo_noise(
            dst.inner_mut(),
            name,
            a,
            b,
            mono,
            seed,
            &roi,
            ALL_THREADS,
        )
    };
    finish(dst, "noise", succeeded)
}

/// Set one pixel.
///
/// A position outside the region is a silent no-op, which is OpenImageIO's
/// behaviour rather than an error. Drawing only happens on the `z = 0` plane.
pub fn render_point(
    dst: &mut ImageBuf,
    x: i32,
    y: i32,
    color: &[f32],
    roi: Option<Roi>,
) -> Result<()> {
    let roi = region_in(roi, dst)?;
    let succeeded = unsafe {
        sys::imagebufalgo::imagebufalgo_render_point(
            dst.inner_mut(),
            x,
            y,
            color,
            &roi,
            ALL_THREADS,
        )
    };
    finish(dst, "render point", succeeded)
}

/// Draw a line between two points, both included.
///
/// `skip_first_point` leaves the starting pixel alone, which is what you want
/// when drawing a chain of lines so the joints are not blended twice.
///
/// The colour is blended by its alpha. A colour with fewer values than the
/// image has channels repeats its last value, so a three-value colour on an
/// RGBA image gives alpha the red value's neighbour rather than one — pass all
/// four to be sure.
pub fn render_line(
    dst: &mut ImageBuf,
    from: [i32; 2],
    to: [i32; 2],
    color: &[f32],
    skip_first_point: bool,
    roi: Option<Roi>,
) -> Result<()> {
    let roi = region_in(roi, dst)?;
    let succeeded = unsafe {
        sys::imagebufalgo::imagebufalgo_render_line(
            dst.inner_mut(),
            from[0],
            from[1],
            to[0],
            to[1],
            color,
            skip_first_point,
            &roi,
            ALL_THREADS,
        )
    };
    finish(dst, "render line", succeeded)
}

/// Draw a box, outlined or filled. Both corners are included.
///
/// A filled box needs its first corner above and to the left of its second;
/// OpenImageIO draws nothing for the other order and calls it success, so that
/// is refused here. An outline accepts either order.
pub fn render_box(
    dst: &mut ImageBuf,
    corner: [i32; 2],
    opposite: [i32; 2],
    color: &[f32],
    fill: bool,
    roi: Option<Roi>,
) -> Result<()> {
    let roi = region_in(roi, dst)?;
    let succeeded = unsafe {
        sys::imagebufalgo::imagebufalgo_render_box(
            dst.inner_mut(),
            corner[0],
            corner[1],
            opposite[0],
            opposite[1],
            color,
            fill,
            &roi,
            ALL_THREADS,
        )
    };
    finish(dst, "render box", succeeded)
}

/// Where text sits relative to the position given for it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[non_exhaustive]
pub enum TextAlignX {
    /// The position is the text's left edge.
    #[default]
    Left,
    /// The position is its right edge.
    Right,
    /// The position is its horizontal centre.
    Center,
}

/// Where text sits vertically relative to the position given for it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[non_exhaustive]
pub enum TextAlignY {
    /// The position is the baseline the letters stand on.
    #[default]
    Baseline,
    /// The position is the top of the text.
    Top,
    /// The position is the bottom, below any descenders.
    Bottom,
    /// The position is its vertical centre.
    Center,
}

/// How [`render_text`] should draw.
#[derive(Debug, Clone, Copy)]
pub struct TextOptions<'a> {
    /// Height in pixels. Must be at least one.
    pub size: u32,
    /// The font's name or path. Empty asks OpenImageIO for its default, which
    /// it may not have; an unfindable font is an error.
    pub font: &'a str,
    /// One value per channel. Blended by its alpha, as [`render_line`] notes.
    pub color: &'a [f32],
    /// Horizontal placement relative to the position.
    pub align_x: TextAlignX,
    /// Vertical placement relative to the position.
    pub align_y: TextAlignY,
    /// Draw a dark halo this many pixels wide behind the text, so it stays
    /// legible over a busy image. Zero for none.
    pub shadow: u32,
}

impl Default for TextOptions<'_> {
    fn default() -> Self {
        Self {
            size: 16,
            font: "",
            color: &[1.0],
            align_x: TextAlignX::default(),
            align_y: TextAlignY::default(),
            shadow: 0,
        }
    }
}

/// Draw text into the image.
///
/// Text with nothing to draw — empty, or only line breaks — is refused.
/// OpenImageIO measures an inverted bounding box for it and then builds an
/// image from that box without checking, so its width underflows.
///
/// This needs a font. OpenImageIO looks in its own search path and in the
/// system's; if it was built without FreeType, or cannot find the font named,
/// the call fails.
pub fn render_text(
    dst: &mut ImageBuf,
    position: [i32; 2],
    text: &str,
    options: &TextOptions<'_>,
    roi: Option<Roi>,
) -> Result<()> {
    let (size, shadow) = text_metrics(options.size, options.shadow)?;
    let roi = region_in(roi, dst)?;
    let succeeded = unsafe {
        sys::imagebufalgo::imagebufalgo_render_text(
            dst.inner_mut(),
            position[0],
            position[1],
            text,
            size,
            options.font,
            options.color,
            options.align_x as i32,
            options.align_y as i32,
            shadow,
            &roi,
            ALL_THREADS,
        )
    };
    finish(dst, "render text", succeeded)
}

/// Measure what [`render_text`] would draw, without drawing it.
///
/// The region is where the text would land if drawn at the origin, so its width
/// and height are the text's size.
///
/// OpenImageIO reports failure here by returning nothing at all — no message,
/// on the buffer or anywhere else — so a missing font and unrenderable text
/// come back as the same error.
///
/// It also measures only x and y, leaving the region's depth and channel
/// ranges empty; `render_text` completes them for its own use and this does the
/// same, so the region that comes back is one you can actually pass on.
pub fn text_size(text: &str, size: u32, font: &str) -> Result<Roi> {
    let (size, _) = text_metrics(size, 0)?;
    let measured = unsafe { sys::imagebufalgo::imagebufalgo_text_size(text, size, font) };
    Roi::from_sys_optional(measured)?.ok_or_else(|| {
        Error::operation(
            "text size",
            "nothing could be measured; the text may be empty, or the font \
             missing, and OpenImageIO does not say which"
                .to_owned(),
        )
    })
}

fn text_metrics(size: u32, shadow: u32) -> Result<(i32, i32)> {
    let size = i32::try_from(size)
        .map_err(|_| Error::InvalidImageSpec("font size exceeds i32::MAX".to_owned()))?;
    if size < 1 {
        return Err(Error::InvalidImageSpec(
            "the font size must be at least 1".to_owned(),
        ));
    }
    let shadow = i32::try_from(shadow)
        .map_err(|_| Error::InvalidImageSpec("shadow width exceeds i32::MAX".to_owned()))?;
    Ok((size, shadow))
}

/// Composite each pixel's deep samples down into one flat pixel.
///
/// Samples are combined front to back, so the nearest opaque one hides what is
/// behind it. This is how a deep render becomes an ordinary image.
///
/// The source needs an alpha channel — OpenImageIO finds it by name, accepting
/// `A`, `Alpha`, the per-component `AR`/`AG`/`AB`, and any name ending in
/// `.A` — and fails without one. A pixel with no samples comes back with its
/// depth at 1e30 and its colours at zero.
///
/// The destination must not have more channels than the source, which is a
/// restriction of OpenImageIO's rather than of the operation: its per-pixel
/// accumulator is sized from the source and indexed up to the wider of the two.
///
/// A source that is already flat is copied wholesale, region and all, which is
/// OpenImageIO's documented behaviour.
///
/// The compositing assumes each pixel's samples are sorted and do not overlap.
/// OpenImageIO says so itself, in a comment on the code; a file that does not
/// satisfy that gives a wrong answer rather than an error.
pub fn flatten(dst: &mut ImageBuf, src: &ImageBuf, roi: Option<Roi>) -> Result<()> {
    let roi = region_in(roi, dst)?;
    let succeeded = unsafe {
        sys::imagebufalgo::imagebufalgo_flatten(dst.inner_mut(), src.inner(), &roi, ALL_THREADS)
    };
    finish(dst, "flatten", succeeded)
}

/// Turn a flat image into a deep one, with at most one sample per pixel.
///
/// A pixel gets a sample when any channel other than depth — `Z` and `Zback`
/// both count as depth here — is non-zero. When the source has its own `Z`
/// channel, a pixel whose only non-zero channel is that depth also gets a
/// sample, provided the depth is below OpenImageIO's 1e30 "infinitely far"
/// cutoff; a source without a `Z` channel gets no such consideration. Depth
/// comes from the source's own `Z` channel if it has one — in which case
/// `z_value` is ignored — and from `z_value` otherwise, with a `Z` channel
/// appended to hold it.
///
/// The destination must be empty, so it can take the deep, floating-point
/// specification this builds. OpenImageIO would otherwise keep a pre-allocated
/// destination's shape and silently drop the writes that do not fit.
pub fn deepen(dst: &mut ImageBuf, src: &ImageBuf, z_value: f32, roi: Option<Roi>) -> Result<()> {
    let roi = region_in(roi, dst)?;
    let succeeded = unsafe {
        sys::imagebufalgo::imagebufalgo_deepen(
            dst.inner_mut(),
            src.inner(),
            z_value,
            &roi,
            ALL_THREADS,
        )
    };
    finish(dst, "deepen", succeeded)
}

/// Merge two deep images, interleaving their samples by depth.
///
/// Overlapping samples are split at each other's depths so the result stays
/// sortable. `occlusion_cull` then drops samples hidden behind an opaque one,
/// which is usually what you want and is OpenImageIO's default.
///
/// All three images must be deep and must have the same channels, in the same
/// order and by the same names.
///
/// This is expensive: the pass that reserves room is quadratic in the number of
/// samples per pixel and runs on one thread, so a dense volumetric image is
/// slow in a way nothing in the signature suggests.
pub fn deep_merge(
    dst: &mut ImageBuf,
    a: &ImageBuf,
    b: &ImageBuf,
    occlusion_cull: bool,
    roi: Option<Roi>,
) -> Result<()> {
    let roi = region_in(roi, dst)?;
    let succeeded = unsafe {
        sys::imagebufalgo::imagebufalgo_deep_merge(
            dst.inner_mut(),
            a.inner(),
            b.inner(),
            occlusion_cull,
            &roi,
            ALL_THREADS,
        )
    };
    finish(dst, "deep merge", succeeded)
}

/// Keep only the samples of `src` in front of `holdout`'s opaque frontier.
///
/// This is the deep equivalent of masking one render against another: whatever
/// `holdout` hides is removed from `src`.
///
/// Only `holdout`'s depths and alpha matter; its other channels are not read,
/// and it need not share `src`'s channel layout. If `holdout` has no alpha,
/// OpenImageIO quietly uses its nearest sample instead of a real opacity
/// frontier, which is a different operation than the one asked for.
///
/// As with [`flatten`], the samples are assumed sorted by depth.
pub fn deep_holdout(
    dst: &mut ImageBuf,
    src: &ImageBuf,
    holdout: &ImageBuf,
    roi: Option<Roi>,
) -> Result<()> {
    let roi = region_in(roi, dst)?;
    let succeeded = unsafe {
        sys::imagebufalgo::imagebufalgo_deep_holdout(
            dst.inner_mut(),
            src.inner(),
            holdout.inner(),
            &roi,
            ALL_THREADS,
        )
    };
    finish(dst, "deep holdout", succeeded)
}

/// Build a convolution kernel by name.
///
/// The name is a reconstruction filter: `"gaussian"`, `"box"`, `"triangle"`,
/// `"catmull-rom"`, `"blackman-harris"`, `"sinc"`, `"lanczos3"`, `"mitchell"`,
/// `"b-spline"`, `"disk"`, `"binomial"`, `"laplacian"` and a few more. An
/// unknown name is an error rather than a silent fall back to a box, which is
/// what OpenImageIO does on its own.
///
/// The result is centred on the origin, which is what [`convolve`] expects. A
/// kernel read from a file has its origin at a corner instead, and convolving
/// with one shifts the image by half its size.
///
/// `normalize` scales the kernel to sum to one. Leave it off for a kernel that
/// is meant to sum to zero, such as `"laplacian"`.
pub fn make_kernel(name: &str, width: f32, height: f32, normalize: bool) -> Result<ImageBuf> {
    let mut kernel = ImageBuf::empty()?;
    let succeeded = unsafe {
        sys::imagebufalgo::imagebufalgo_make_kernel(
            kernel.inner_mut(),
            name,
            width,
            height,
            1.0,
            normalize,
        )
    };
    if succeeded {
        Ok(kernel)
    } else {
        Err(Error::operation("make kernel", kernel.take_error()))
    }
}

/// Convolve with a kernel image.
///
/// The kernel's own origin is the filter's centre, so use [`make_kernel`],
/// which centres it. An empty kernel is refused: OpenImageIO would divide by
/// its zero sum and fill the result with `NaN` while reporting success.
pub fn convolve(
    dst: &mut ImageBuf,
    src: &ImageBuf,
    kernel: &ImageBuf,
    normalize: bool,
    roi: Option<Roi>,
) -> Result<()> {
    let roi = region_in(roi, dst)?;
    let succeeded = unsafe {
        sys::imagebufalgo::imagebufalgo_convolve(
            dst.inner_mut(),
            src.inner(),
            kernel.inner(),
            normalize,
            &roi,
            ALL_THREADS,
        )
    };
    finish(dst, "convolve", succeeded)
}

/// Apply the 3x3 Laplacian, an edge detector.
///
/// The result is a signed second derivative and is not normalised, so it holds
/// negative values. Give it a floating-point destination; an integer one clamps
/// everything below zero away.
pub fn laplacian(dst: &mut ImageBuf, src: &ImageBuf, roi: Option<Roi>) -> Result<()> {
    let roi = region_in(roi, dst)?;
    let succeeded = unsafe {
        sys::imagebufalgo::imagebufalgo_laplacian(dst.inner_mut(), src.inner(), &roi, ALL_THREADS)
    };
    finish(dst, "laplacian", succeeded)
}

/// Sharpen by subtracting a blurred copy.
///
/// `kernel` is the blur to subtract — `"gaussian"` by default, or the special
/// name `"median"` for a median blur, which sharpens without haloing an edge.
/// `contrast` scales the difference added back, and `threshold` leaves
/// differences smaller than itself alone, so grain is not amplified.
///
/// The destination must either be empty or hold the same pixel type as the
/// source. OpenImageIO reads the source through an iterator of the
/// *destination's* type without converting, so a mismatch would misread the
/// source and, for a wider destination type, read past its end.
pub fn unsharp_mask(
    dst: &mut ImageBuf,
    src: &ImageBuf,
    kernel: &str,
    width: f32,
    contrast: f32,
    threshold: f32,
    roi: Option<Roi>,
) -> Result<()> {
    let roi = region_in(roi, dst)?;
    let succeeded = unsafe {
        sys::imagebufalgo::imagebufalgo_unsharp_mask(
            dst.inner_mut(),
            src.inner(),
            kernel,
            width,
            contrast,
            threshold,
            &roi,
            ALL_THREADS,
        )
    };
    finish(dst, "unsharp mask", succeeded)
}

/// Replace each pixel with the median of its neighbourhood.
///
/// This removes salt-and-pepper noise without softening edges, which a blur
/// would. `height` of `None` matches the width.
///
/// The window must be at least two across. OpenImageIO accepts one and returns
/// the image translated by a pixel rather than unchanged, so that is refused
/// here.
pub fn median_filter(
    dst: &mut ImageBuf,
    src: &ImageBuf,
    width: u32,
    height: Option<u32>,
    roi: Option<Roi>,
) -> Result<()> {
    let (width, height) = window("median filter", width, height)?;
    let roi = region_in(roi, dst)?;
    let succeeded = unsafe {
        sys::imagebufalgo::imagebufalgo_median_filter(
            dst.inner_mut(),
            src.inner(),
            width,
            height,
            &roi,
            ALL_THREADS,
        )
    };
    finish(dst, "median filter", succeeded)
}

/// Grow the bright areas: each pixel becomes the maximum of its neighbourhood.
///
/// `height` of `None` matches the width, and the window must be at least two
/// across, as [`median_filter`] explains.
pub fn dilate(
    dst: &mut ImageBuf,
    src: &ImageBuf,
    width: u32,
    height: Option<u32>,
    roi: Option<Roi>,
) -> Result<()> {
    let (width, height) = window("dilate", width, height)?;
    let roi = region_in(roi, dst)?;
    let succeeded = unsafe {
        sys::imagebufalgo::imagebufalgo_dilate(
            dst.inner_mut(),
            src.inner(),
            width,
            height,
            &roi,
            ALL_THREADS,
        )
    };
    finish(dst, "dilate", succeeded)
}

/// Shrink the bright areas: each pixel becomes the minimum of its
/// neighbourhood.
///
/// `height` of `None` matches the width, and the window must be at least two
/// across, as [`median_filter`] explains.
pub fn erode(
    dst: &mut ImageBuf,
    src: &ImageBuf,
    width: u32,
    height: Option<u32>,
    roi: Option<Roi>,
) -> Result<()> {
    let (width, height) = window("erode", width, height)?;
    let roi = region_in(roi, dst)?;
    let succeeded = unsafe {
        sys::imagebufalgo::imagebufalgo_erode(
            dst.inner_mut(),
            src.inner(),
            width,
            height,
            &roi,
            ALL_THREADS,
        )
    };
    finish(dst, "erode", succeeded)
}

fn window(operation: &str, width: u32, height: Option<u32>) -> Result<(i32, i32)> {
    let to_i32 = |value: u32| {
        i32::try_from(value)
            .map_err(|_| Error::InvalidImageSpec(format!("{operation} window exceeds i32::MAX")))
    };
    Ok((
        to_i32(width)?,
        height.map(to_i32).transpose()?.unwrap_or(-1),
    ))
}

/// Transform one channel into the frequency domain.
///
/// The result is always a two-channel float image at the origin — real part
/// first, imaginary part second — whatever the destination held before.
///
/// Only one channel is transformed: the region's first, or channel zero. The
/// region also defaults to the union of the data and display windows rather
/// than to the data window alone, so an image whose pixels are smaller than its
/// display window is transformed with the difference zero-padded.
pub fn fft(dst: &mut ImageBuf, src: &ImageBuf, roi: Option<Roi>) -> Result<()> {
    let roi = region_in(roi, dst)?;
    let succeeded = unsafe {
        sys::imagebufalgo::imagebufalgo_fft(dst.inner_mut(), src.inner(), &roi, ALL_THREADS)
    };
    finish(dst, "fft", succeeded)
}

/// Transform back out of the frequency domain.
///
/// The source must be the two-channel complex image [`fft`] produces, and its
/// pixels must be in memory: a buffer still attached to a file has no pixel
/// address, and OpenImageIO would dereference the null it gets back. Call
/// [`ImageBuf::read`](crate::ImageBuf::read) first if in doubt.
pub fn ifft(dst: &mut ImageBuf, src: &ImageBuf, roi: Option<Roi>) -> Result<()> {
    let roi = region_in(roi, dst)?;
    let succeeded = unsafe {
        sys::imagebufalgo::imagebufalgo_ifft(dst.inner_mut(), src.inner(), &roi, ALL_THREADS)
    };
    finish(dst, "ifft", succeeded)
}

/// Convert a two-channel magnitude-and-phase image into real and imaginary
/// parts.
///
/// Both images need exactly two channels. Phase is in radians.
///
/// (OpenImageIO's header describes this the other way round. The name is right
/// and the prose is wrong; this converts *from* polar.)
pub fn polar_to_complex(dst: &mut ImageBuf, src: &ImageBuf, roi: Option<Roi>) -> Result<()> {
    let roi = region_in(roi, dst)?;
    let succeeded = unsafe {
        sys::imagebufalgo::imagebufalgo_polar_to_complex(
            dst.inner_mut(),
            src.inner(),
            &roi,
            ALL_THREADS,
        )
    };
    finish(dst, "polar to complex", succeeded)
}

/// Convert a two-channel real-and-imaginary image into magnitude and phase.
///
/// Both images need exactly two channels. The phase comes back in `0..2π`,
/// not `-π..π`.
pub fn complex_to_polar(dst: &mut ImageBuf, src: &ImageBuf, roi: Option<Roi>) -> Result<()> {
    let roi = region_in(roi, dst)?;
    let succeeded = unsafe {
        sys::imagebufalgo::imagebufalgo_complex_to_polar(
            dst.inner_mut(),
            src.inner(),
            &roi,
            ALL_THREADS,
        )
    };
    finish(dst, "complex to polar", succeeded)
}

/// Settings the OpenColorIO operations share.
///
/// [`Default`] matches OpenImageIO's own defaults, which means `unpremult` is
/// **on**: colour is divided by alpha before the transform and multiplied back
/// after, which is what a premultiplied image needs.
#[derive(Debug, Clone, Copy)]
pub struct OcioOptions<'a> {
    /// Divide colour by alpha before transforming and multiply back after.
    ///
    /// OpenImageIO ignores this when the region covers fewer than four
    /// channels, or when the source is marked as holding unassociated alpha.
    pub unpremult: bool,
    /// Apply the transform backwards.
    pub inverse: bool,
    /// An OpenColorIO context key, or several separated by commas.
    ///
    /// Keys and values are matched up pairwise. If the two lists are different
    /// lengths, or only one is given, OpenImageIO discards the context without
    /// saying so.
    pub context_key: &'a str,
    /// The value, or values, for [`context_key`](Self::context_key).
    pub context_value: &'a str,
}

impl Default for OcioOptions<'_> {
    fn default() -> Self {
        Self {
            unpremult: true,
            inverse: false,
            context_key: "",
            context_value: "",
        }
    }
}

/// Transform colours by a 4x4 matrix.
///
/// `matrix` is sixteen values in row order, applied as a row vector times the
/// matrix, so elements 12, 13 and 14 are the translation.
///
/// Two consequences of OpenImageIO applying this as a four-component transform
/// are worth knowing. The fourth component is the alpha channel when the image
/// has one and zero when it does not, so on an RGB image the translation row is
/// multiplied by zero and has no effect; and on an RGBA image, a matrix whose
/// fourth column is not `(0, 0, 0, 1)` changes alpha as well as colour.
///
/// This does not consult OpenColorIO, and does not update the image's recorded
/// colour space, which will go on claiming whatever the source said.
pub fn color_matrix_transform(
    dst: &mut ImageBuf,
    src: &ImageBuf,
    matrix: &[f32; 16],
    unpremult: bool,
    roi: Option<Roi>,
) -> Result<()> {
    let roi = region_in(roi, dst)?;
    let succeeded = unsafe {
        sys::imagebufalgo::imagebufalgo_colormatrixtransform(
            dst.inner_mut(),
            src.inner(),
            matrix,
            unpremult,
            &roi,
            ALL_THREADS,
        )
    };
    finish(dst, "colour matrix transform", succeeded)
}

/// Apply one or more OpenColorIO looks.
///
/// `looks` is a comma- or colon-separated list, each optionally prefixed `+` or
/// `-` to apply it forwards or backwards. An empty list is allowed, and gives a
/// plain conversion from one space to the other.
///
/// `from_space` and `to_space` of `None` mean the source's own recorded colour
/// space, falling back to the configuration's `scene_linear` role.
pub fn ocio_look(
    dst: &mut ImageBuf,
    src: &ImageBuf,
    looks: &str,
    from_space: Option<&str>,
    to_space: Option<&str>,
    options: &OcioOptions<'_>,
    roi: Option<Roi>,
) -> Result<()> {
    let roi = region_in(roi, dst)?;
    let succeeded = unsafe {
        sys::imagebufalgo::imagebufalgo_ociolook(
            dst.inner_mut(),
            src.inner(),
            looks,
            from_space.unwrap_or_default(),
            to_space.unwrap_or_default(),
            options.unpremult,
            options.inverse,
            options.context_key,
            options.context_value,
            &roi,
            ALL_THREADS,
        )
    };
    finish(dst, "OpenColorIO look", succeeded)
}

/// Apply an OpenColorIO display and view transform.
///
/// This is what turns a rendered image into one to put on a particular screen.
/// `from_space` of `None` means the source's own recorded colour space.
// Eight arguments, one past clippy's limit. The display, the view, the source
// space and the looks are four independent names with no natural grouping —
// inventing a struct to hold them would obscure the call rather than clarify
// it, and OpenImageIO's own signature has them separate for the same reason.
#[allow(clippy::too_many_arguments)]
pub fn ocio_display(
    dst: &mut ImageBuf,
    src: &ImageBuf,
    display: &str,
    view: &str,
    from_space: Option<&str>,
    looks: &str,
    options: &OcioOptions<'_>,
    roi: Option<Roi>,
) -> Result<()> {
    let roi = region_in(roi, dst)?;
    let succeeded = unsafe {
        sys::imagebufalgo::imagebufalgo_ociodisplay(
            dst.inner_mut(),
            src.inner(),
            display,
            view,
            from_space.unwrap_or_default(),
            looks,
            options.unpremult,
            options.inverse,
            options.context_key,
            options.context_value,
            &roi,
            ALL_THREADS,
        )
    };
    finish(dst, "OpenColorIO display", succeeded)
}

/// Apply a transform read from a file: a LUT, a `.cube`, a `.clf`, or anything
/// else OpenColorIO's file transform accepts.
///
/// The file is the whole transform, so there are no colour spaces to name.
///
/// On success OpenColorIO may retag the result's colour space from the
/// transform file's own path, if that path matches one of the configuration's
/// file rules — so the name of your LUT can change what the image claims to be.
pub fn ocio_file_transform(
    dst: &mut ImageBuf,
    src: &ImageBuf,
    transform_path: &std::path::Path,
    options: &OcioOptions<'_>,
    roi: Option<Roi>,
) -> Result<()> {
    let name = crate::path_to_utf8(transform_path)?;
    let roi = region_in(roi, dst)?;
    let succeeded = unsafe {
        sys::imagebufalgo::imagebufalgo_ociofiletransform(
            dst.inner_mut(),
            src.inner(),
            name,
            options.unpremult,
            options.inverse,
            &roi,
            ALL_THREADS,
        )
    };
    finish(dst, "OpenColorIO file transform", succeeded)
}

/// Apply a transform the OpenColorIO configuration defines by name.
pub fn ocio_named_transform(
    dst: &mut ImageBuf,
    src: &ImageBuf,
    name: &str,
    options: &OcioOptions<'_>,
    roi: Option<Roi>,
) -> Result<()> {
    let roi = region_in(roi, dst)?;
    let succeeded = unsafe {
        sys::imagebufalgo::imagebufalgo_ocionamedtransform(
            dst.inner_mut(),
            src.inner(),
            name,
            options.unpremult,
            options.inverse,
            options.context_key,
            options.context_value,
            &roi,
            ALL_THREADS,
        )
    };
    finish(dst, "OpenColorIO named transform", succeeded)
}

/// What [`pixel_stats`] measured, one entry per channel of the source.
///
/// Every vector has the source's channel count, not the region's, so a region
/// that names fewer channels still reports every channel; those it did not
/// visit have a [`finite_count`](Self::finite_count) of zero.
#[derive(Debug, Clone, PartialEq)]
pub struct PixelStats {
    /// The smallest finite value seen.
    pub min: Vec<f32>,
    /// The largest finite value seen.
    pub max: Vec<f32>,
    /// The mean of the finite values.
    pub average: Vec<f32>,
    /// The population standard deviation of the finite values.
    pub standard_deviation: Vec<f32>,
    /// How many values were `NaN`.
    pub nan_count: Vec<u64>,
    /// How many were infinite.
    pub infinite_count: Vec<u64>,
    /// How many were neither, and so contributed to the figures above.
    pub finite_count: Vec<u64>,
}

/// Measure each channel: range, mean, spread, and how many values were not
/// finite.
///
/// `NaN` and infinity are counted rather than folded into the average, so a
/// render with a handful of bad pixels still reports a usable range. A channel
/// with no finite values at all reports zeroes; its
/// [`finite_count`](PixelStats::finite_count) is what distinguishes that from
/// a channel that really is all zero.
///
/// Deep images are refused: their samples are not one value per pixel, so a
/// per-pixel mean would not mean anything.
pub fn pixel_stats(src: &ImageBuf, roi: Option<Roi>) -> Result<PixelStats> {
    let roi = region_in(roi, src)?;
    let stats =
        unsafe { sys::imagebufalgo::imagebufalgo_pixel_stats(src.inner(), &roi, ALL_THREADS) };
    if !stats.ok {
        return Err(Error::operation("pixel statistics", stats.error));
    }
    Ok(PixelStats {
        min: stats.min,
        max: stats.max,
        average: stats.average,
        standard_deviation: stats.standard_deviation,
        nan_count: stats.nan_count,
        infinite_count: stats.infinite_count,
        finite_count: stats.finite_count,
    })
}

/// Count how many pixels of one channel fall in each of `bins` equal buckets
/// spanning `range`.
///
/// Values outside `range` are counted in the nearest bucket rather than
/// discarded, so the totals always add up to the number of pixels examined.
///
/// `ignore_empty` skips pixels that are zero in every channel of the region,
/// which is how to exclude the transparent surround of a render.
pub fn histogram(
    src: &ImageBuf,
    channel: u32,
    bins: u32,
    range: std::ops::Range<f32>,
    ignore_empty: bool,
    roi: Option<Roi>,
) -> Result<Vec<u64>> {
    let channel = i32::try_from(channel)
        .map_err(|_| Error::InvalidImageSpec("channel exceeds i32::MAX".to_owned()))?;
    let bins = i32::try_from(bins)
        .map_err(|_| Error::InvalidImageSpec("bin count exceeds i32::MAX".to_owned()))?;
    let roi = region_in(roi, src)?;
    let mut message = String::new();
    let counts = unsafe {
        sys::imagebufalgo::imagebufalgo_histogram(
            src.inner(),
            channel,
            bins,
            range.start,
            range.end,
            ignore_empty,
            &roi,
            ALL_THREADS,
            &mut message,
        )
    };
    if counts.is_empty() {
        return Err(Error::operation("histogram", message));
    }
    Ok(counts.into_iter().collect())
}

/// The colour every pixel shares, if they all share one.
///
/// `None` means the image varies. The colour is one value per channel of the
/// source, with channels outside the region zeroed.
///
/// A `threshold` of zero compares the stored values exactly, in the image's
/// own format, without converting each to `f32` first. For the formats
/// narrower than `f32` that conversion is exact, so the answer matches; for
/// a wider format — `f64`, `u32`, `i32` — the native comparison is the
/// stricter one, and distinguishes values a conversion to `f32` would
/// collapse, such as `u32` values past the 24-bit mantissa.
///
/// The region must begin at channel zero. OpenImageIO sizes its reference
/// buffer to the region's channel count but fills it by absolute channel
/// number, so a region starting higher writes past the end of that buffer.
pub fn constant_color(
    src: &ImageBuf,
    threshold: f32,
    roi: Option<Roi>,
) -> Result<Option<Vec<f32>>> {
    let channels = src.spec()?.channel_count() as usize;
    let mut color = vec![0.0_f32; channels];
    let roi = region_in(roi, src)?;
    let mut message = String::new();
    let constant = unsafe {
        sys::imagebufalgo::imagebufalgo_is_constant_color(
            src.inner(),
            threshold,
            &mut color,
            &roi,
            ALL_THREADS,
            &mut message,
        )
    };
    if constant {
        return Ok(Some(color));
    }
    if message.is_empty() {
        // Not constant, which is an answer rather than a failure.
        Ok(None)
    } else {
        Err(Error::operation("constant colour", message))
    }
}

/// Whether one channel holds `value` everywhere in the region.
///
/// The channel must exist: OpenImageIO answers a bad channel index with the
/// same `false` it uses for "not constant", so this reports an error instead.
pub fn is_constant_channel(
    src: &ImageBuf,
    channel: u32,
    value: f32,
    threshold: f32,
    roi: Option<Roi>,
) -> Result<bool> {
    let channel = i32::try_from(channel)
        .map_err(|_| Error::InvalidImageSpec("channel exceeds i32::MAX".to_owned()))?;
    let roi = region_in(roi, src)?;
    let mut message = String::new();
    let constant = unsafe {
        sys::imagebufalgo::imagebufalgo_is_constant_channel(
            src.inner(),
            channel,
            value,
            threshold,
            &roi,
            ALL_THREADS,
            &mut message,
        )
    };
    if !constant && !message.is_empty() {
        return Err(Error::operation("constant channel", message));
    }
    Ok(constant)
}

/// Whether every channel of each pixel holds that pixel's first channel.
///
/// This is a per-pixel test, so a greyscale gradient is monochrome. Alpha
/// counts, which is rarely what you want: narrow the region to the colour
/// channels, or an opaque grey image reports false because alpha is 1 where
/// the colours are not.
pub fn is_monochrome(src: &ImageBuf, threshold: f32, roi: Option<Roi>) -> Result<bool> {
    let roi = region_in(roi, src)?;
    let mut message = String::new();
    let monochrome = unsafe {
        sys::imagebufalgo::imagebufalgo_is_monochrome(
            src.inner(),
            threshold,
            &roi,
            ALL_THREADS,
            &mut message,
        )
    };
    if !monochrome && !message.is_empty() {
        return Err(Error::operation("monochrome test", message));
    }
    Ok(monochrome)
}

/// The smallest region outside which every pixel is black.
///
/// `None` means there is nothing but black. This is how to trim the empty
/// surround from a render before writing it.
///
/// A pixel counts as black only when every channel of the region is exactly
/// zero, alpha included, so an image with alpha 1 over a black background does
/// not shrink at all. The search trims one row or column at a time, so it costs
/// a pass per edge rather than a single pass over the image.
///
/// The region must begin at channel zero, for the reason
/// [`constant_color`] gives: this is built on it.
pub fn nonzero_region(src: &ImageBuf, roi: Option<Roi>) -> Result<Option<Roi>> {
    let roi = region_in(roi, src)?;
    let mut message = String::new();
    let found = unsafe {
        sys::imagebufalgo::imagebufalgo_nonzero_region(src.inner(), &roi, ALL_THREADS, &mut message)
    };
    if !message.is_empty() {
        return Err(Error::operation("nonzero region", message));
    }
    Roi::from_sys_optional(found)
}

/// A SHA-1 digest of the region's pixels, as hexadecimal.
///
/// This hashes the bytes as the image stores them, not the values they mean, so
/// the same picture held as `half` and as `f32` gives different digests. It is
/// for asking "did this file's pixels change", not "do these two images look
/// the same"; [`compare`] answers that.
///
/// `extra_info` is mixed into the digest, so a caller can bind it to something
/// beyond the pixels.
///
/// A region narrower than the image gives an answer that depends on how the
/// buffer was loaded, so prefer a full-width region or none at all.
pub fn pixel_hash_sha1(src: &ImageBuf, extra_info: &str, roi: Option<Roi>) -> Result<String> {
    let roi = region_in(roi, src)?;
    let mut message = String::new();
    let digest = unsafe {
        sys::imagebufalgo::imagebufalgo_pixel_hash_sha1(
            src.inner(),
            extra_info,
            &roi,
            ALL_THREADS,
            &mut message,
        )
    };
    if digest.is_empty() {
        return Err(Error::operation("pixel hash", message));
    }
    Ok(digest)
}

/// Rotate a quarter turn clockwise.
///
/// The region selects part of the **source**, not of the destination; the
/// module documentation lists the seven operations that do, `flip`, `flop` and
/// `transpose` among them. Prefer an empty destination: OpenImageIO installs the rotated
/// display window only when it allocates one itself, and reads a pre-allocated
/// destination's display window while writing, so one that disagrees with the
/// source's yields silently offset pixels.
pub fn rotate_90(dst: &mut ImageBuf, src: &ImageBuf, src_roi: Option<Roi>) -> Result<()> {
    let roi = region(src_roi);
    let succeeded = unsafe {
        sys::imagebufalgo::imagebufalgo_rotate90(dst.inner_mut(), src.inner(), &roi, ALL_THREADS)
    };
    finish(dst, "rotate 90", succeeded)
}

/// Rotate a half turn. See [`rotate_90`] for how the region is read.
pub fn rotate_180(dst: &mut ImageBuf, src: &ImageBuf, src_roi: Option<Roi>) -> Result<()> {
    let roi = region(src_roi);
    let succeeded = unsafe {
        sys::imagebufalgo::imagebufalgo_rotate180(dst.inner_mut(), src.inner(), &roi, ALL_THREADS)
    };
    finish(dst, "rotate 180", succeeded)
}

/// Rotate three quarters clockwise. See [`rotate_90`] for how the region is
/// read.
pub fn rotate_270(dst: &mut ImageBuf, src: &ImageBuf, src_roi: Option<Roi>) -> Result<()> {
    let roi = region(src_roi);
    let succeeded = unsafe {
        sys::imagebufalgo::imagebufalgo_rotate270(dst.inner_mut(), src.inner(), &roi, ALL_THREADS)
    };
    finish(dst, "rotate 270", succeeded)
}

/// Undo the source's `Orientation` attribute, so the image is stored the way
/// it should be displayed.
///
/// This is the operation for a photograph a camera stored sideways with a tag
/// saying so. It has no region: it always works on the whole image.
///
/// An `Orientation` outside the eight EXIF values is an error.
pub fn reorient(dst: &mut ImageBuf, src: &ImageBuf) -> Result<()> {
    let succeeded = unsafe {
        sys::imagebufalgo::imagebufalgo_reorient(dst.inner_mut(), src.inner(), ALL_THREADS)
    };
    finish(dst, "reorient", succeeded)
}

/// How [`warp`], [`rotate`] and [`st_warp`] should resample, and what to do at
/// the edges.
#[derive(Debug, Clone, Copy, Default)]
pub struct WarpOptions<'a> {
    /// The reconstruction filter's name. `None` means `lanczos3`.
    pub filter: Option<&'a str>,
    /// The filter's width. `None` means that filter's own.
    pub filter_width: Option<f32>,
    /// What lies outside the source: `"black"`, `"clamp"`, `"periodic"` or
    /// `"mirror"`. `None` leaves OpenImageIO's default.
    ///
    /// [`rotate`] rejects anything but `None` here, because OpenImageIO fixes
    /// it at black and would ignore the request.
    pub wrap: Option<&'a str>,
    /// Extend the source's edge pixels outward before filtering, so a filter
    /// does not pull darkness in from beyond the data window.
    ///
    /// [`rotate`] rejects this for the same reason it rejects `wrap`.
    pub edge_clamp: bool,
    /// Grow the result to hold the whole transformed image. Without it the
    /// result keeps the source's dimensions and the corners are lost.
    pub recompute_region: bool,
}

/// Rotate by an arbitrary angle.
///
/// `angle` is in **radians**, and turns clockwise, because y points down.
///
/// `center` defaults to the middle of the display window, which is not the
/// middle of the data window when the image is cropped or has overscan.
///
/// Pixels outside the source read as black. OpenImageIO offers no choice about
/// that here, so rather than accept a wrap mode and drop it, this refuses
/// [`WarpOptions::wrap`] and [`WarpOptions::edge_clamp`]; [`warp`] with a
/// rotation matrix honours both.
pub fn rotate(
    dst: &mut ImageBuf,
    src: &ImageBuf,
    angle: f32,
    center: Option<[f32; 2]>,
    options: &WarpOptions<'_>,
    roi: Option<Roi>,
) -> Result<()> {
    if options.wrap.is_some() || options.edge_clamp {
        return Err(Error::InvalidImageSpec(
            "rotate always leaves black outside the source; use warp with a rotation matrix to choose a wrap mode or edge clamping"
                .to_owned(),
        ));
    }
    let roi = region_in(roi, dst)?;
    let [center_x, center_y] = center.unwrap_or([0.0, 0.0]);
    let succeeded = unsafe {
        sys::imagebufalgo::imagebufalgo_rotate(
            dst.inner_mut(),
            src.inner(),
            angle,
            center.is_some(),
            center_x,
            center_y,
            options.filter.unwrap_or_default(),
            options.filter_width.unwrap_or(0.0),
            options.recompute_region,
            &roi,
            ALL_THREADS,
        )
    };
    finish(dst, "rotate", succeeded)
}

/// Apply a 3x3 transform to the image.
///
/// `matrix` is nine values in row order, mapping source coordinates to
/// destination ones. This is the general form of [`rotate`], and unlike it,
/// every edge behaviour is available.
pub fn warp(
    dst: &mut ImageBuf,
    src: &ImageBuf,
    matrix: &[f32; 9],
    options: &WarpOptions<'_>,
    roi: Option<Roi>,
) -> Result<()> {
    let roi = region_in(roi, dst)?;
    let succeeded = unsafe {
        sys::imagebufalgo::imagebufalgo_warp(
            dst.inner_mut(),
            src.inner(),
            matrix,
            options.filter.unwrap_or_default(),
            options.filter_width.unwrap_or(0.0),
            options.wrap.unwrap_or_default(),
            options.edge_clamp,
            options.recompute_region,
            &roi,
            ALL_THREADS,
        )
    };
    finish(dst, "warp", succeeded)
}

/// Warp by a map that says, per pixel, where to read from.
///
/// `coordinates` holds source positions rather than offsets: the first named
/// channel gives the x coordinate and the second the y, both normalised to
/// `0..1` across the source's display window. This is how a lens distortion or
/// an optical flow is applied.
///
/// `channels` names those two channels of `coordinates`, and `flip` mirrors
/// each of them, for maps authored with the opposite convention.
///
/// [`WarpOptions::wrap`], [`WarpOptions::edge_clamp`] and
/// [`WarpOptions::recompute_region`] have no effect here; the map decides where
/// every pixel comes from, so there is nothing left for them to say.
pub fn st_warp(
    dst: &mut ImageBuf,
    src: &ImageBuf,
    coordinates: &ImageBuf,
    channels: [u32; 2],
    flip: [bool; 2],
    options: &WarpOptions<'_>,
    roi: Option<Roi>,
) -> Result<()> {
    let index = |name: &'static str, value: u32| {
        i32::try_from(value)
            .map_err(|_| Error::InvalidImageSpec(format!("{name} exceeds i32::MAX")))
    };
    let roi = region_in(roi, dst)?;
    let succeeded = unsafe {
        sys::imagebufalgo::imagebufalgo_st_warp(
            dst.inner_mut(),
            src.inner(),
            coordinates.inner(),
            options.filter.unwrap_or_default(),
            options.filter_width.unwrap_or(0.0),
            index("s channel", channels[0])?,
            index("t channel", channels[1])?,
            flip[0],
            flip[1],
            &roi,
            ALL_THREADS,
        )
    };
    finish(dst, "st_warp", succeeded)
}

/// Either an image or one constant value per channel.
///
/// This is OpenImageIO's `Image_or_Const`, which several operations accept in
/// place of a second image. Where a constant is given, too few values repeats
/// the last one across the remaining channels, and an empty slice means zero —
/// except in [`clamp`] and [`ContrastRemap`], which say what empty means for
/// them.
#[derive(Debug, Clone, Copy)]
pub enum Operand<'a> {
    /// Take this operand's values from an image.
    Image(&'a ImageBuf),
    /// Use these constants, one per channel.
    Constant(&'a [f32]),
}

/// Multiply and add: `dst = a * b + c`, per pixel and per channel.
///
/// This is one operation rather than a multiply followed by an add, so it
/// rounds once. `a` is always an image, which satisfies OpenImageIO's rule
/// that at least one of the first two arguments must be one.
///
/// ```no_run
/// use oiio::{algo, algo::Operand, ImageBuf, ImageSpec, PixelFormat};
///
/// # fn main() -> oiio::Result<()> {
/// # let spec = ImageSpec::new(64, 64, 3, PixelFormat::F32)?;
/// # let source = ImageBuf::new(&spec)?;
/// let mut result = ImageBuf::new(&spec)?;
/// // Scale by 2 and lift by 0.1, in one pass.
/// algo::mad(
///     &mut result,
///     &source,
///     Operand::Constant(&[2.0]),
///     Operand::Constant(&[0.1]),
///     None,
/// )?;
/// # Ok(())
/// # }
/// ```
pub fn mad(
    dst: &mut ImageBuf,
    a: &ImageBuf,
    b: Operand<'_>,
    c: Operand<'_>,
    roi: Option<Roi>,
) -> Result<()> {
    let roi = region_in(roi, dst)?;
    let succeeded = match (b, c) {
        (Operand::Image(b), Operand::Image(c)) => unsafe {
            sys::imagebufalgo::imagebufalgo_mad_iii(
                dst.inner_mut(),
                a.inner(),
                b.inner(),
                c.inner(),
                &roi,
                ALL_THREADS,
            )
        },
        (Operand::Image(b), Operand::Constant(c)) => unsafe {
            sys::imagebufalgo::imagebufalgo_mad_iic(
                dst.inner_mut(),
                a.inner(),
                b.inner(),
                c,
                &roi,
                ALL_THREADS,
            )
        },
        (Operand::Constant(b), Operand::Image(c)) => unsafe {
            sys::imagebufalgo::imagebufalgo_mad_ici(
                dst.inner_mut(),
                a.inner(),
                b,
                c.inner(),
                &roi,
                ALL_THREADS,
            )
        },
        (Operand::Constant(b), Operand::Constant(c)) => unsafe {
            sys::imagebufalgo::imagebufalgo_mad_icc(
                dst.inner_mut(),
                a.inner(),
                b,
                c,
                &roi,
                ALL_THREADS,
            )
        },
    };
    finish(dst, "multiply and add", succeeded)
}

/// Compute `1 - a`.
///
/// Every channel in the region is inverted, alpha included, so restrict the
/// region to the colour channels unless that is what you want. For
/// premultiplied images, [`unpremult`] then `invert` then [`premult`] is the
/// usual sequence.
pub fn invert(dst: &mut ImageBuf, a: &ImageBuf, roi: Option<Roi>) -> Result<()> {
    let roi = region_in(roi, dst)?;
    let succeeded = unsafe {
        sys::imagebufalgo::imagebufalgo_invert(dst.inner_mut(), a.inner(), &roi, ALL_THREADS)
    };
    finish(dst, "invert", succeeded)
}

/// Raise each channel to a per-channel power.
///
/// An empty `exponents` means an exponent of zero for every channel, and so a
/// result of one everywhere; that is OpenImageIO's padding rule rather than an
/// oversight. A negative value raised to a fractional power is `NaN`, not
/// zero.
pub fn pow(dst: &mut ImageBuf, a: &ImageBuf, exponents: &[f32], roi: Option<Roi>) -> Result<()> {
    let roi = region_in(roi, dst)?;
    let succeeded = unsafe {
        sys::imagebufalgo::imagebufalgo_pow(
            dst.inner_mut(),
            a.inner(),
            exponents,
            &roi,
            ALL_THREADS,
        )
    };
    finish(dst, "power", succeeded)
}

/// Clamp each channel into a per-channel range.
///
/// An empty `min` or `max` means "do not clamp on that side" — unlike most of
/// the per-channel slices here, where empty means zero. Too few values repeats
/// the last. No check is made that `min <= max`: an inverted range silently
/// yields `max`.
///
/// `clamp_alpha_to_unit` additionally holds the source's alpha channel in
/// `0..=1`, and does nothing if the source does not designate one.
pub fn clamp(
    dst: &mut ImageBuf,
    src: &ImageBuf,
    min: &[f32],
    max: &[f32],
    clamp_alpha_to_unit: bool,
    roi: Option<Roi>,
) -> Result<()> {
    let roi = region_in(roi, dst)?;
    let succeeded = unsafe {
        sys::imagebufalgo::imagebufalgo_clamp(
            dst.inner_mut(),
            src.inner(),
            min,
            max,
            clamp_alpha_to_unit,
            &roi,
            ALL_THREADS,
        )
    };
    finish(dst, "clamp", succeeded)
}

/// Take the smaller of two operands, per pixel and per channel.
///
/// Channels present in only one of two images are copied through unchanged.
pub fn min(dst: &mut ImageBuf, a: &ImageBuf, b: Operand<'_>, roi: Option<Roi>) -> Result<()> {
    let roi = region_in(roi, dst)?;
    let succeeded = match b {
        Operand::Image(b) => unsafe {
            sys::imagebufalgo::imagebufalgo_min_images(
                dst.inner_mut(),
                a.inner(),
                b.inner(),
                &roi,
                ALL_THREADS,
            )
        },
        Operand::Constant(values) => unsafe {
            sys::imagebufalgo::imagebufalgo_min_constant(
                dst.inner_mut(),
                a.inner(),
                values,
                &roi,
                ALL_THREADS,
            )
        },
    };
    finish(dst, "minimum", succeeded)
}

/// Take the larger of two operands, per pixel and per channel.
///
/// Two images must have the same number of channels, and the destination must
/// have at least as many. [`min`] has no such restriction; the difference is
/// not by design. OpenImageIO's image-against-image `max` widens its channel
/// range where `min` narrows it, which makes it read past the shorter input
/// and write past a narrower destination — its own assertion, in code the
/// widening makes unreachable, says the range was meant to narrow. Rather than
/// pass that on, this refuses the shapes that would run off the end. The
/// bug is in OpenImageIO 3.1 and still in 3.2, so the restriction stands until
/// it is fixed there.
pub fn max(dst: &mut ImageBuf, a: &ImageBuf, b: Operand<'_>, roi: Option<Roi>) -> Result<()> {
    let roi = region_in(roi, dst)?;
    let succeeded = match b {
        Operand::Image(b) => unsafe {
            sys::imagebufalgo::imagebufalgo_max_images(
                dst.inner_mut(),
                a.inner(),
                b.inner(),
                &roi,
                ALL_THREADS,
            )
        },
        Operand::Constant(values) => unsafe {
            sys::imagebufalgo::imagebufalgo_max_constant(
                dst.inner_mut(),
                a.inner(),
                values,
                &roi,
                ALL_THREADS,
            )
        },
    };
    finish(dst, "maximum", succeeded)
}

/// How [`contrast_remap`] should reshape the tone curve.
///
/// Every field is one value per channel. An empty slice takes that field's
/// default, which is the value named below rather than zero; too few values
/// repeats the last one.
#[derive(Debug, Clone, Copy, Default)]
pub struct ContrastRemap<'a> {
    /// The input level that becomes [`min`](Self::min). Default 0.
    pub black: &'a [f32],
    /// The input level that becomes [`max`](Self::max). Default 1.
    pub white: &'a [f32],
    /// The output level [`black`](Self::black) maps to. Default 0.
    pub min: &'a [f32],
    /// The output level [`white`](Self::white) maps to. Default 1.
    pub max: &'a [f32],
    /// Steepness of an S-curve applied between the two remappings. Default 1,
    /// which is no curve at all; larger values increase contrast.
    pub sigmoid_contrast: &'a [f32],
    /// Where that S-curve pivots. Default 0.5.
    pub sigmoid_threshold: &'a [f32],
}

/// Remap levels, optionally through an S-curve.
///
/// The result is not clamped, so follow it with [`clamp`] if the output has to
/// stay in range.
///
/// Setting a channel's `black` equal to its `white` is a division by zero. When
/// *every* channel has them equal, OpenImageIO takes that as a deliberate
/// binary threshold and produces `min` below the level and `max` at or above
/// it; when only some channels do, those channels produce infinities instead.
pub fn contrast_remap(
    dst: &mut ImageBuf,
    src: &ImageBuf,
    remap: &ContrastRemap<'_>,
    roi: Option<Roi>,
) -> Result<()> {
    let roi = region_in(roi, dst)?;
    let succeeded = unsafe {
        sys::imagebufalgo::imagebufalgo_contrast_remap(
            dst.inner_mut(),
            src.inner(),
            remap.black,
            remap.white,
            remap.min,
            remap.max,
            remap.sigmoid_contrast,
            remap.sigmoid_threshold,
            &roi,
            ALL_THREADS,
        )
    };
    finish(dst, "contrast remap", succeeded)
}

/// Move three consecutive channels toward or away from their luminance.
///
/// `scale` is 0 for fully desaturated, 1 for unchanged, and more than 1 to
/// oversaturate. Channels outside `first_channel..first_channel + 3` are
/// copied unaltered. The luminance weights are fixed at linear sRGB's, whatever
/// the image's actual colour space.
///
/// The source must have three channels at `first_channel`, and neither its
/// alpha nor its depth channel may be among them.
pub fn saturate(
    dst: &mut ImageBuf,
    src: &ImageBuf,
    scale: f32,
    first_channel: u32,
    roi: Option<Roi>,
) -> Result<()> {
    let first_channel = i32::try_from(first_channel)
        .map_err(|_| Error::InvalidImageSpec("first channel exceeds i32::MAX".to_owned()))?;
    let roi = region_in(roi, dst)?;
    let succeeded = unsafe {
        sys::imagebufalgo::imagebufalgo_saturate(
            dst.inner_mut(),
            src.inner(),
            scale,
            first_channel,
            &roi,
            ALL_THREADS,
        )
    };
    finish(dst, "saturate", succeeded)
}

/// Copy part of one image into another at a given position.
///
/// `src_roi` selects part of the **source**, not of the destination; the
/// module documentation lists the six operations that do. The destination
/// position is an offset applied to the source's own coordinates, so a source
/// whose data window begins at x=100 pasted at `x = 0` lands at x=100, not at
/// the origin.
///
/// Pixels and channels that fall outside the destination are dropped silently,
/// so a paste can be partial without reporting anything.
///
/// `first_channel` may be negative, which lands a later source channel on the
/// destination's first. It may not be negative enough to put every source
/// channel outside the destination: OpenImageIO would size the destination
/// from a negative channel count and terminate the process.
pub fn paste(
    dst: &mut ImageBuf,
    position: [i32; 3],
    first_channel: i32,
    src: &ImageBuf,
    src_roi: Option<Roi>,
) -> Result<()> {
    let src_roi = region(src_roi);
    let succeeded = unsafe {
        sys::imagebufalgo::imagebufalgo_paste(
            dst.inner_mut(),
            position[0],
            position[1],
            position[2],
            first_channel,
            src.inner(),
            &src_roi,
            ALL_THREADS,
        )
    };
    finish(dst, "paste", succeeded)
}

/// Extract a region and move it to the origin.
///
/// This is [`crop`] followed by a reposition: the result's display window
/// covers exactly the extracted rectangle, starting at 0,0.
///
/// As with `crop`, and unlike the rest of this module, the destination is
/// discarded before anything is written, so pre-allocating one does not choose
/// the output format — the result always takes the source's.
///
/// A channel range narrows which channels survive but does not renumber them:
/// the result keeps the source's channel count with the others blacked out.
/// Use [`channels`] to actually drop channels. Deep images are supported.
pub fn cut(dst: &mut ImageBuf, src: &ImageBuf, roi: Option<Roi>) -> Result<()> {
    let roi = region_in(roi, dst)?;
    let succeeded = unsafe {
        sys::imagebufalgo::imagebufalgo_cut(dst.inner_mut(), src.inner(), &roi, ALL_THREADS)
    };
    finish(dst, "cut", succeeded)
}

/// Take the absolute value of every channel.
pub fn abs(dst: &mut ImageBuf, a: &ImageBuf, roi: Option<Roi>) -> Result<()> {
    let roi = region_in(roi, dst)?;
    let succeeded = unsafe {
        sys::imagebufalgo::imagebufalgo_abs(dst.inner_mut(), a.inner(), &roi, ALL_THREADS)
    };
    finish(dst, "absolute value", succeeded)
}

/// Compute the absolute difference between two images.
pub fn absdiff(dst: &mut ImageBuf, a: &ImageBuf, b: &ImageBuf, roi: Option<Roi>) -> Result<()> {
    let roi = region_in(roi, dst)?;
    let succeeded = unsafe {
        sys::imagebufalgo::imagebufalgo_absdiff_images(
            dst.inner_mut(),
            a.inner(),
            b.inner(),
            &roi,
            ALL_THREADS,
        )
    };
    finish(dst, "absolute difference", succeeded)
}

/// Copy pixels, optionally converting them to another format.
pub fn copy(
    dst: &mut ImageBuf,
    src: &ImageBuf,
    convert_to: Option<crate::PixelFormat>,
    roi: Option<Roi>,
) -> Result<()> {
    let roi = region_in(roi, dst)?;
    let convert = convert_to.unwrap_or(crate::PixelFormat::Other).to_sys();
    let succeeded = unsafe {
        sys::imagebufalgo::imagebufalgo_copy(
            dst.inner_mut(),
            src.inner(),
            convert,
            &roi,
            ALL_THREADS,
        )
    };
    finish(dst, "copy", succeeded)
}

/// Crop to a region, keeping the pixels' original coordinates.
///
/// The destination is discarded first, so unlike most of this module it cannot
/// be pre-allocated to choose the result's pixel format; the result takes the
/// source's. [`cut`] additionally moves the result to the origin.
pub fn crop(dst: &mut ImageBuf, src: &ImageBuf, roi: Option<Roi>) -> Result<()> {
    let roi = region_in(roi, dst)?;
    let succeeded = unsafe {
        sys::imagebufalgo::imagebufalgo_crop(dst.inner_mut(), src.inner(), &roi, ALL_THREADS)
    };
    finish(dst, "crop", succeeded)
}

/// Mirror vertically, top to bottom.
pub fn flip(dst: &mut ImageBuf, src: &ImageBuf, roi: Option<Roi>) -> Result<()> {
    let roi = region_in(roi, dst)?;
    let succeeded = unsafe {
        sys::imagebufalgo::imagebufalgo_flip(dst.inner_mut(), src.inner(), &roi, ALL_THREADS)
    };
    finish(dst, "flip", succeeded)
}

/// Mirror horizontally, left to right.
pub fn flop(dst: &mut ImageBuf, src: &ImageBuf, roi: Option<Roi>) -> Result<()> {
    let roi = region_in(roi, dst)?;
    let succeeded = unsafe {
        sys::imagebufalgo::imagebufalgo_flop(dst.inner_mut(), src.inner(), &roi, ALL_THREADS)
    };
    finish(dst, "flop", succeeded)
}

/// Transpose, exchanging rows and columns.
pub fn transpose(dst: &mut ImageBuf, src: &ImageBuf, roi: Option<Roi>) -> Result<()> {
    let roi = region_in(roi, dst)?;
    let succeeded = unsafe {
        sys::imagebufalgo::imagebufalgo_transpose(dst.inner_mut(), src.inner(), &roi, ALL_THREADS)
    };
    finish(dst, "transpose", succeeded)
}

/// How [`fit`] uses the space when the aspect ratios differ.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[non_exhaustive]
pub enum FitMode {
    /// Preserve the aspect ratio, padding the leftover space.
    #[default]
    Letterbox,
    /// Match the requested width, letting the height fall where it may.
    Width,
    /// Match the requested height, letting the width fall where it may.
    Height,
}

impl FitMode {
    fn name(self) -> &'static str {
        match self {
            Self::Letterbox => "letterbox",
            Self::Width => "width",
            Self::Height => "height",
        }
    }
}

/// Where one output channel's values come from in [`channels`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ChannelSource {
    /// Copy an input channel by index.
    Channel(u32),
    /// Fill with a constant instead of copying anything.
    Constant(f32),
}

/// Resize into the destination's own dimensions.
///
/// The destination decides the output size, so it must be allocated at the
/// wanted size — an empty buffer has nothing to resize into. `filter_name` is
/// an OpenImageIO filter such as `"lanczos3"`, `"blackman-harris"` or
/// `"box"`; `None` lets OpenImageIO pick one suited to the scale factor.
pub fn resize(
    dst: &mut ImageBuf,
    src: &ImageBuf,
    filter_name: Option<&str>,
    filter_width: Option<f32>,
    roi: Option<Roi>,
) -> Result<()> {
    let roi = region_in(roi, dst)?;
    let succeeded = unsafe {
        sys::imagebufalgo::imagebufalgo_resize(
            dst.inner_mut(),
            src.inner(),
            filter_name.unwrap_or(""),
            filter_width.unwrap_or(0.0),
            &roi,
            ALL_THREADS,
        )
    };
    finish(dst, "resize", succeeded)
}

/// Resize to fit inside the destination, preserving the aspect ratio.
pub fn fit(
    dst: &mut ImageBuf,
    src: &ImageBuf,
    filter_name: Option<&str>,
    filter_width: Option<f32>,
    mode: FitMode,
    exact: bool,
    roi: Option<Roi>,
) -> Result<()> {
    let roi = region_in(roi, dst)?;
    let succeeded = unsafe {
        sys::imagebufalgo::imagebufalgo_fit(
            dst.inner_mut(),
            src.inner(),
            filter_name.unwrap_or(""),
            filter_width.unwrap_or(0.0),
            mode.name(),
            exact,
            &roi,
            ALL_THREADS,
        )
    };
    finish(dst, "fit", succeeded)
}

/// Resize without a reconstruction filter.
///
/// Cheaper than [`resize`] and correspondingly cruder: `interpolate` chooses
/// between bilinear sampling and nearest neighbour.
pub fn resample(
    dst: &mut ImageBuf,
    src: &ImageBuf,
    interpolate: bool,
    roi: Option<Roi>,
) -> Result<()> {
    let roi = region_in(roi, dst)?;
    let succeeded = unsafe {
        sys::imagebufalgo::imagebufalgo_resample(
            dst.inner_mut(),
            src.inner(),
            interpolate,
            &roi,
            ALL_THREADS,
        )
    };
    finish(dst, "resample", succeeded)
}

/// Composite `a` over `b`, the Porter-Duff operator.
///
/// Both images must have an alpha channel and hold premultiplied values.
pub fn over(dst: &mut ImageBuf, a: &ImageBuf, b: &ImageBuf, roi: Option<Roi>) -> Result<()> {
    let roi = region_in(roi, dst)?;
    let succeeded = unsafe {
        sys::imagebufalgo::imagebufalgo_over(
            dst.inner_mut(),
            a.inner(),
            b.inner(),
            &roi,
            ALL_THREADS,
        )
    };
    finish(dst, "over", succeeded)
}

/// Multiply the colour channels by alpha.
pub fn premult(dst: &mut ImageBuf, src: &ImageBuf, roi: Option<Roi>) -> Result<()> {
    let roi = region_in(roi, dst)?;
    let succeeded = unsafe {
        sys::imagebufalgo::imagebufalgo_premult(dst.inner_mut(), src.inner(), &roi, ALL_THREADS)
    };
    finish(dst, "premultiply", succeeded)
}

/// Divide the colour channels by alpha, undoing [`premult`].
pub fn unpremult(dst: &mut ImageBuf, src: &ImageBuf, roi: Option<Roi>) -> Result<()> {
    let roi = region_in(roi, dst)?;
    let succeeded = unsafe {
        sys::imagebufalgo::imagebufalgo_unpremult(dst.inner_mut(), src.inner(), &roi, ALL_THREADS)
    };
    finish(dst, "unpremultiply", succeeded)
}

/// Multiply the colour channels back by alpha, undoing [`unpremult`] while
/// keeping the values unpremultiplied work produced for zero-alpha pixels.
///
/// The source must have an alpha channel: unlike [`premult`], whose no-alpha
/// case is a documented copy, there is nothing to re-premultiply by, so a
/// source without one is an error.
pub fn repremult(dst: &mut ImageBuf, src: &ImageBuf, roi: Option<Roi>) -> Result<()> {
    let roi = region_in(roi, dst)?;
    let succeeded = unsafe {
        sys::imagebufalgo::imagebufalgo_repremult(dst.inner_mut(), src.inner(), &roi, ALL_THREADS)
    };
    finish(dst, "re-premultiply", succeeded)
}

/// Composite `a` over `b` using depth: the closer surface wins per pixel.
///
/// `z_zero_is_infinity` treats a depth of zero as infinitely far rather than
/// infinitely close, which is what renderers that leave empty pixels at zero
/// need. Both images need matching channel counts, and OpenImageIO finds the
/// `Z` channel by name or falls back to guessing that alpha is the last
/// channel on unmarked four-channel images — name the channels.
pub fn zover(
    dst: &mut ImageBuf,
    a: &ImageBuf,
    b: &ImageBuf,
    z_zero_is_infinity: bool,
    roi: Option<Roi>,
) -> Result<()> {
    let roi = region_in(roi, dst)?;
    let succeeded = unsafe {
        sys::imagebufalgo::imagebufalgo_zover(
            dst.inner_mut(),
            a.inner(),
            b.inner(),
            z_zero_is_infinity,
            &roi,
            ALL_THREADS,
        )
    };
    finish(dst, "zover", succeeded)
}

/// Multiply every channel of `a` by single-channel `b`, per pixel.
///
/// `b` must have exactly one channel; this is the mask or matte multiply
/// [`mul`] cannot express, since arithmetic requires matching counts.
pub fn scale(dst: &mut ImageBuf, a: &ImageBuf, b: &ImageBuf, roi: Option<Roi>) -> Result<()> {
    let roi = region_in(roi, dst)?;
    let succeeded = unsafe {
        sys::imagebufalgo::imagebufalgo_scale(
            dst.inner_mut(),
            a.inner(),
            b.inner(),
            &roi,
            ALL_THREADS,
        )
    };
    finish(dst, "scale", succeeded)
}

/// How [`fix_non_finite`] treats a `NaN` or infinite value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum NonFiniteFix {
    /// Count the bad values without changing them.
    None,
    /// Replace them with zero.
    Black,
    /// Replace them with the average of the finite neighbours in a 3x3 box,
    /// falling back to zero when every neighbour is bad too. On deep images
    /// this degrades to the zero replacement.
    Box3,
    /// Change nothing and report an error if any bad value exists.
    Error,
}

impl NonFiniteFix {
    fn to_sys(self) -> i32 {
        match self {
            Self::None => 0,
            Self::Black => 1,
            Self::Box3 => 2,
            Self::Error => 100,
        }
    }
}

/// Repair `NaN` and infinite values, returning how many pixels were touched.
///
/// For pixel formats that cannot hold a non-finite value this is a copy that
/// reports zero. The destination must be empty or share the source's pixel
/// format, since OpenImageIO walks the source at the destination's width.
pub fn fix_non_finite(
    dst: &mut ImageBuf,
    src: &ImageBuf,
    mode: NonFiniteFix,
    roi: Option<Roi>,
) -> Result<u64> {
    let roi = region_in(roi, dst)?;
    let mut pixels_fixed = 0_i64;
    let succeeded = unsafe {
        sys::imagebufalgo::imagebufalgo_fix_non_finite(
            dst.inner_mut(),
            src.inner(),
            mode.to_sys(),
            &mut pixels_fixed,
            &roi,
            ALL_THREADS,
        )
    };
    finish(dst, "fix non-finite", succeeded)?;
    Ok(pixels_fixed.max(0) as u64)
}

/// Compress the value range logarithmically above a low knee.
///
/// Values up to 0.18 pass through unchanged; everything above is folded down
/// a log curve (the Sony Imageworks coefficients OpenImageIO ships), so even
/// mid-greys move and extreme highlights land below 1.0. This is the
/// transform that lets high dynamic range imagery survive filtering that
/// would otherwise ring around highlights; [`rangeexpand`] undoes it,
/// approximately — the pair round-trips values, not bit patterns. With
/// `use_luma`, the scale factor comes from luminance so a pixel's hue
/// survives the trip.
pub fn rangecompress(
    dst: &mut ImageBuf,
    src: &ImageBuf,
    use_luma: bool,
    roi: Option<Roi>,
) -> Result<()> {
    let roi = region_in(roi, dst)?;
    let succeeded = unsafe {
        sys::imagebufalgo::imagebufalgo_rangecompress(
            dst.inner_mut(),
            src.inner(),
            use_luma,
            &roi,
            ALL_THREADS,
        )
    };
    finish(dst, "rangecompress", succeeded)
}

/// Undo [`rangecompress`], expanding the compressed highlights back out.
pub fn rangeexpand(
    dst: &mut ImageBuf,
    src: &ImageBuf,
    use_luma: bool,
    roi: Option<Roi>,
) -> Result<()> {
    let roi = region_in(roi, dst)?;
    let succeeded = unsafe {
        sys::imagebufalgo::imagebufalgo_rangeexpand(
            dst.inner_mut(),
            src.inner(),
            use_luma,
            &roi,
            ALL_THREADS,
        )
    };
    finish(dst, "rangeexpand", succeeded)
}

/// Sum the channels into a single-channel image, weighted.
///
/// There must be exactly one weight per source channel. OpenImageIO pads a
/// short list against the *destination's* channel count, which is one, and
/// then reads it across the source's channels — past the end of the caller's
/// own slice. Pass ones for an unweighted sum.
pub fn channel_sum(
    dst: &mut ImageBuf,
    src: &ImageBuf,
    weights: &[f32],
    roi: Option<Roi>,
) -> Result<()> {
    let roi = region_in(roi, dst)?;
    let succeeded = unsafe {
        sys::imagebufalgo::imagebufalgo_channel_sum(
            dst.inner_mut(),
            src.inner(),
            weights,
            &roi,
            ALL_THREADS,
        )
    };
    finish(dst, "channel sum", succeeded)
}

/// Concatenate the channels of two images into one buffer.
///
/// The result covers the union of the two data windows and holds `a`'s
/// channels followed by `b`'s — the way AOVs are merged into one layered
/// image. The destination must be empty: OpenImageIO shapes the result
/// itself and disregards both a caller's region and a pre-allocated shape.
/// Deep images are not supported.
pub fn channel_append(dst: &mut ImageBuf, a: &ImageBuf, b: &ImageBuf) -> Result<()> {
    let succeeded = unsafe {
        sys::imagebufalgo::imagebufalgo_channel_append(
            dst.inner_mut(),
            a.inner(),
            b.inner(),
            ALL_THREADS,
        )
    };
    finish(dst, "channel append", succeeded)
}

/// The per-pixel maximum across channels, as a single-channel image.
pub fn maxchan(dst: &mut ImageBuf, src: &ImageBuf, roi: Option<Roi>) -> Result<()> {
    let roi = region_in(roi, dst)?;
    let succeeded = unsafe {
        sys::imagebufalgo::imagebufalgo_maxchan(dst.inner_mut(), src.inner(), &roi, ALL_THREADS)
    };
    finish(dst, "maxchan", succeeded)
}

/// The per-pixel minimum across channels, as a single-channel image.
pub fn minchan(dst: &mut ImageBuf, src: &ImageBuf, roi: Option<Roi>) -> Result<()> {
    let roi = region_in(roi, dst)?;
    let succeeded = unsafe {
        sys::imagebufalgo::imagebufalgo_minchan(dst.inner_mut(), src.inner(), &roi, ALL_THREADS)
    };
    finish(dst, "minchan", succeeded)
}

/// Build a new channel layout: reorder, drop, duplicate, or add channels.
///
/// Each entry of `sources` produces one output channel, so the output has
/// `sources.len()` channels. `names`, when given, must have the same length.
///
/// ```no_run
/// use oiio::algo::{self, ChannelSource};
/// # use oiio::{ImageBuf, ImageSpec, PixelFormat};
/// # fn main() -> oiio::Result<()> {
/// # let spec = ImageSpec::new(4, 4, 3, PixelFormat::F32)?;
/// # let rgb = ImageBuf::new(&spec)?;
/// // RGB to BGRA, with a fully opaque alpha that the source lacks.
/// let mut bgra = ImageBuf::empty()?;
/// algo::channels(
///     &mut bgra,
///     &rgb,
///     &[
///         ChannelSource::Channel(2),
///         ChannelSource::Channel(1),
///         ChannelSource::Channel(0),
///         ChannelSource::Constant(1.0),
///     ],
///     Some(&["B", "G", "R", "A"]),
/// )?;
/// # Ok(())
/// # }
/// ```
pub fn channels(
    dst: &mut ImageBuf,
    src: &ImageBuf,
    sources: &[ChannelSource],
    names: Option<&[&str]>,
) -> Result<()> {
    if sources.is_empty() {
        return Err(Error::InvalidImageSpec(
            "a channel layout needs at least one channel".to_owned(),
        ));
    }
    if let Some(names) = names {
        if names.len() != sources.len() {
            return Err(Error::InvalidImageSpec(format!(
                "expected {} channel names, got {}",
                sources.len(),
                names.len()
            )));
        }
    }

    // OpenImageIO takes two parallel arrays: an index per output channel, or
    // -1 to mean "take the constant at this position instead".
    let mut order = Vec::with_capacity(sources.len());
    let mut values = Vec::with_capacity(sources.len());
    for source in sources {
        match *source {
            ChannelSource::Channel(index) => {
                order.push(i32::try_from(index).map_err(|_| {
                    Error::InvalidImageSpec("channel index exceeds i32::MAX".to_owned())
                })?);
                values.push(0.0);
            }
            ChannelSource::Constant(value) => {
                order.push(-1);
                values.push(value);
            }
        }
    }
    let names: Vec<String> = names
        .unwrap_or(&[])
        .iter()
        .map(|name| (*name).to_owned())
        .collect();

    let succeeded = unsafe {
        sys::imagebufalgo::imagebufalgo_channels(
            dst.inner_mut(),
            src.inner(),
            order.len() as i32,
            &order,
            &values,
            &names,
            false,
            ALL_THREADS,
        )
    };
    finish(dst, "channel layout", succeeded)
}

/// Convert between colour spaces.
///
/// The names must be ones the active configuration knows; ask a
/// [`ColorConfig`](crate::ColorConfig) rather than guessing, since they differ
/// between configurations. A role such as `"scene_linear"` works wherever a
/// space name does.
///
/// `unpremult` divides out alpha before converting and multiplies it back
/// afterwards, which is what you want for images holding premultiplied
/// colour: a colour transform is not linear, so applying it to premultiplied
/// values darkens the edges of anything partly transparent.
pub fn color_convert(
    dst: &mut ImageBuf,
    src: &ImageBuf,
    from_space: &str,
    to_space: &str,
    unpremult: bool,
    roi: Option<Roi>,
) -> Result<()> {
    if from_space.is_empty() || to_space.is_empty() {
        return Err(Error::InvalidImageSpec(
            "a colour conversion needs both a source and a destination space".to_owned(),
        ));
    }
    let roi = region_in(roi, dst)?;
    let succeeded = unsafe {
        sys::imagebufalgo::imagebufalgo_colorconvert(
            dst.inner_mut(),
            src.inner(),
            from_space,
            to_space,
            unpremult,
            &roi,
            ALL_THREADS,
        )
    };
    finish(dst, "colour convert", succeeded)
}

/// Compare two images numerically.
///
/// `fail_threshold` and `warn_threshold` are per-channel absolute differences.
/// The result reports the mean and maximum error, where the worst pixel is,
/// and how many values exceeded each threshold.
///
/// This fails rather than reporting nonsense when the comparison cannot be
/// made — one image deep and the other flat, or either holding no pixels. In
/// those cases OpenImageIO returns a result whose measurements were never
/// assigned, so there is nothing to hand back.
pub fn compare(
    a: &ImageBuf,
    b: &ImageBuf,
    fail_threshold: f32,
    warn_threshold: f32,
    roi: Option<Roi>,
) -> Result<CompareSummary> {
    let roi = region_in(roi, a)?;
    let mut message = String::new();
    let summary = unsafe {
        sys::imagebufalgo::imagebufalgo_compare(
            a.inner(),
            b.inner(),
            fail_threshold,
            warn_threshold,
            &roi,
            ALL_THREADS,
            &mut message,
        )
    };
    if message.is_empty() {
        Ok(summary)
    } else {
        Err(Error::operation("compare", message))
    }
}
