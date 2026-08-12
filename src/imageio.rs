use std::path::Path;

use crate::{path_to_utf8, pixel, sys, Error, ImageSpec, Pixel, Result};

/// An open image file.
pub struct ImageInput(cxx::UniquePtr<sys::imageio::ImageInput>);

impl ImageInput {
    /// Open an image file.
    pub fn from_path(image_path: &Path) -> Result<Self> {
        let image_path_str = path_to_utf8(image_path)?;

        match sys::imageio::imageinput_open_without_config(image_path_str) {
            Ok(imageinput) if !imageinput.is_null() => Ok(Self(imageinput)),
            Ok(_) => {
                let message = if sys::imageio::has_error() {
                    sys::imageio::get_error(true)
                } else {
                    "OpenImageIO did not provide an error message".to_owned()
                };
                Err(Error::OpenImage {
                    path: image_path.to_path_buf(),
                    message,
                })
            }
            Err(error) => Err(Error::OpenImage {
                path: image_path.to_path_buf(),
                message: error.to_string(),
            }),
        }
    }

    /// Return the input plugin's format name.
    pub fn format_name(&self) -> &str {
        sys::imageio::imageinput_format_name(self.inner())
    }

    /// Return an owned description of the currently selected image.
    pub fn image_spec(&self) -> Result<ImageSpec> {
        ImageSpec::from_sys(sys::imageio::imageinput_spec(self.inner()))
    }

    /// Return an owned description of a subimage and mip level.
    pub fn image_spec_at(&mut self, subimage: u32, mip_level: u32) -> Result<ImageSpec> {
        let subimage_i32 = level_index(subimage)?;
        let mip_level_i32 = level_index(mip_level)?;
        let spec = sys::imageio::imageinput_spec_subimage_miplevel(
            self.inner_mut(),
            subimage_i32,
            mip_level_i32,
        );
        let Some(spec) = spec.as_ref() else {
            let _ = sys::imageio::imageinput_geterror(self.inner_mut());
            return Err(Error::InvalidImageLevel {
                subimage,
                mip_level,
            });
        };
        if !sys::imageio::imagespec_valid(spec) {
            let _ = sys::imageio::imageinput_geterror(self.inner_mut());
            return Err(Error::InvalidImageLevel {
                subimage,
                mip_level,
            });
        }
        ImageSpec::from_sys(spec)
    }

    /// Read all channels of the base image into a contiguous scalar buffer.
    ///
    /// The buffer length must exactly equal
    /// `width * height * depth * channels`. No call into C++ is made if the
    /// length is wrong or the multiplication overflows.
    pub fn read_image_into<T: Pixel>(&mut self, pixels: &mut [T]) -> Result<()> {
        self.read_image_into_at(0, 0, pixels)
    }

    /// Read all channels of a subimage and mip level into a contiguous buffer.
    pub fn read_image_into_at<T: Pixel>(
        &mut self,
        subimage: u32,
        mip_level: u32,
        pixels: &mut [T],
    ) -> Result<()> {
        let spec = self.image_spec_at(subimage, mip_level)?;
        if spec.is_deep() {
            return Err(Error::UnsupportedDeepImage);
        }
        let expected = spec.element_count()?;
        validate_buffer_len(expected, pixels.len())?;

        let channel_end = i32::try_from(spec.channel_count()).map_err(|_| {
            Error::InvalidImageSpec("channel count does not fit in an i32".to_owned())
        })?;
        // SAFETY: Pixel is sealed to initialized scalar layouts whose type,
        // alignment, element count, and byte extent were validated above.
        let succeeded = unsafe {
            sys::imageio::imageinput_read_image_span(
                self.inner_mut(),
                level_index(subimage)?,
                level_index(mip_level)?,
                0,
                channel_end,
                pixel::type_desc::<T>(),
                pixel::as_bytes_mut(pixels),
            )
        };
        if succeeded {
            Ok(())
        } else {
            Err(self.take_error("read image"))
        }
    }

    /// Close the file and report any delayed format or I/O errors.
    ///
    /// Dropping an `ImageInput` without calling this method still releases
    /// all native resources, but cannot report close errors.
    pub fn close(mut self) -> Result<()> {
        if sys::imageio::imageinput_close(self.inner_mut()) {
            Ok(())
        } else {
            Err(self.take_error("close image"))
        }
    }

    /// Get the current thread count.
    pub fn threads(&self) -> i32 {
        sys::imageio::imageinput_threads(self.inner())
    }

    /// Set the thread count. Zero requests OpenImageIO's default.
    pub fn set_threads(&mut self, threads: i32) {
        sys::imageio::imageinput_set_threads(self.inner_mut(), threads);
    }

    fn inner(&self) -> &sys::imageio::ImageInput {
        self.0
            .as_ref()
            .expect("ImageInput invariant violated: null native pointer")
    }

    fn inner_mut(&mut self) -> std::pin::Pin<&mut sys::imageio::ImageInput> {
        self.0
            .as_mut()
            .expect("ImageInput invariant violated: null native pointer")
    }

    fn take_error(&mut self, operation: &'static str) -> Error {
        Error::operation(
            operation,
            sys::imageio::imageinput_geterror(self.inner_mut()),
        )
    }
}

fn level_index(index: u32) -> Result<i32> {
    i32::try_from(index)
        .map_err(|_| Error::InvalidImageSpec("image level index exceeds i32::MAX".to_owned()))
}

pub(crate) fn validate_buffer_len(expected: usize, actual: usize) -> Result<()> {
    if expected == actual {
        Ok(())
    } else {
        Err(Error::BufferLength { expected, actual })
    }
}
