//! Safe, focused Rust bindings to OpenImageIO 3.1.
//!
//! The high-level API owns native resources, validates image dimensions and
//! buffer lengths before crossing the C++ boundary, and supports contiguous
//! pixels of type [`u8`], [`u16`], [`struct@f16`], and [`f32`].
//!
//! Read a whole image directly:
//!
//! ```no_run
//! use oiio::ImageInput;
//! use std::path::Path;
//!
//! # fn main() -> oiio::Result<()> {
//! let mut input = ImageInput::from_path(Path::new("image.png"))?;
//! let spec = input.image_spec()?;
//! let mut pixels = vec![0_u16; spec.element_count()?];
//! input.read_image_into(&mut pixels)?;
//! input.close()?;
//! # Ok(())
//! # }
//! ```
//!
//! Or read a region through a private, thread-safe cache:
//!
//! ```no_run
//! use oiio::{ImageCache, Roi};
//! use std::path::Path;
//!
//! # fn main() -> oiio::Result<()> {
//! let cache = ImageCache::new()?;
//! let path = Path::new("image.exr");
//! let roi = Roi::new(0..64, 0..64, 0..1, 0..4)?;
//! let mut pixels = vec![0.0_f32; roi.element_count()?];
//! cache.get_pixels_into(path, roi, &mut pixels)?;
//! # Ok(())
//! # }
//! ```

use oiio_sys as sys;

mod error;
mod image_cache;
mod image_spec;
mod imageio;
mod pixel;
mod roi;

pub use error::{Error, Result};
pub use half::f16;
pub use image_cache::{ImageCache, ImageCacheBuilder};
pub use image_spec::ImageSpec;
pub use imageio::ImageInput;
pub use pixel::Pixel;
pub use roi::Roi;

/// Backwards-compatible name for errors returned by `ImageInput`.
pub type ImageInputIoError = Error;

pub(crate) fn path_to_utf8(path: &std::path::Path) -> Result<&str> {
    path.to_str()
        .ok_or_else(|| Error::NonUtf8Path(path.to_path_buf()))
}
