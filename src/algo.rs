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
