use std::path::Path;

use crate::{
    imageio::validate_buffer_len, path_to_utf8, pixel, sys, Error, ImageSpec, Pixel, Result,
};

/// An open image file being written.
pub struct ImageOutput(cxx::UniquePtr<sys::imageio::ImageOutput>);

impl ImageOutput {
    /// Create and open a two-dimensional scanline image for writing.
    ///
    /// `T` is the channel type stored in the output file. [`Self::write_image`]
    /// may receive a different [`Pixel`] type, which OpenImageIO converts to
    /// the storage type.
    pub fn create<T: Pixel>(
        image_path: &Path,
        width: u32,
        height: u32,
        channels: u32,
    ) -> Result<Self> {
        let image_path_str = path_to_utf8(image_path)?;
        let width = output_dimension("width", width)?;
        let height = output_dimension("height", height)?;
        let channels = output_dimension("channel count", channels)?;
        validate_storage_size::<T>(width, height, channels)?;

        let spec = sys::imageio::imagespec_from_resolution_format(
            width,
            height,
            channels,
            pixel::type_desc::<T>(),
        );
        let Some(spec) = spec.as_ref() else {
            return Err(Error::InvalidImageSpec(
                "OpenImageIO could not allocate the output specification".to_owned(),
            ));
        };

        // ImageOutput::create reports failures through OIIO's thread-local
        // global error state. Clear any earlier diagnostic so a failed create
        // cannot be attributed to an unrelated operation.
        let _ = sys::imageio::get_error(true);
        let output = sys::imageio::imageoutput_create_without_ioproxy(image_path_str, "");
        if output.is_null() {
            return Err(Error::OpenImage {
                path: image_path.to_path_buf(),
                message: global_error(),
            });
        }

        let mut output = Self(output);
        if sys::imageio::imageoutput_open(
            output.inner_mut(),
            image_path_str,
            spec,
            sys::imageio::OpenMode::Create,
        ) {
            Ok(output)
        } else {
            Err(Error::OpenImage {
                path: image_path.to_path_buf(),
                message: output.take_error_message(),
            })
        }
    }

    /// Return the output plugin's format name.
    pub fn format_name(&self) -> &str {
        sys::imageio::imageoutput_format_name(self.inner())
    }

    /// Return an owned description of the open output image.
    pub fn image_spec(&self) -> Result<ImageSpec> {
        ImageSpec::from_sys(sys::imageio::imageoutput_spec(self.inner()))
    }

    /// Write the entire image from a contiguous scalar buffer.
    ///
    /// The buffer length must exactly equal
    /// `width * height * depth * channels`. No write call into C++ is made if
    /// the length is wrong or the multiplication overflows. OpenImageIO
    /// converts `T` to the storage type chosen by [`Self::create`].
    pub fn write_image<T: Pixel>(&mut self, pixels: &[T]) -> Result<()> {
        let spec = self.image_spec()?;
        if spec.is_deep() {
            return Err(Error::UnsupportedDeepImage);
        }
        validate_buffer_len(spec.element_count()?, pixels.len())?;

        // SAFETY: Pixel is sealed to initialized scalar layouts whose type,
        // alignment, element count, and byte extent were validated above.
        let succeeded = unsafe {
            sys::imageio::imageoutput_write_image_span(
                self.inner_mut(),
                pixel::type_desc::<T>(),
                pixel::as_bytes(pixels),
            )
        };
        if succeeded {
            Ok(())
        } else {
            Err(self.take_error("write image"))
        }
    }

    /// Close the file and report delayed encoding or I/O errors.
    ///
    /// Dropping an `ImageOutput` without calling this method still releases
    /// all native resources, but cannot report close errors.
    pub fn close(mut self) -> Result<()> {
        if sys::imageio::imageoutput_close(self.inner_mut()) {
            Ok(())
        } else {
            Err(self.take_error("close image"))
        }
    }

    /// Get the current thread count.
    pub fn threads(&self) -> i32 {
        sys::imageio::imageoutput_threads(self.inner())
    }

    /// Set the thread count. Zero requests OpenImageIO's default.
    pub fn set_threads(&mut self, threads: i32) {
        sys::imageio::imageoutput_set_threads(self.inner_mut(), threads);
    }

    fn inner(&self) -> &sys::imageio::ImageOutput {
        self.0
            .as_ref()
            .expect("ImageOutput invariant violated: null native pointer")
    }

    fn inner_mut(&mut self) -> std::pin::Pin<&mut sys::imageio::ImageOutput> {
        self.0
            .as_mut()
            .expect("ImageOutput invariant violated: null native pointer")
    }

    fn take_error(&self, operation: &'static str) -> Error {
        Error::operation(operation, self.take_error_message())
    }

    fn take_error_message(&self) -> String {
        let message = sys::imageio::imageoutput_geterror(self.inner(), true);
        if message.is_empty() {
            "OpenImageIO did not provide an error message".to_owned()
        } else {
            message
        }
    }
}

fn output_dimension(name: &str, value: u32) -> Result<i32> {
    if value == 0 {
        return Err(Error::InvalidImageSpec(format!(
            "{name} must be positive, got 0"
        )));
    }
    i32::try_from(value)
        .map_err(|_| Error::InvalidImageSpec(format!("{name} exceeds the OpenImageIO i32 limit")))
}

fn validate_storage_size<T: Pixel>(width: i32, height: i32, channels: i32) -> Result<()> {
    [width, height, channels]
        .into_iter()
        .try_fold(1usize, |total, dimension| {
            total.checked_mul(dimension as usize)
        })
        .and_then(|elements| elements.checked_mul(std::mem::size_of::<T>()))
        .ok_or(Error::BufferSizeOverflow)
        .map(|_| ())
}

fn global_error() -> String {
    let message = if sys::imageio::has_error() {
        sys::imageio::get_error(true)
    } else {
        String::new()
    };
    if message.is_empty() {
        "OpenImageIO did not provide an error message".to_owned()
    } else {
        message
    }
}
