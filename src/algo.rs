//! Operations on [`ImageBuf`], mirroring OpenImageIO's `ImageBufAlgo`.
//!
//! Every operation writes into a destination buffer and takes an optional
//! region; `None` means the whole image, which is OpenImageIO's own default.
//! Because the destination is `&mut` and the sources are `&`, Rust rejects at
//! compile time the aliasing cases these functions are not written for.
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

fn region(roi: Option<Roi>) -> sys::imageio::ROI {
    // An undefined ROI is how OpenImageIO spells "the whole image".
    roi.map_or_else(sys::imageio::roi_default, Roi::to_sys)
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
    let roi = region(roi);
    let succeeded = sys::imagebufalgo::imagebufalgo_zero(dst.inner_mut(), &roi, ALL_THREADS);
    finish(dst, "zero", succeeded)
}

/// Fill the region with one value per channel.
///
/// Fewer values than the image has channels fills only those channels.
pub fn fill(dst: &mut ImageBuf, values: &[f32], roi: Option<Roi>) -> Result<()> {
    if values.is_empty() {
        return Err(Error::InvalidImageSpec(
            "fill needs at least one channel value".to_owned(),
        ));
    }
    let roi = region(roi);
    let succeeded =
        sys::imagebufalgo::imagebufalgo_fill(dst.inner_mut(), values, &roi, ALL_THREADS);
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
            let roi = region(roi);
            let succeeded = $images(dst.inner_mut(), a.inner(), b.inner(), &roi, ALL_THREADS);
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
            let roi = region(roi);
            let succeeded = $constants(dst.inner_mut(), a.inner(), values, &roi, ALL_THREADS);
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
    let roi = region(roi);
    let stats = sys::imagebufalgo::imagebufalgo_pixel_stats(src.inner(), &roi, ALL_THREADS);
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
    let roi = region(roi);
    let mut message = String::new();
    let counts = sys::imagebufalgo::imagebufalgo_histogram(
        src.inner(),
        channel,
        bins,
        range.start,
        range.end,
        ignore_empty,
        &roi,
        ALL_THREADS,
        &mut message,
    );
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
/// A `threshold` of zero compares the stored values exactly, in the image's own
/// format, so two `half` values that differ only after conversion to `f32` still
/// count as equal.
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
    let roi = region(roi);
    let mut message = String::new();
    let constant = sys::imagebufalgo::imagebufalgo_is_constant_color(
        src.inner(),
        threshold,
        &mut color,
        &roi,
        ALL_THREADS,
        &mut message,
    );
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
    let roi = region(roi);
    let mut message = String::new();
    let constant = sys::imagebufalgo::imagebufalgo_is_constant_channel(
        src.inner(),
        channel,
        value,
        threshold,
        &roi,
        ALL_THREADS,
        &mut message,
    );
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
    let roi = region(roi);
    let mut message = String::new();
    let monochrome = sys::imagebufalgo::imagebufalgo_is_monochrome(
        src.inner(),
        threshold,
        &roi,
        ALL_THREADS,
        &mut message,
    );
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
    let roi = region(roi);
    let mut message = String::new();
    let found = sys::imagebufalgo::imagebufalgo_nonzero_region(
        src.inner(),
        &roi,
        ALL_THREADS,
        &mut message,
    );
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
    let roi = region(roi);
    let mut message = String::new();
    let digest = sys::imagebufalgo::imagebufalgo_pixel_hash_sha1(
        src.inner(),
        extra_info,
        &roi,
        ALL_THREADS,
        &mut message,
    );
    if digest.is_empty() {
        return Err(Error::operation("pixel hash", message));
    }
    Ok(digest)
}

/// Rotate a quarter turn clockwise.
///
/// The region selects part of the **source**, not of the destination — the
/// three right-angle rotations and [`paste`] are the only operations here that
/// work that way. Prefer an empty destination: OpenImageIO installs the rotated
/// display window only when it allocates one itself, and reads a pre-allocated
/// destination's display window while writing, so one that disagrees with the
/// source's yields silently offset pixels.
pub fn rotate_90(dst: &mut ImageBuf, src: &ImageBuf, src_roi: Option<Roi>) -> Result<()> {
    let roi = region(src_roi);
    let succeeded =
        sys::imagebufalgo::imagebufalgo_rotate90(dst.inner_mut(), src.inner(), &roi, ALL_THREADS);
    finish(dst, "rotate 90", succeeded)
}

/// Rotate a half turn. See [`rotate_90`] for how the region is read.
pub fn rotate_180(dst: &mut ImageBuf, src: &ImageBuf, src_roi: Option<Roi>) -> Result<()> {
    let roi = region(src_roi);
    let succeeded =
        sys::imagebufalgo::imagebufalgo_rotate180(dst.inner_mut(), src.inner(), &roi, ALL_THREADS);
    finish(dst, "rotate 180", succeeded)
}

/// Rotate three quarters clockwise. See [`rotate_90`] for how the region is
/// read.
pub fn rotate_270(dst: &mut ImageBuf, src: &ImageBuf, src_roi: Option<Roi>) -> Result<()> {
    let roi = region(src_roi);
    let succeeded =
        sys::imagebufalgo::imagebufalgo_rotate270(dst.inner_mut(), src.inner(), &roi, ALL_THREADS);
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
    let succeeded =
        sys::imagebufalgo::imagebufalgo_reorient(dst.inner_mut(), src.inner(), ALL_THREADS);
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
            "rotate always leaves black outside the source; use warp with a              rotation matrix to choose a wrap mode or edge clamping"
                .to_owned(),
        ));
    }
    let roi = region(roi);
    let [center_x, center_y] = center.unwrap_or([0.0, 0.0]);
    let succeeded = sys::imagebufalgo::imagebufalgo_rotate(
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
    );
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
    let roi = region(roi);
    let succeeded = sys::imagebufalgo::imagebufalgo_warp(
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
    );
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
    let roi = region(roi);
    let succeeded = sys::imagebufalgo::imagebufalgo_st_warp(
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
    );
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
    let roi = region(roi);
    let succeeded = match (b, c) {
        (Operand::Image(b), Operand::Image(c)) => sys::imagebufalgo::imagebufalgo_mad_iii(
            dst.inner_mut(),
            a.inner(),
            b.inner(),
            c.inner(),
            &roi,
            ALL_THREADS,
        ),
        (Operand::Image(b), Operand::Constant(c)) => sys::imagebufalgo::imagebufalgo_mad_iic(
            dst.inner_mut(),
            a.inner(),
            b.inner(),
            c,
            &roi,
            ALL_THREADS,
        ),
        (Operand::Constant(b), Operand::Image(c)) => sys::imagebufalgo::imagebufalgo_mad_ici(
            dst.inner_mut(),
            a.inner(),
            b,
            c.inner(),
            &roi,
            ALL_THREADS,
        ),
        (Operand::Constant(b), Operand::Constant(c)) => sys::imagebufalgo::imagebufalgo_mad_icc(
            dst.inner_mut(),
            a.inner(),
            b,
            c,
            &roi,
            ALL_THREADS,
        ),
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
    let roi = region(roi);
    let succeeded =
        sys::imagebufalgo::imagebufalgo_invert(dst.inner_mut(), a.inner(), &roi, ALL_THREADS);
    finish(dst, "invert", succeeded)
}

/// Raise each channel to a per-channel power.
///
/// An empty `exponents` means an exponent of zero for every channel, and so a
/// result of one everywhere; that is OpenImageIO's padding rule rather than an
/// oversight. A negative value raised to a fractional power is `NaN`, not
/// zero.
pub fn pow(dst: &mut ImageBuf, a: &ImageBuf, exponents: &[f32], roi: Option<Roi>) -> Result<()> {
    let roi = region(roi);
    let succeeded = sys::imagebufalgo::imagebufalgo_pow(
        dst.inner_mut(),
        a.inner(),
        exponents,
        &roi,
        ALL_THREADS,
    );
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
    let roi = region(roi);
    let succeeded = sys::imagebufalgo::imagebufalgo_clamp(
        dst.inner_mut(),
        src.inner(),
        min,
        max,
        clamp_alpha_to_unit,
        &roi,
        ALL_THREADS,
    );
    finish(dst, "clamp", succeeded)
}

/// Take the smaller of two operands, per pixel and per channel.
///
/// Channels present in only one of two images are copied through unchanged.
pub fn min(dst: &mut ImageBuf, a: &ImageBuf, b: Operand<'_>, roi: Option<Roi>) -> Result<()> {
    let roi = region(roi);
    let succeeded = match b {
        Operand::Image(b) => sys::imagebufalgo::imagebufalgo_min_images(
            dst.inner_mut(),
            a.inner(),
            b.inner(),
            &roi,
            ALL_THREADS,
        ),
        Operand::Constant(values) => sys::imagebufalgo::imagebufalgo_min_constant(
            dst.inner_mut(),
            a.inner(),
            values,
            &roi,
            ALL_THREADS,
        ),
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
    let roi = region(roi);
    let succeeded = match b {
        Operand::Image(b) => sys::imagebufalgo::imagebufalgo_max_images(
            dst.inner_mut(),
            a.inner(),
            b.inner(),
            &roi,
            ALL_THREADS,
        ),
        Operand::Constant(values) => sys::imagebufalgo::imagebufalgo_max_constant(
            dst.inner_mut(),
            a.inner(),
            values,
            &roi,
            ALL_THREADS,
        ),
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
    let roi = region(roi);
    let succeeded = sys::imagebufalgo::imagebufalgo_contrast_remap(
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
    );
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
    let roi = region(roi);
    let succeeded = sys::imagebufalgo::imagebufalgo_saturate(
        dst.inner_mut(),
        src.inner(),
        scale,
        first_channel,
        &roi,
        ALL_THREADS,
    );
    finish(dst, "saturate", succeeded)
}

/// Copy part of one image into another at a given position.
///
/// `src_roi` selects part of the **source**, not of the destination — this is
/// the one region argument here that does. The destination position is an
/// offset applied to the source's own coordinates, so a source whose data
/// window begins at x=100 pasted at `x = 0` lands at x=100, not at the origin.
///
/// Pixels and channels that fall outside the destination are dropped silently,
/// so a paste can be partial without reporting anything.
pub fn paste(
    dst: &mut ImageBuf,
    position: [i32; 3],
    first_channel: i32,
    src: &ImageBuf,
    src_roi: Option<Roi>,
) -> Result<()> {
    let src_roi = region(src_roi);
    let succeeded = sys::imagebufalgo::imagebufalgo_paste(
        dst.inner_mut(),
        position[0],
        position[1],
        position[2],
        first_channel,
        src.inner(),
        &src_roi,
        ALL_THREADS,
    );
    finish(dst, "paste", succeeded)
}

/// Extract a region and move it to the origin.
///
/// This is [`crop`] followed by a reposition: the result's display window
/// covers exactly the extracted rectangle, starting at 0,0. Unlike `crop`, and
/// unlike the rest of this module, the destination is always discarded first,
/// so it cannot be pre-allocated to choose an output format — the result takes
/// the source's.
///
/// A channel range narrows which channels survive but does not renumber them:
/// the result keeps the source's channel count with the others blacked out.
/// Use [`channels`] to actually drop channels. Deep images are supported.
pub fn cut(dst: &mut ImageBuf, src: &ImageBuf, roi: Option<Roi>) -> Result<()> {
    let roi = region(roi);
    let succeeded =
        sys::imagebufalgo::imagebufalgo_cut(dst.inner_mut(), src.inner(), &roi, ALL_THREADS);
    finish(dst, "cut", succeeded)
}

/// Take the absolute value of every channel.
pub fn abs(dst: &mut ImageBuf, a: &ImageBuf, roi: Option<Roi>) -> Result<()> {
    let roi = region(roi);
    let succeeded =
        sys::imagebufalgo::imagebufalgo_abs(dst.inner_mut(), a.inner(), &roi, ALL_THREADS);
    finish(dst, "absolute value", succeeded)
}

/// Compute the absolute difference between two images.
pub fn absdiff(dst: &mut ImageBuf, a: &ImageBuf, b: &ImageBuf, roi: Option<Roi>) -> Result<()> {
    let roi = region(roi);
    let succeeded = sys::imagebufalgo::imagebufalgo_absdiff_images(
        dst.inner_mut(),
        a.inner(),
        b.inner(),
        &roi,
        ALL_THREADS,
    );
    finish(dst, "absolute difference", succeeded)
}

/// Copy pixels, optionally converting them to another format.
pub fn copy(
    dst: &mut ImageBuf,
    src: &ImageBuf,
    convert_to: Option<crate::PixelFormat>,
    roi: Option<Roi>,
) -> Result<()> {
    let roi = region(roi);
    let convert = convert_to.unwrap_or(crate::PixelFormat::Other).to_sys();
    let succeeded = sys::imagebufalgo::imagebufalgo_copy(
        dst.inner_mut(),
        src.inner(),
        convert,
        &roi,
        ALL_THREADS,
    );
    finish(dst, "copy", succeeded)
}

/// Crop to a region, keeping the pixels' original coordinates.
pub fn crop(dst: &mut ImageBuf, src: &ImageBuf, roi: Option<Roi>) -> Result<()> {
    let roi = region(roi);
    let succeeded =
        sys::imagebufalgo::imagebufalgo_crop(dst.inner_mut(), src.inner(), &roi, ALL_THREADS);
    finish(dst, "crop", succeeded)
}

/// Mirror vertically, top to bottom.
pub fn flip(dst: &mut ImageBuf, src: &ImageBuf, roi: Option<Roi>) -> Result<()> {
    let roi = region(roi);
    let succeeded =
        sys::imagebufalgo::imagebufalgo_flip(dst.inner_mut(), src.inner(), &roi, ALL_THREADS);
    finish(dst, "flip", succeeded)
}

/// Mirror horizontally, left to right.
pub fn flop(dst: &mut ImageBuf, src: &ImageBuf, roi: Option<Roi>) -> Result<()> {
    let roi = region(roi);
    let succeeded =
        sys::imagebufalgo::imagebufalgo_flop(dst.inner_mut(), src.inner(), &roi, ALL_THREADS);
    finish(dst, "flop", succeeded)
}

/// Transpose, exchanging rows and columns.
pub fn transpose(dst: &mut ImageBuf, src: &ImageBuf, roi: Option<Roi>) -> Result<()> {
    let roi = region(roi);
    let succeeded =
        sys::imagebufalgo::imagebufalgo_transpose(dst.inner_mut(), src.inner(), &roi, ALL_THREADS);
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
    let roi = region(roi);
    let succeeded = sys::imagebufalgo::imagebufalgo_resize(
        dst.inner_mut(),
        src.inner(),
        filter_name.unwrap_or(""),
        filter_width.unwrap_or(0.0),
        &roi,
        ALL_THREADS,
    );
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
    let roi = region(roi);
    let succeeded = sys::imagebufalgo::imagebufalgo_fit(
        dst.inner_mut(),
        src.inner(),
        filter_name.unwrap_or(""),
        filter_width.unwrap_or(0.0),
        mode.name(),
        exact,
        &roi,
        ALL_THREADS,
    );
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
    let roi = region(roi);
    let succeeded = sys::imagebufalgo::imagebufalgo_resample(
        dst.inner_mut(),
        src.inner(),
        interpolate,
        &roi,
        ALL_THREADS,
    );
    finish(dst, "resample", succeeded)
}

/// Composite `a` over `b`, the Porter-Duff operator.
///
/// Both images must have an alpha channel and hold premultiplied values.
pub fn over(dst: &mut ImageBuf, a: &ImageBuf, b: &ImageBuf, roi: Option<Roi>) -> Result<()> {
    let roi = region(roi);
    let succeeded = sys::imagebufalgo::imagebufalgo_over(
        dst.inner_mut(),
        a.inner(),
        b.inner(),
        &roi,
        ALL_THREADS,
    );
    finish(dst, "over", succeeded)
}

/// Multiply the colour channels by alpha.
pub fn premult(dst: &mut ImageBuf, src: &ImageBuf, roi: Option<Roi>) -> Result<()> {
    let roi = region(roi);
    let succeeded =
        sys::imagebufalgo::imagebufalgo_premult(dst.inner_mut(), src.inner(), &roi, ALL_THREADS);
    finish(dst, "premultiply", succeeded)
}

/// Divide the colour channels by alpha, undoing [`premult`].
pub fn unpremult(dst: &mut ImageBuf, src: &ImageBuf, roi: Option<Roi>) -> Result<()> {
    let roi = region(roi);
    let succeeded =
        sys::imagebufalgo::imagebufalgo_unpremult(dst.inner_mut(), src.inner(), &roi, ALL_THREADS);
    finish(dst, "unpremultiply", succeeded)
}

/// Sum the channels into a single-channel image, optionally weighted.
pub fn channel_sum(
    dst: &mut ImageBuf,
    src: &ImageBuf,
    weights: &[f32],
    roi: Option<Roi>,
) -> Result<()> {
    let roi = region(roi);
    let succeeded = sys::imagebufalgo::imagebufalgo_channel_sum(
        dst.inner_mut(),
        src.inner(),
        weights,
        &roi,
        ALL_THREADS,
    );
    finish(dst, "channel sum", succeeded)
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

    let succeeded = sys::imagebufalgo::imagebufalgo_channels(
        dst.inner_mut(),
        src.inner(),
        order.len() as i32,
        &order,
        &values,
        &names,
        false,
        ALL_THREADS,
    );
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
    let roi = region(roi);
    let succeeded = sys::imagebufalgo::imagebufalgo_colorconvert(
        dst.inner_mut(),
        src.inner(),
        from_space,
        to_space,
        unpremult,
        &roi,
        ALL_THREADS,
    );
    finish(dst, "colour convert", succeeded)
}

/// Compare two images numerically.
///
/// `fail_threshold` and `warn_threshold` are per-channel absolute differences.
/// The result reports the mean and maximum error, where the worst pixel is,
/// and how many values exceeded each threshold.
pub fn compare(
    a: &ImageBuf,
    b: &ImageBuf,
    fail_threshold: f32,
    warn_threshold: f32,
    roi: Option<Roi>,
) -> CompareSummary {
    let roi = region(roi);
    sys::imagebufalgo::imagebufalgo_compare(
        a.inner(),
        b.inner(),
        fail_threshold,
        warn_threshold,
        &roi,
        ALL_THREADS,
    )
}
