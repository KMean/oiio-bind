use std::path::PathBuf;

/// Errors reported by the safe OpenImageIO wrappers.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    #[error("path is not valid UTF-8: {0:?}")]
    NonUtf8Path(PathBuf),

    #[error("could not open image {path:?}: {message}")]
    OpenImage { path: PathBuf, message: String },

    #[error("could not create an image writer for {path:?}: {message}")]
    CreateImage { path: PathBuf, message: String },

    #[error("invalid region on the {axis} axis: {message}")]
    InvalidRegion { axis: &'static str, message: String },

    #[error("{operation} failed: {message}")]
    Operation {
        operation: &'static str,
        message: String,
    },

    #[error("invalid image specification: {0}")]
    InvalidImageSpec(String),

    #[error("subimage or mip level is out of range: subimage {subimage}, mip level {mip_level}")]
    InvalidImageLevel { subimage: u32, mip_level: u32 },

    #[error("invalid region of interest: {0}")]
    InvalidRoi(String),

    #[error("pixel buffer length mismatch: expected {expected} elements, got {actual}")]
    BufferLength { expected: usize, actual: usize },

    #[error("pixel buffer size overflow")]
    BufferSizeOverflow,

    #[error("deep images are not supported by the contiguous pixel API")]
    UnsupportedDeepImage,

    #[error("invalid ImageCache setting {name}: {value}")]
    InvalidCacheSetting { name: &'static str, value: String },
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
