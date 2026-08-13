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
