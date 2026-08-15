//! Safe Rust bindings to OpenImageIO 3.1.
//!
//! Read and write every format OpenImageIO supports, in `u8`, `u16`,
//! [`f16`](struct@f16) or `f32`. Dimensions, regions and buffer lengths are
//! all checked before anything crosses into C++, so a mistake is a typed
//! error rather than undefined behaviour.
//!
//! # Reading
//!
//! ```no_run
//! use oiio::ImageInput;
//! use std::path::Path;
//!
//! # fn main() -> oiio::Result<()> {
//! let mut input = ImageInput::from_path(Path::new("image.exr"))?;
//! let spec = input.image_spec()?;
//! let mut pixels = vec![0.0_f32; spec.element_count()?];
//! input.read_image_into(&mut pixels)?;
//! input.close()?;
//! # Ok(())
//! # }
//! ```
//!
//! Part of an image is a [`Roi`], which selects pixels *and* channels, so
//! reading one AOV out of a 38-channel EXR does not decode the other 37:
//!
//! ```no_run
//! # use oiio::ImageInput;
//! # use std::path::Path;
//! # fn main() -> oiio::Result<()> {
//! # let mut input = ImageInput::from_path(Path::new("image.exr"))?;
//! # let spec = input.image_spec()?;
//! let roi = spec.data_window()?.with_y(0..64)?.with_channels(0..3)?;
//! let mut pixels = vec![0.0_f32; roi.element_count()?];
//! input.read_region_into(roi, &mut pixels)?;
//! # Ok(())
//! # }
//! ```
//!
//! # What else is here
//!
//! - [`ImageOutput`] writes files, whole or in scanline and tile pieces,
//!   with subimages, mip levels and multi-part files.
//! - [`ImageCache`] serves regions from many files under a memory budget,
//!   with [`ImageHandle`] to skip repeated name lookups and [`TileGuard`] to
//!   borrow one tile at a time.
//! - [`ImageBuf`] holds an image in memory and [`algo`] operates on it:
//!   arithmetic, compositing, resizing, channel shuffling and colour
//!   conversion.
//! - [`DeepImage`] reads deep files, where each pixel holds a list of
//!   samples rather than one value.
//! - [`ColorConfig`] reports which colour spaces the active OpenColorIO
//!   configuration defines, so conversions need not guess at names.
//! - [`make_texture`] writes the tiled, MIP-mapped files a renderer wants,
//!   and [`TextureSystem`] does the filtered lookups into them.
//! - Neither reading nor writing needs the filesystem:
//!   [`ImageInput::from_memory`] and [`ImageOutput::to_memory`] work on
//!   buffers.
//!
//! # Metadata
//!
//! Attributes arrive as [`AttributeValue`]. Integers, floats and strings are
//! modelled directly; everything else keeps the bytes OpenImageIO stored, so
//! an attribute this crate does not understand still survives being read from
//! one image and written to another.

#![warn(missing_docs)]

use oiio_sys as sys;

pub mod algo;

mod attribute;
mod color;
mod deep;
mod error;
mod image_buf;
mod image_cache;
mod image_spec;
mod imageio;
mod pixel;
mod pixel_format;
mod roi;
mod texture;

pub use attribute::AttributeValue;
pub use color::ColorConfig;
pub use deep::{DeepChannel, DeepImage};
pub use error::{Error, Result};
pub use half::f16;
pub use image_buf::{ImageBuf, Storage, Wrap};
pub use image_cache::{ImageCache, ImageCacheBuilder, ImageHandle, Perthread, TileGuard};
pub use image_spec::ImageSpec;
pub use imageio::{ImageInput, ImageOutput};
pub use pixel::Pixel;
pub use pixel_format::PixelFormat;
pub use roi::Roi;
pub use texture::{
    make_texture, make_texture_from_buffer, Derivatives, InterpolationMode, MipMode, TextureConfig,
    TextureMode, TextureOptions, TextureSystem, UdimInventory, WrapMode,
};

/// Backwards-compatible name for errors returned by `ImageInput`.
pub type ImageInputIoError = Error;

pub(crate) fn path_to_utf8(path: &std::path::Path) -> Result<&str> {
    path.to_str()
        .ok_or_else(|| Error::NonUtf8Path(path.to_path_buf()))
}
