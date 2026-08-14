use std::path::PathBuf;

/// Errors reported by the safe OpenImageIO wrappers.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    /// OpenImageIO takes UTF-8 paths, and this one is not.
    #[error("path is not valid UTF-8: {0:?}")]
    NonUtf8Path(
        /// The path as given.
        PathBuf,
    ),

    /// A file could not be opened for reading.
    #[error("could not open image {path:?}: {message}")]
    OpenImage {
        /// The file that could not be opened.
        path: PathBuf,
        /// What OpenImageIO said about it.
        message: String,
    },

    /// No writer could be created, usually because the extension names no
    /// format OpenImageIO can write.
    #[error("could not create an image writer for {path:?}: {message}")]
    CreateImage {
        /// The file that would have been written.
        path: PathBuf,
        /// What OpenImageIO said about it.
        message: String,
    },

    /// A read or write region is outside the data window, misaligned with the
    /// tile grid, or otherwise not one the format can address.
    #[error("invalid region on the {axis} axis: {message}")]
    InvalidRegion {
        /// Which axis is at fault: `"x"`, `"y"` or `"z"`.
        axis: &'static str,
        /// What is wrong with it.
        message: String,
    },

    /// An OpenImageIO call failed and reported why.
    #[error("{operation} failed: {message}")]
    Operation {
        /// What was being attempted.
        operation: &'static str,
        /// What OpenImageIO said about it.
        message: String,
    },

    /// An image specification describes something impossible, or one could
    /// not be built.
    #[error("invalid image specification: {0}")]
    InvalidImageSpec(
        /// What is wrong with it.
        String,
    ),

    /// The file has no such subimage or mip level.
    #[error("subimage or mip level is out of range: subimage {subimage}, mip level {mip_level}")]
    InvalidImageLevel {
        /// The subimage asked for.
        subimage: u32,
        /// The mip level asked for.
        mip_level: u32,
    },

    /// A region of interest is empty, reversed, or outside the image.
    #[error("invalid region of interest: {0}")]
    InvalidRoi(
        /// What is wrong with it.
        String,
    ),

    /// The buffer does not hold exactly as many values as the operation
    /// needs. Nothing was read or written.
    #[error("pixel buffer length mismatch: expected {expected} elements, got {actual}")]
    BufferLength {
        /// How many scalar values the operation needed.
        expected: usize,
        /// How many the buffer holds.
        actual: usize,
    },

    /// The dimensions multiply out to more values than can be counted.
    #[error("pixel buffer size overflow")]
    BufferSizeOverflow,

    /// A deep image cannot be read into a contiguous buffer; use
    /// [`ImageInput::read_deep_image`](crate::ImageInput::read_deep_image).
    #[error("deep images are not supported by the contiguous pixel API")]
    UnsupportedDeepImage,

    /// An [`ImageCache`](crate::ImageCache) setting is out of range.
    #[error("invalid ImageCache setting {name}: {value}")]
    InvalidCacheSetting {
        /// The setting's name.
        name: &'static str,
        /// The value that was refused.
        value: String,
    },

    /// A cached tile was asked for as a type it does not hold. A tile keeps
    /// the format the cache stores — the file's own for `uint8`, `uint16`
    /// and `half`, and `f32` for everything else — and is not converted on
    /// the way out; `TileGuard::format` reports which it is.
    #[error(
        "tile holds {actual} pixels, not {requested}; ask for the format TileGuard::format reports"
    )]
    TilePixelFormat {
        /// The type the caller asked for.
        requested: crate::PixelFormat,
        /// The type the tile actually holds.
        actual: crate::PixelFormat,
    },
}

impl Error {
    pub(crate) fn operation(operation: &'static str, message: String) -> Self {
        Self::Operation {
            operation,
            message: if message.is_empty() {
                "OpenImageIO did not provide an error message".to_owned()
            } else {
                message
            },
        }
    }
}

/// Result type used by the safe OpenImageIO wrappers.
pub type Result<T> = std::result::Result<T, Error>;
