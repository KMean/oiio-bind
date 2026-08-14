use std::path::Path;

use crate::imageio::validate_buffer_len;
use crate::{path_to_utf8, pixel, sys, Error, ImageSpec, Pixel, PixelFormat, Result, Roi};

/// What a point read outside the data window answers with.
///
/// A closed set on purpose: OpenImageIO's iterator dispatches on the wrap
/// mode through a table whose out-of-range entries are unchecked in release
/// builds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Wrap {
    /// Black, which is OpenImageIO's default.
    Default,
    /// Zero for every channel.
    Black,
    /// The nearest edge pixel.
    Clamp,
    /// The image tiled over the plane. Needs a display window with positive
    /// size, whose dimensions the coordinate is folded by.
    Periodic,
    /// The image reflected at each edge. Needs a positive display window
    /// too.
    Mirror,
}

impl Wrap {
    fn to_sys(self) -> sys::imagebuf::WrapMode {
        match self {
            Self::Default => sys::imagebuf::WrapMode::WrapDefault,
            Self::Black => sys::imagebuf::WrapMode::WrapBlack,
            Self::Clamp => sys::imagebuf::WrapMode::WrapClamp,
            Self::Periodic => sys::imagebuf::WrapMode::WrapPeriodic,
            Self::Mirror => sys::imagebuf::WrapMode::WrapMirror,
        }
    }
}

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
    ///
    /// A specification larger than the machine can allocate is an error here.
    /// OpenImageIO does not propagate the failure on its own: it records the
    /// error, leaves the buffer with no pixels, and the caller that would find
    /// out is the zero-fill, which asserts and then divides by zero. The shim
    /// allocates and zeroes in two steps so the failure can be returned.
    pub fn new(spec: &ImageSpec) -> Result<Self> {
        // A deep image's samples are indexed by an int inside DeepData::init,
        // so a spec with more pixels than that can hold is refused here rather
        // than allowed to truncate.
        if spec.is_deep() {
            let pixels = spec.pixel_count()?;
            if pixels > i32::MAX as usize {
                return Err(Error::InvalidImageSpec(format!(
                    "a deep image is limited to {} pixels, and this spec has {pixels}",
                    i32::MAX
                )));
            }
        }

        let native_spec = spec.to_sys()?;
        let Some(native_spec) = native_spec.as_ref() else {
            return Err(Error::InvalidImageSpec(
                "OpenImageIO could not allocate an image specification".to_owned(),
            ));
        };
        let mut error = String::new();
        let inner = sys::imagebuf::imagebuf_new_from_spec_checked(native_spec, &mut error);
        if inner.is_null() && !error.is_empty() {
            return Err(Error::operation("allocate image buffer", error));
        }
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

    /// Whether the pixels in memory have actually been filled in.
    ///
    /// False for a buffer whose read failed. OpenImageIO allocates the pixels
    /// before it opens the file, so a failed read leaves the allocation behind
    /// untouched; without this the buffer looks readable and hands back
    /// whatever the heap happened to contain.
    pub fn pixels_valid(&self) -> bool {
        sys::imagebuf::imagebuf_pixels_valid(self.inner())
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
        self.require_flat("read image buffer pixels")?;
        self.require_region_inside(&roi)?;
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
        if !succeeded {
            return Err(Error::operation(
                "read image buffer pixels",
                sys::imagebuf::imagebuf_geterror(self.inner(), true),
            ));
        }
        if let Err(error) = self.require_filled("read image buffer pixels") {
            // The copy has already happened, so the caller's slice is holding
            // whatever the allocation held. Returning an error is not enough on
            // its own: scrub it, so a caller who ignores the error still cannot
            // read process memory out of it.
            // SAFETY: as above; `pixels` is a slice of initialized scalars and
            // zero is a valid bit pattern for every one of them.
            pixel::as_bytes_mut(pixels).fill(0);
            return Err(error);
        }
        Ok(())
    }

    /// Copy a contiguous buffer into a region.
    ///
    /// The source length must exactly equal `roi.element_count()`.
    pub fn set_pixels<T: Pixel>(&mut self, roi: Roi, pixels: &[T]) -> Result<()> {
        self.require_flat("write image buffer pixels")?;
        self.require_region_inside(&roi)?;
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
    /// Shrinking a pixel drops the samples past the new end but keeps the room
    /// they occupied, so growing it again within that room brings their old
    /// values back rather than zeroes. Write every sample you intend to read.
    ///
    /// The storage the counts describe is allocated on the first value
    /// written, so a total too large for the machine is reported there — or
    /// here, once the samples are already allocated and this call has to
    /// grow them.
    pub fn set_deep_sample_count(&mut self, x: i32, y: i32, count: u32) -> Result<()> {
        self.require_deep("set deep sample count")?;
        // OpenImageIO's setter indexes the pixel without checking the range,
        // so an out-of-range coordinate silently resizes a different pixel —
        // one the reader, which does check, then reports as empty.
        self.require_inside("set deep sample count", x, y)?;
        let count = i32::try_from(count).map_err(|_| {
            Error::InvalidImageSpec("deep sample count exceeds i32::MAX".to_owned())
        })?;
        let mut error = String::new();
        if !sys::imagebuf::imagebuf_set_deep_samples(self.inner_mut(), x, y, 0, count, &mut error) {
            return Err(Error::operation("set deep sample count", error));
        }
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
        let mut error = String::new();
        if !sys::imagebuf::imagebuf_set_deep_value(
            self.inner_mut(),
            x,
            y,
            0,
            channel,
            sample,
            value,
            &mut error,
        ) {
            return Err(Error::operation("write deep value", error));
        }
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
        let mut error = String::new();
        if !sys::imagebuf::imagebuf_set_deep_value_uint(
            self.inner_mut(),
            x,
            y,
            0,
            channel,
            sample,
            value,
            &mut error,
        ) {
            return Err(Error::operation("write deep value", error));
        }
        Ok(())
    }

    /// Reject a coordinate outside the data window.
    fn require_inside(&self, operation: &'static str, x: i32, y: i32) -> Result<()> {
        let spec = self.spec()?;
        let [origin_x, origin_y, _] = spec.origin();
        let [width, height, _] = spec.dimensions();
        let inside = x >= origin_x
            && y >= origin_y
            && (x - origin_x) < width as i32
            && (y - origin_y) < height as i32;
        if inside {
            Ok(())
        } else {
            Err(Error::operation(
                operation,
                format!(
                    "{x},{y} is outside the data window {origin_x},{origin_y} \
                     to {},{}",
                    origin_x + width as i32,
                    origin_y + height as i32
                ),
            ))
        }
    }

    /// Refuse a deep image where flat pixels are expected.
    ///
    /// A deep `ImageBuf` reports `Storage::Local` but has no flat storage at
    /// all: `realloc` allocates zero bytes and the samples live in the
    /// `DeepData`. `ImageBuf::get_pixels` then takes its general path, and the
    /// iterator leaves `m_proxydata` null for a deep buffer because neither of
    /// the two branches that assign it applies, so `p[c]` reads or writes
    /// through a null pointer. Reading a deep EXR with `ImageBuf::read` is
    /// enough to get here.
    /// Refuse pixels that were allocated but never filled in.
    ///
    /// `ImageBufImpl::read` allocates before it opens the file, and the
    /// allocation is not zeroed. When the open or the decode then fails it
    /// records the error and clears the valid flag, but leaves the untouched
    /// allocation and the local storage in place. `ImageBuf::get_pixels` gates
    /// its fast path on `localpixels()` alone and never consults the flag, so
    /// it copied whatever the heap happened to hold into the caller's slice and
    /// reported success.
    ///
    /// This has to run afterwards rather than before. A buffer attached to a
    /// file it has not read yet is indistinguishable from one whose read
    /// failed: both are local storage with the flag clear. It is `localpixels`
    /// inside the read that performs the deferred load and sets the flag, so
    /// the flag only answers the question once the call has been made.
    fn require_filled(&self, operation: &'static str) -> Result<()> {
        if self.storage() != Storage::Local || self.pixels_valid() {
            return Ok(());
        }
        // Take whatever the failed read recorded, both to say something useful
        // and so OpenImageIO does not complain at destruction that nobody
        // collected it.
        let recorded = sys::imagebuf::imagebuf_geterror(self.inner(), true);
        let mut message =
            "the pixels were never read; the deferred read failed and left the allocation untouched"
                .to_owned();
        if !recorded.is_empty() {
            message.push_str(" (");
            message.push_str(recorded.trim_end());
            message.push(')');
        }
        Err(Error::operation(operation, message))
    }

    fn require_flat(&self, operation: &'static str) -> Result<()> {
        if self.is_deep() {
            Err(Error::operation(
                operation,
                "this image is deep; use DeepImage or ImageBuf::deep_value \
                 to reach its samples"
                    .to_owned(),
            ))
        } else {
            Ok(())
        }
    }

    /// Refuse a region the image does not have, on any axis.
    ///
    /// The channel axis is the one that traps. `ImageBuf::get_pixels` clamps
    /// `chend` to the channel count but never `chbegin`, so a range starting at
    /// or past the end leaves a zero or negative channel count; `ROI::contains`
    /// still passes, so it takes the fast path into `copy_image`, which divides
    /// the pixel size by that count and then memcpys with it.
    ///
    /// The spatial axes are quieter and just as wrong. A region outside the
    /// data window reads as zeros, because the iterator's default wrap is
    /// `WrapBlack`, so the caller cannot tell an absent region from a black
    /// one; and `set_pixels` skips every pixel that does not exist and still
    /// reports success, so the write silently goes nowhere. A `chend` past the
    /// channel count is worse than either: the strides were computed from the
    /// range the caller asked for, so OpenImageIO writes the real channels at
    /// the wide stride and every remaining slot keeps whatever the caller's
    /// buffer already held.
    ///
    /// `ImageInput::read_region_into` has always rejected these, so this is
    /// also what stops the two halves of the crate from disagreeing.
    fn require_region_inside(&self, roi: &Roi) -> Result<()> {
        roi.validate_within(&self.spec()?)
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

    /// Which subimage this buffer currently represents.
    ///
    /// Meaningful for a buffer attached to a file; a buffer built from a
    /// specification reports zero, and several operations that rewrite the
    /// buffer reset it to zero.
    pub fn subimage(&self) -> u32 {
        sys::imagebuf::imagebuf_subimage(self.inner()).max(0) as u32
    }

    /// Which mip level this buffer currently represents.
    pub fn mip_level(&self) -> u32 {
        sys::imagebuf::imagebuf_miplevel(self.inner()).max(0) as u32
    }

    /// How many threads operations on this buffer may use; zero means
    /// OpenImageIO's global default.
    pub fn threads(&self) -> u32 {
        sys::imagebuf::imagebuf_threads(self.inner()).max(0) as u32
    }

    /// Set the thread count for operations on this buffer; zero restores
    /// OpenImageIO's global default.
    pub fn set_threads(&mut self, count: u32) -> Result<()> {
        let count = i32::try_from(count)
            .map_err(|_| Error::InvalidImageSpec("thread count exceeds i32::MAX".to_owned()))?;
        sys::imagebuf::imagebuf_set_threads(self.inner_mut(), count);
        Ok(())
    }

    /// One channel of one pixel, with out-of-window coordinates answered by
    /// the wrap mode. Deep buffers are refused — their pixels are sample
    /// lists, not values.
    pub fn channel_at(&self, x: i32, y: i32, channel: u32, wrap: Wrap) -> Result<f32> {
        self.require_flat("read a channel")?;
        let channel = i32::try_from(channel)
            .map_err(|_| Error::InvalidRoi("channel exceeds i32::MAX".to_owned()))?;
        self.require_wrappable(wrap)?;
        Ok(sys::imagebuf::imagebuf_getchannel(
            self.inner(),
            x,
            y,
            0,
            channel,
            wrap.to_sys(),
        ))
    }

    /// Every channel of one pixel, written into `values`.
    ///
    /// A slice shorter than the channel count reads that many channels; a
    /// longer one has its tail zeroed by OpenImageIO.
    pub fn pixel_at_into(&self, x: i32, y: i32, wrap: Wrap, values: &mut [f32]) -> Result<()> {
        self.require_flat("read a pixel")?;
        self.require_wrappable(wrap)?;
        let count = i32::try_from(values.len())
            .map_err(|_| Error::InvalidRoi("channel count exceeds i32::MAX".to_owned()))?;
        // SAFETY: the pointer and count describe exactly the caller's slice.
        unsafe {
            sys::imagebuf::imagebuf_getpixel(
                self.inner(),
                x,
                y,
                0,
                values.as_mut_ptr(),
                count,
                wrap.to_sys(),
            );
        }
        Ok(())
    }

    /// Set every channel of one pixel.
    ///
    /// The coordinate must lie inside the data window: OpenImageIO skips an
    /// outside write silently, which this crate reports instead.
    pub fn set_pixel_at(&mut self, x: i32, y: i32, values: &[f32]) -> Result<()> {
        self.require_flat("write a pixel")?;
        self.require_inside("write a pixel", x, y)?;
        sys::imagebuf::imagebuf_setpixel(self.inner_mut(), x, y, 0, values);
        Ok(())
    }

    /// A bilinearly interpolated pixel at a continuous coordinate, written
    /// into `values`, whose length must equal the channel count.
    pub fn interpolated_pixel_into(
        &self,
        x: f32,
        y: f32,
        wrap: Wrap,
        values: &mut [f32],
    ) -> Result<()> {
        self.require_interpolatable(wrap, values)?;
        // SAFETY: the slice holds exactly the channel count, checked above.
        unsafe {
            sys::imagebuf::imagebuf_interppixel(
                self.inner(),
                x,
                y,
                values.as_mut_ptr(),
                wrap.to_sys(),
            );
        }
        Ok(())
    }

    /// [`ImageBuf::interpolated_pixel_into`] with bicubic interpolation.
    pub fn interpolated_pixel_bicubic_into(
        &self,
        x: f32,
        y: f32,
        wrap: Wrap,
        values: &mut [f32],
    ) -> Result<()> {
        self.require_interpolatable(wrap, values)?;
        // SAFETY: the slice holds exactly the channel count, checked above.
        unsafe {
            sys::imagebuf::imagebuf_interppixel_bicubic(
                self.inner(),
                x,
                y,
                values.as_mut_ptr(),
                wrap.to_sys(),
            );
        }
        Ok(())
    }

    /// A bilinearly interpolated pixel addressed in NDC — `0..1` across the
    /// display window — written into `values`, whose length must equal the
    /// channel count. The display window must have positive size, which
    /// [`ImageBuf::set_full_window`] guarantees for windows set through it.
    pub fn interpolated_pixel_ndc_into(
        &self,
        s: f32,
        t: f32,
        wrap: Wrap,
        values: &mut [f32],
    ) -> Result<()> {
        self.require_interpolatable(wrap, values)?;
        self.require_positive_display("interpolate in NDC")?;
        // SAFETY: the slice holds exactly the channel count, checked above.
        unsafe {
            sys::imagebuf::imagebuf_interppixel_NDC(
                self.inner(),
                s,
                t,
                values.as_mut_ptr(),
                wrap.to_sys(),
            );
        }
        Ok(())
    }

    /// [`ImageBuf::interpolated_pixel_ndc_into`] with bicubic interpolation.
    pub fn interpolated_pixel_bicubic_ndc_into(
        &self,
        s: f32,
        t: f32,
        wrap: Wrap,
        values: &mut [f32],
    ) -> Result<()> {
        self.require_interpolatable(wrap, values)?;
        self.require_positive_display("interpolate in NDC")?;
        // SAFETY: the slice holds exactly the channel count, checked above.
        unsafe {
            sys::imagebuf::imagebuf_interppixel_bicubic_NDC(
                self.inner(),
                s,
                t,
                values.as_mut_ptr(),
                wrap.to_sys(),
            );
        }
        Ok(())
    }

    /// The shared preconditions of the interpolators: a flat buffer, a wrap
    /// mode this buffer supports, a slice of exactly the channel count, and
    /// a channel count OpenImageIO's per-call stack scratch can hold.
    fn require_interpolatable(&self, wrap: Wrap, values: &[f32]) -> Result<()> {
        self.require_flat("interpolate a pixel")?;
        self.require_wrappable(wrap)?;
        let channels = self.channel_count() as usize;
        if values.len() != channels {
            return Err(Error::BufferLength {
                expected: channels,
                actual: values.len(),
            });
        }
        // The interpolators alloca their scratch from the channel count on
        // OpenImageIO's stack frame; a four-figure channel count is fine, an
        // unbounded one is a stack overflow no catch can reach.
        if channels > 1024 {
            return Err(Error::operation(
                "interpolate a pixel",
                format!("interpolation supports at most 1024 channels, this image has {channels}"),
            ));
        }
        Ok(())
    }

    /// Periodic and mirror wraps divide by the display window's size.
    fn require_wrappable(&self, wrap: Wrap) -> Result<()> {
        if matches!(wrap, Wrap::Periodic | Wrap::Mirror) {
            self.require_positive_display("wrap periodically")?;
        }
        Ok(())
    }

    fn require_positive_display(&self, operation: &'static str) -> Result<()> {
        let spec = self.spec()?;
        let [width, height, _] = spec.full_dimensions();
        if width == 0 || height == 0 {
            return Err(Error::operation(
                operation,
                "the display window has zero size, which this operation divides by".to_owned(),
            ));
        }
        Ok(())
    }

    /// Move the data window's origin, carrying the pixels with it.
    ///
    /// Exclusive because OpenImageIO documents spec mutation as not
    /// thread-safe, as are the other metadata setters here.
    pub fn set_origin(&mut self, origin: [i32; 3]) {
        sys::imagebuf::imagebuf_set_origin(self.inner_mut(), origin[0], origin[1], origin[2]);
    }

    /// Set the display (full) window from an origin and a size.
    ///
    /// The size must be positive on every axis: OpenImageIO stores whatever
    /// it is given, and a zero or negative display window later divides the
    /// periodic and mirror wrap modes and the NDC mappings by it.
    pub fn set_full_window(&mut self, origin: [i32; 3], size: [u32; 3]) -> Result<()> {
        let mut end = [0_i32; 3];
        for axis in 0..3 {
            if size[axis] == 0 {
                return Err(Error::InvalidImageSpec(
                    "the display window needs a positive size on every axis".to_owned(),
                ));
            }
            let extent = i32::try_from(size[axis])
                .map_err(|_| Error::InvalidImageSpec("display window too large".to_owned()))?;
            end[axis] = origin[axis].checked_add(extent).ok_or_else(|| {
                Error::InvalidImageSpec("the display window overflows i32".to_owned())
            })?;
        }
        sys::imagebuf::imagebuf_set_full(
            self.inner_mut(),
            origin[0],
            end[0],
            origin[1],
            end[1],
            origin[2],
            end[2],
        );
        Ok(())
    }

    /// Set the display window from a region.
    ///
    /// The crate's [`Roi`] is always defined and non-empty, which is what
    /// keeps OpenImageIO's undefined-region arithmetic out of reach here.
    pub fn set_display_window(&mut self, roi: Roi) {
        let sys_roi = roi.to_sys();
        sys::imagebuf::imagebuf_set_roi_full(self.inner_mut(), &sys_roi);
    }

    /// Record the EXIF-style orientation, 1 through 8.
    pub fn set_orientation(&mut self, orientation: u32) -> Result<()> {
        if !(1..=8).contains(&orientation) {
            return Err(Error::InvalidImageSpec(format!(
                "orientation is EXIF's 1..=8, got {orientation}"
            )));
        }
        sys::imagebuf::imagebuf_set_orientation(self.inner_mut(), orientation as i32);
        Ok(())
    }

    /// Replace this buffer's metadata with a copy of another's, keeping the
    /// pixels and the geometry.
    pub fn copy_metadata(&mut self, src: &ImageBuf) {
        sys::imagebuf::imagebuf_copy_metadata(self.inner_mut(), src.inner());
    }

    /// Merge another buffer's metadata into this one's.
    ///
    /// With `override_existing`, attributes both images carry take the other
    /// image's value; otherwise existing attributes win. A non-empty
    /// `pattern` is a regular expression selecting which attribute names are
    /// merged, and an invalid pattern is an error — OpenImageIO would
    /// otherwise let the regex constructor take the process down.
    pub fn merge_metadata(
        &mut self,
        src: &ImageBuf,
        override_existing: bool,
        pattern: &str,
    ) -> Result<()> {
        let mut error = String::new();
        if !sys::imagebuf::imagebuf_merge_metadata(
            self.inner_mut(),
            src.inner(),
            override_existing,
            pattern,
            &mut error,
        ) {
            return Err(Error::operation("merge metadata", error));
        }
        Ok(())
    }

    /// Copy this buffer, reporting failure instead of handing back a copy
    /// that cannot serve pixels.
    ///
    /// OpenImageIO's copy constructor catches its own allocation failure,
    /// records an error on the copy, and returns it anyway — with the
    /// source's valid-pixels flag still set, so the first read of the broken
    /// copy would reach a division by zero or a null cache inside
    /// OpenImageIO. [`Clone`] uses this and panics on failure, which is the
    /// convention for `Clone` under memory pressure; call this directly to
    /// handle the failure instead.
    pub fn try_clone(&self) -> Result<Self> {
        let mut error = String::new();
        let inner = sys::imagebuf::imagebuf_clone_checked(self.inner(), &mut error);
        if inner.is_null() {
            return Err(Error::operation("copy image buffer", error));
        }
        Ok(Self { inner })
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
    /// Panics when the copy cannot be allocated; use
    /// [`ImageBuf::try_clone`] to handle that as an error instead.
    fn clone(&self) -> Self {
        self.try_clone()
            .expect("copying this image buffer failed; ImageBuf::try_clone reports the reason")
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
