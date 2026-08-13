use std::path::Path;

use crate::imageio::validate_buffer_len;
use crate::{path_to_utf8, pixel, sys, Error, ImageSpec, Pixel, PixelFormat, Result, Roi};

/// Where an [`ImageBuf`]'s pixels live.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Storage {
    /// No pixels yet: the buffer names a file it has not read.
    Uninitialized,
    /// Pixels the buffer allocated and owns.
    Local,
    /// Pixels belonging to the caller, which the buffer only points at.
    App,
    /// Pixels served on demand by an [`ImageCache`](crate::ImageCache).
    Cache,
}

impl Storage {
    fn from_sys(storage: sys::imagebuf::IBStorage) -> Self {
        match storage {
            sys::imagebuf::IBStorage::LOCALBUFFER => Self::Local,
            sys::imagebuf::IBStorage::APPBUFFER => Self::App,
            sys::imagebuf::IBStorage::IMAGECACHE => Self::Cache,
            _ => Self::Uninitialized,
        }
    }
}

/// An image held in memory, the unit OpenImageIO's algorithms operate on.
///
/// A buffer is either allocated from a specification or attached to a file.
/// A file-backed buffer does not read anything until asked, so opening one is
/// cheap:
///
/// ```no_run
/// use oiio::ImageBuf;
/// use std::path::Path;
///
/// # fn main() -> oiio::Result<()> {
/// let mut image = ImageBuf::from_path(Path::new("image.exr"))?;
/// let spec = image.spec()?;          // metadata only, no pixels yet
/// image.read()?;                     // now the pixels
///
/// let roi = spec.data_window()?;
/// let mut pixels = vec![0.0_f32; roi.element_count()?];
/// image.get_pixels_into(roi, &mut pixels)?;
/// # Ok(())
/// # }
/// ```
pub struct ImageBuf {
    inner: cxx::UniquePtr<sys::imagebuf::ImageBuf>,
}

impl ImageBuf {
    /// An image with no specification and no pixels yet.
    ///
    /// This is what to pass as the destination of an
    /// [`algo`](crate::algo) operation that should decide the result's shape
    /// for itself — [`transpose`](crate::algo::transpose) exchanges the
    /// dimensions, and [`copy`](crate::algo::copy) can change the pixel
    /// format. Handing those an already-allocated destination makes them write
    /// into it as it is, keeping its size and format.
    pub fn empty() -> Result<Self> {
        Self::from_inner(sys::imagebuf::imagebuf_default(), "create image buffer")
    }

    /// Allocate an image described by `spec`, with every pixel set to zero.
    pub fn new(spec: &ImageSpec) -> Result<Self> {
        let native_spec = spec.to_sys()?;
        let Some(native_spec) = native_spec.as_ref() else {
            return Err(Error::InvalidImageSpec(
                "OpenImageIO could not allocate an image specification".to_owned(),
            ));
        };
        let inner = sys::imagebuf::imagebuf_new_from_spec(
            native_spec,
            sys::imagebuf::InitializePixels::Yes,
        );
        Self::from_inner(inner, "allocate image buffer")
    }

    /// Attach to an image file without reading its pixels.
    ///
    /// The specification becomes available immediately;
    /// [`ImageBuf::read`] fetches the pixels.
    pub fn from_path(image_path: &Path) -> Result<Self> {
        Self::from_path_at(image_path, 0, 0)
    }

    /// Attach to one subimage and mip level of an image file.
    pub fn from_path_at(image_path: &Path, subimage: u32, mip_level: u32) -> Result<Self> {
        let filename = path_to_utf8(image_path)?;
        let subimage = level_index(subimage)?;
        let mip_level = level_index(mip_level)?;

        // SAFETY: a null config and a null ioproxy request the defaults, and
        // a null shared cache asks OpenImageIO to use its own.
        let inner = unsafe {
            sys::imagebuf::imagebuf_new_from_file(
                filename,
                subimage,
                mip_level,
                cxx::SharedPtr::null(),
                std::ptr::null(),
                std::ptr::null_mut(),
            )
        };
        let mut buffer = Self::from_inner(inner, "open image buffer")?;

        // Resolve the specification now so a missing or unreadable file is
        // reported here rather than at the first pixel access.
        if !sys::imagebuf::imagebuf_init_spec(buffer.inner_mut(), filename, subimage, mip_level) {
            return Err(Error::OpenImage {
                path: image_path.to_path_buf(),
                message: buffer.take_error(),
            });
        }
        Ok(buffer)
    }

    /// Read the pixels of the attached file.
    pub fn read(&mut self) -> Result<()> {
        self.read_at(0, 0, None)
    }

    /// Read one subimage and mip level, optionally converting the pixels.
    ///
    /// Passing `None` keeps the file's own format.
    pub fn read_at(
        &mut self,
        subimage: u32,
        mip_level: u32,
        convert_to: Option<PixelFormat>,
    ) -> Result<()> {
        let subimage = level_index(subimage)?;
        let mip_level = level_index(mip_level)?;
        let convert = convert_to.unwrap_or(PixelFormat::Other).to_sys();
        let succeeded =
            sys::imagebuf::imagebuf_read(self.inner_mut(), subimage, mip_level, true, convert);
        self.check("read image buffer", succeeded)
    }

    /// The image's specification.
    pub fn spec(&self) -> Result<ImageSpec> {
        ImageSpec::from_sys(sys::imagebuf::imagebuf_spec(self.inner()))
    }

    /// The specification of the file on disk, before any conversion.
    pub fn native_spec(&self) -> Result<ImageSpec> {
        ImageSpec::from_sys(sys::imagebuf::imagebuf_nativespec(self.inner()))
    }

    /// Where this buffer's pixels live.
    pub fn storage(&self) -> Storage {
        Storage::from_sys(sys::imagebuf::imagebuf_storage(self.inner()))
    }

    /// Whether the buffer has a specification yet.
    pub fn is_initialized(&self) -> bool {
        sys::imagebuf::imagebuf_initialized(self.inner())
    }

    /// The file name this buffer is attached to, empty when it has none.
    pub fn name(&self) -> &str {
        sys::imagebuf::imagebuf_name(self.inner())
    }

    /// The format the attached file is stored in, empty when there is none.
    pub fn file_format_name(&self) -> &str {
        sys::imagebuf::imagebuf_file_format_name(self.inner())
    }

    /// Number of subimages in the attached file.
    pub fn subimage_count(&self) -> i32 {
        sys::imagebuf::imagebuf_nsubimages(self.inner())
    }

    /// Number of mip levels in the current subimage.
    pub fn mip_level_count(&self) -> i32 {
        sys::imagebuf::imagebuf_nmiplevels(self.inner())
    }

    /// Number of channels per pixel.
    pub fn channel_count(&self) -> i32 {
        sys::imagebuf::imagebuf_nchannels(self.inner())
    }

    /// Copy a region out into a contiguous buffer.
    ///
    /// The destination length must exactly equal `roi.element_count()`, and
    /// values are converted to `T` if the image holds another format.
    pub fn get_pixels_into<T: Pixel>(&self, roi: Roi, pixels: &mut [T]) -> Result<()> {
        validate_buffer_len(roi.element_count()?, pixels.len())?;
        let sys_roi = roi.to_sys();

        // SAFETY: Pixel is sealed to initialized scalar layouts, and the shim
        // re-derives the layout from the region before writing anything.
        let succeeded = unsafe {
            sys::imagebuf::imagebuf_get_pixels_span(
                self.inner(),
                &sys_roi,
                pixel::type_desc::<T>(),
                pixel::as_bytes_mut(pixels),
            )
        };
        if succeeded {
            Ok(())
        } else {
            Err(Error::operation(
                "read image buffer pixels",
                sys::imagebuf::imagebuf_geterror(self.inner(), true),
            ))
        }
    }

    /// Copy a contiguous buffer into a region.
    ///
    /// The source length must exactly equal `roi.element_count()`.
    pub fn set_pixels<T: Pixel>(&mut self, roi: Roi, pixels: &[T]) -> Result<()> {
        validate_buffer_len(roi.element_count()?, pixels.len())?;
        let sys_roi = roi.to_sys();

        // SAFETY: as in `get_pixels_into`.
        let succeeded = unsafe {
            sys::imagebuf::imagebuf_set_pixels_span(
                self.inner_mut(),
                &sys_roi,
                pixel::type_desc::<T>(),
                pixel::as_bytes(pixels),
            )
        };
        self.check("write image buffer pixels", succeeded)
    }

    /// Write the image to a file, keeping its current pixel format.
    pub fn write(&mut self, image_path: &Path) -> Result<()> {
        self.write_as(image_path, None)
    }

    /// Write the image to a file in a chosen pixel format.
    pub fn write_as(&mut self, image_path: &Path, format: Option<PixelFormat>) -> Result<()> {
        let filename = path_to_utf8(image_path)?;
        let dtype = format.unwrap_or(PixelFormat::Other).to_sys();
        let succeeded = sys::imagebuf::imagebuf_write(self.inner_mut(), filename, dtype, "");
        self.check("write image buffer", succeeded)
    }

    /// Ensure the pixels are held locally and writable.
    ///
    /// A cache-backed buffer is read into its own storage; one that is already
    /// local is left alone.
    pub fn make_writable(&mut self) -> Result<()> {
        let succeeded = sys::imagebuf::imagebuf_make_writeable(self.inner_mut(), true);
        self.check("make image buffer writable", succeeded)
    }

    /// Whether this buffer holds a deep image, where each pixel is a list of
    /// samples rather than one value per channel.
    ///
    /// A buffer becomes deep by being built from a deep
    /// [`ImageSpec`](ImageSpec::as_deep), or by being read from a deep file.
    pub fn is_deep(&self) -> bool {
        sys::imagebuf::imagebuf_deep(self.inner())
    }

    /// How many samples one pixel holds.
    ///
    /// Zero for a pixel nothing was written to, and for any pixel of a flat
    /// image.
    pub fn deep_sample_count(&self, x: i32, y: i32) -> u32 {
        sys::imagebuf::imagebuf_deep_samples(self.inner(), x, y, 0).max(0) as u32
    }

    /// Set how many samples one pixel holds.
    ///
    /// Growing a pixel leaves the new samples zeroed; shrinking discards the
    /// ones past the new end.
    pub fn set_deep_sample_count(&mut self, x: i32, y: i32, count: u32) -> Result<()> {
        self.require_deep("set deep sample count")?;
        let count = i32::try_from(count).map_err(|_| {
            Error::InvalidImageSpec("deep sample count exceeds i32::MAX".to_owned())
        })?;
        sys::imagebuf::imagebuf_set_deep_samples(self.inner_mut(), x, y, 0, count);
        Ok(())
    }

    /// One sample's value, as a float.
    pub fn deep_value(&self, x: i32, y: i32, channel: u32, sample: u32) -> Result<f32> {
        let (channel, sample) = self.deep_index("read deep value", x, y, channel, sample)?;
        Ok(sys::imagebuf::imagebuf_deep_value(
            self.inner(),
            x,
            y,
            0,
            channel,
            sample,
        ))
    }

    /// One sample's value, as an unsigned integer.
    ///
    /// Use this for a channel whose type is unsigned; reading it as a float
    /// would round values a float cannot hold exactly.
    pub fn deep_value_uint(&self, x: i32, y: i32, channel: u32, sample: u32) -> Result<u32> {
        let (channel, sample) = self.deep_index("read deep value", x, y, channel, sample)?;
        Ok(sys::imagebuf::imagebuf_deep_value_uint(
            self.inner(),
            x,
            y,
            0,
            channel,
            sample,
        ))
    }

    /// Set one sample's value from a float.
    pub fn set_deep_value(
        &mut self,
        x: i32,
        y: i32,
        channel: u32,
        sample: u32,
        value: f32,
    ) -> Result<()> {
        let (channel, sample) = self.deep_index("write deep value", x, y, channel, sample)?;
        sys::imagebuf::imagebuf_set_deep_value(self.inner_mut(), x, y, 0, channel, sample, value);
        Ok(())
    }

    /// Set one sample's value from an unsigned integer.
    pub fn set_deep_value_uint(
        &mut self,
        x: i32,
        y: i32,
        channel: u32,
        sample: u32,
        value: u32,
    ) -> Result<()> {
        let (channel, sample) = self.deep_index("write deep value", x, y, channel, sample)?;
        sys::imagebuf::imagebuf_set_deep_value_uint(
            self.inner_mut(),
            x,
            y,
            0,
            channel,
            sample,
            value,
        );
        Ok(())
    }

    fn require_deep(&self, operation: &'static str) -> Result<()> {
        if self.is_deep() {
            Ok(())
        } else {
            Err(Error::operation(
                operation,
                "this image is not deep; build it from ImageSpec::as_deep".to_owned(),
            ))
        }
    }

    /// Validate a channel and sample index before it reaches C++.
    ///
    /// OpenImageIO answers an out-of-range index with a null pointer and then
    /// either reads zero or drops the write, both silently, so the check has to
    /// happen here.
    fn deep_index(
        &self,
        operation: &'static str,
        x: i32,
        y: i32,
        channel: u32,
        sample: u32,
    ) -> Result<(i32, i32)> {
        self.require_deep(operation)?;
        let channels = self.channel_count().max(0) as u32;
        if channel >= channels {
            return Err(Error::operation(
                operation,
                format!("channel {channel} is outside the image's {channels}"),
            ));
        }
        let samples = self.deep_sample_count(x, y);
        if sample >= samples {
            return Err(Error::operation(
                operation,
                format!("sample {sample} is outside the {samples} at {x},{y}"),
            ));
        }
        let to_i32 = |value: u32| {
            i32::try_from(value)
                .map_err(|_| Error::InvalidImageSpec("deep index exceeds i32::MAX".to_owned()))
        };
        Ok((to_i32(channel)?, to_i32(sample)?))
    }

    fn from_inner(
        inner: cxx::UniquePtr<sys::imagebuf::ImageBuf>,
        operation: &'static str,
    ) -> Result<Self> {
        if inner.is_null() {
            return Err(Error::operation(
                operation,
                "OpenImageIO returned a null image buffer".to_owned(),
            ));
        }
        Ok(Self { inner })
    }

    pub(crate) fn take_error(&self) -> String {
        sys::imagebuf::imagebuf_geterror(self.inner(), true)
    }

    fn check(&mut self, operation: &'static str, succeeded: bool) -> Result<()> {
        if succeeded {
            Ok(())
        } else {
            Err(Error::operation(operation, self.take_error()))
        }
    }

    pub(crate) fn inner(&self) -> &sys::imagebuf::ImageBuf {
        self.inner
            .as_ref()
            .expect("ImageBuf invariant violated: null native pointer")
    }

    pub(crate) fn inner_mut(&mut self) -> std::pin::Pin<&mut sys::imagebuf::ImageBuf> {
        self.inner
            .as_mut()
            .expect("ImageBuf invariant violated: null native pointer")
    }
}

impl Clone for ImageBuf {
    fn clone(&self) -> Self {
        Self {
            inner: sys::imagebuf::imagebuf_clone(self.inner()),
        }
    }
}

impl std::fmt::Debug for ImageBuf {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ImageBuf")
            .field("name", &self.name())
            .field("storage", &self.storage())
            .field("channels", &self.channel_count())
            .finish_non_exhaustive()
    }
}

fn level_index(index: u32) -> Result<i32> {
    i32::try_from(index)
        .map_err(|_| Error::InvalidImageSpec("image level index exceeds i32::MAX".to_owned()))
}
