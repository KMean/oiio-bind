use std::ops::Range;
use std::path::{Path, PathBuf};

use crate::image_spec::element_count;
use crate::{path_to_utf8, pixel, sys, DeepImage, Error, ImageSpec, Pixel, Result, Roi};

/// An open image, either a file or a buffer in memory.
///
/// Field order is load-bearing: Rust drops fields in declaration order, so the
/// reader closes before the proxy it reads through, which in turn drops before
/// the memory that proxy borrows.
pub struct ImageInput {
    inner: cxx::UniquePtr<sys::imageio::ImageInput>,
    _proxy: Option<cxx::UniquePtr<sys::filesystem::IOProxy>>,
    _bytes: Option<Vec<u8>>,
}

impl ImageInput {
    /// Open an image file with configuration hints.
    ///
    /// The hint spec's attributes steer the reader before it opens:
    /// `oiio:UnassociatedAlpha` asks for alpha left unassociated,
    /// `oiio:RawColor` suppresses colour conversion in formats that would
    /// apply one, and each format documents its own. The hint spec's
    /// dimensions are ignored; only its attributes matter.
    pub fn from_path_with_config(image_path: &Path, config: &ImageSpec) -> Result<Self> {
        let image_path_str = path_to_utf8(image_path)?;
        let native = config.to_sys()?;
        let Some(native) = native.as_ref() else {
            return Err(Error::InvalidImageSpec(
                "OpenImageIO could not allocate the configuration hints".to_owned(),
            ));
        };
        match sys::imageio::imageinput_open_with_config(image_path_str, native) {
            Ok(imageinput) if !imageinput.is_null() => Ok(Self {
                inner: imageinput,
                _proxy: None,
                _bytes: None,
            }),
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
            Err(exception) => Err(Error::OpenImage {
                path: image_path.to_path_buf(),
                message: exception.what().to_owned(),
            }),
        }
    }

    /// Open an image file.
    pub fn from_path(image_path: &Path) -> Result<Self> {
        let image_path_str = path_to_utf8(image_path)?;

        match sys::imageio::imageinput_open_without_config(image_path_str) {
            Ok(imageinput) if !imageinput.is_null() => Ok(Self {
                inner: imageinput,
                _proxy: None,
                _bytes: None,
            }),
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

    /// Read an image already held in memory, without touching the filesystem.
    ///
    /// `name_hint` is not opened; it only tells OpenImageIO which reader to
    /// use, so its extension must match the bytes. The buffer is moved into
    /// the reader and released when the reader is dropped.
    ///
    /// ```
    /// use oiio::{ImageInput, ImageOutput, ImageSpec, PixelFormat};
    ///
    /// # fn main() -> oiio::Result<()> {
    /// let spec = ImageSpec::new(4, 4, 3, PixelFormat::F32)?;
    /// let pixels = vec![0.5_f32; spec.element_count()?];
    ///
    /// let mut output = ImageOutput::to_memory("image.exr", &spec)?;
    /// output.write_image(&pixels)?;
    /// let encoded = output.close_into_bytes()?;
    ///
    /// let mut input = ImageInput::from_memory("image.exr", encoded)?;
    /// let mut decoded = vec![0.0_f32; spec.element_count()?];
    /// input.read_image_into(&mut decoded)?;
    /// input.close()?;
    ///
    /// assert_eq!(decoded, pixels);
    /// # Ok(())
    /// # }
    /// ```
    pub fn from_memory(name_hint: &str, bytes: Vec<u8>) -> Result<Self> {
        // The proxy borrows the buffer, so `bytes` must outlive it. Moving the
        // Vec afterwards moves only its header, never the heap allocation the
        // proxy points at.
        // SAFETY: `bytes` is moved into the returned value below and dropped
        // after both the reader and the proxy.
        let mut proxy = unsafe { sys::filesystem::ioproxy_memreader_new(&bytes) };
        let Some(proxy_ref) = proxy.as_mut() else {
            return Err(Self::memory_error(
                name_hint,
                "OpenImageIO could not allocate a memory proxy".to_owned(),
            ));
        };

        // SAFETY: the proxy outlives the reader; both are owned by the value
        // returned here, and the reader is declared first so it drops first.
        let inner = unsafe {
            let proxy_ptr = proxy_ref.get_unchecked_mut() as *mut sys::filesystem::IOProxy;
            sys::imageio::imageinput_open_with_ioproxy(name_hint, proxy_ptr)
        };
        if inner.is_null() {
            return Err(Self::memory_error(name_hint, global_error()));
        }

        Ok(Self {
            inner,
            _proxy: Some(proxy),
            _bytes: Some(bytes),
        })
    }

    /// Return the input plugin's format name.
    pub fn format_name(&self) -> &str {
        sys::imageio::imageinput_format_name(self.inner())
    }

    fn memory_error(name_hint: &str, message: String) -> Error {
        Error::OpenImage {
            path: PathBuf::from(name_hint),
            message: if message.is_empty() {
                "OpenImageIO did not provide an error message".to_owned()
            } else {
                message
            },
        }
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

    /// Read part of the base image into a contiguous scalar buffer.
    ///
    /// See [`ImageInput::read_region_into_at`].
    pub fn read_region_into<T: Pixel>(&mut self, roi: Roi, pixels: &mut [T]) -> Result<()> {
        self.read_region_into_at(0, 0, roi, pixels)
    }

    /// Read part of a subimage and mip level into a contiguous buffer.
    ///
    /// The region selects pixels and channels, so this is also how a channel
    /// subset is read: start from [`ImageSpec::data_window`] and narrow it.
    ///
    /// ```no_run
    /// use oiio::ImageInput;
    /// use std::path::Path;
    ///
    /// # fn main() -> oiio::Result<()> {
    /// let mut input = ImageInput::from_path(Path::new("image.exr"))?;
    /// let spec = input.image_spec()?;
    ///
    /// // The first three channels of the top 64 scanlines.
    /// let roi = spec.data_window()?.with_y(0..64)?.with_channels(0..3)?;
    /// let mut pixels = vec![0.0_f32; roi.element_count()?];
    /// input.read_region_into(roi, &mut pixels)?;
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// How the region may be shaped depends on how the file stores pixels,
    /// because that is what OpenImageIO itself can address:
    ///
    /// - A tiled image is read tile by tile, so each axis must start on the
    ///   tile grid and either end on it or at the edge of the data window.
    /// - A scanline image is read a whole row at a time, so the x range must
    ///   cover the full width and the region must be a single z slice.
    ///
    /// For arbitrary regions of a tiled file, use
    /// [`ImageCache::get_pixels_into`](crate::ImageCache::get_pixels_into),
    /// which assembles them from tiles.
    pub fn read_region_into_at<T: Pixel>(
        &mut self,
        subimage: u32,
        mip_level: u32,
        roi: Roi,
        pixels: &mut [T],
    ) -> Result<()> {
        let spec = self.image_spec_at(subimage, mip_level)?;
        if spec.is_deep() {
            return Err(Error::UnsupportedDeepImage);
        }
        roi.validate_within(&spec)?;
        validate_buffer_len(roi.element_count()?, pixels.len())?;

        let subimage_i32 = level_index(subimage)?;
        let mip_level_i32 = level_index(mip_level)?;
        let channels = roi.channels();
        let channel_begin = level_index(channels.start)?;
        let channel_end = level_index(channels.end)?;
        let origin = spec.origin();
        let dimensions = spec.dimensions();

        let succeeded = if spec.is_tiled() {
            let [tile_width, tile_height, tile_depth] = spec.tile_dimensions();
            validate_tile_alignment("x", &roi.x(), origin[0], dimensions[0], tile_width)?;
            validate_tile_alignment("y", &roi.y(), origin[1], dimensions[1], tile_height)?;
            validate_tile_alignment("z", &roi.z(), origin[2], dimensions[2], tile_depth.max(1))?;

            // SAFETY: Pixel is sealed to initialized scalar layouts, and the
            // shim re-derives the layout from this level's own dimensions.
            unsafe {
                sys::imageio::imageinput_read_tiles_span(
                    self.inner_mut(),
                    subimage_i32,
                    mip_level_i32,
                    roi.x().start,
                    roi.x().end,
                    roi.y().start,
                    roi.y().end,
                    roi.z().start,
                    roi.z().end,
                    channel_begin,
                    channel_end,
                    pixel::type_desc::<T>(),
                    pixel::as_bytes_mut(pixels),
                )
            }
        } else {
            let width_end = i64::from(origin[0]) + i64::from(dimensions[0]);
            if i64::from(roi.x().start) != i64::from(origin[0])
                || i64::from(roi.x().end) != width_end
            {
                return Err(Error::InvalidRegion {
                    axis: "x",
                    message: format!(
                        "a scanline image is read a whole row at a time, so the x range must \
                         cover the full width {}..{width_end}",
                        origin[0]
                    ),
                });
            }
            if roi.depth() != 1 {
                return Err(Error::InvalidRegion {
                    axis: "z",
                    message: "a scanline image is read one z slice at a time".to_owned(),
                });
            }

            // SAFETY: as in the tiled branch.
            unsafe {
                sys::imageio::imageinput_read_scanlines_span(
                    self.inner_mut(),
                    subimage_i32,
                    mip_level_i32,
                    roi.y().start,
                    roi.y().end,
                    roi.z().start,
                    channel_begin,
                    channel_end,
                    pixel::type_desc::<T>(),
                    pixel::as_bytes_mut(pixels),
                )
            }
        };

        if succeeded {
            Ok(())
        } else {
            Err(self.take_error("read region"))
        }
    }

    /// Read a deep image, where each pixel holds a list of samples.
    ///
    /// The contiguous pixel API refuses deep files, because a fixed number of
    /// values per pixel cannot describe them. This reads one instead.
    pub fn read_deep_image(&mut self) -> Result<DeepImage> {
        self.read_deep_image_at(0, 0)
    }

    /// Read a deep subimage and mip level.
    pub fn read_deep_image_at(&mut self, subimage: u32, mip_level: u32) -> Result<DeepImage> {
        let spec = self.image_spec_at(subimage, mip_level)?;
        if !spec.is_deep() {
            return Err(Error::operation(
                "read deep image",
                "this image is not deep; use read_image_into".to_owned(),
            ));
        }

        let mut deep = sys::deepdata::deepdata_default();
        let Some(pinned) = deep.as_mut() else {
            return Err(Error::operation(
                "read deep image",
                "OpenImageIO could not allocate deep data".to_owned(),
            ));
        };
        let succeeded = sys::imageio::imageinput_read_native_deep_image(
            self.inner_mut(),
            level_index(subimage)?,
            level_index(mip_level)?,
            pinned,
        );
        if !succeeded {
            return Err(self.take_error("read deep image"));
        }

        DeepImage::from_parts(deep, &spec)
    }

    /// Read the base image in its declared native channel formats.
    ///
    /// Each channel keeps its declared storage format — the half/float mix
    /// a multi-AOV EXR carries — packed per pixel in channel order, with
    /// [`ImageSpec::native_pixel_bytes`] as the stride. For most formats
    /// that is byte-exact with the file's pixel data; formats packing
    /// samples below a byte or between byte sizes (1/2/4/12-bit TIFF,
    /// sub-byte PNM) declare the next whole type as native and hand back
    /// unpacked, rescaled samples, so the guarantee is the *declared*
    /// formats, not the on-disk bit packing. This is the read for
    /// value-preserving transcoding or decoding values yourself; for
    /// numbers to compute with, [`ImageInput::read_image_into`] converts
    /// instead.
    pub fn read_native_image(&mut self) -> Result<Vec<u8>> {
        self.read_native_image_at(0, 0)
    }

    /// Read a subimage and mip level exactly as the file stores it; see
    /// [`ImageInput::read_native_image`].
    pub fn read_native_image_at(&mut self, subimage: u32, mip_level: u32) -> Result<Vec<u8>> {
        const OPERATION: &str = "read native image";
        let spec = self.image_spec_at(subimage, mip_level)?;
        if spec.is_deep() {
            return Err(Error::UnsupportedDeepImage);
        }
        let expected = spec
            .pixel_count()?
            .checked_mul(spec.native_pixel_bytes()?)
            .ok_or_else(|| Error::InvalidImageSpec("the image's byte size overflows".to_owned()))?;

        let mut bytes = vec![0_u8; expected];
        let mut error = String::new();
        let succeeded = sys::imageio::imageinput_read_native_image_bytes(
            self.inner_mut(),
            level_index(subimage)?,
            level_index(mip_level)?,
            &mut bytes,
            &mut error,
        );
        if succeeded {
            Ok(bytes)
        } else {
            Err(Error::operation(OPERATION, error))
        }
    }

    /// Read a contiguous band of scanlines of a deep subimage and mip level.
    ///
    /// The returned deep image is the band: as wide as the data window, as
    /// tall as `y`, its origin at the band's top-left corner, every channel
    /// included. Bands may be read in any order.
    ///
    /// All channels are always read. OpenImageIO's channel-subset path pairs
    /// each kept channel's data with the wrong channel's name, so this crate
    /// does not offer it for deep reads.
    pub fn read_deep_scanlines_at(
        &mut self,
        subimage: u32,
        mip_level: u32,
        y: Range<i32>,
    ) -> Result<DeepImage> {
        const OPERATION: &str = "read deep scanlines";
        let spec = self.require_deep_spec(OPERATION, subimage, mip_level)?;
        if spec.is_tiled() {
            return Err(Error::InvalidImageSpec(
                "this file is tiled; use read_deep_tiles_at".to_owned(),
            ));
        }
        if spec.dimensions()[2] != 1 {
            return Err(Error::InvalidImageSpec(
                "scanline reads require a two-dimensional image".to_owned(),
            ));
        }
        let rows = validate_axis("y", &y, spec.origin()[1], spec.dimensions()[1])?;

        let mut deep = sys::deepdata::deepdata_default();
        let Some(pinned) = deep.as_mut() else {
            return Err(Error::operation(
                OPERATION,
                "OpenImageIO could not allocate deep data".to_owned(),
            ));
        };
        let succeeded = sys::imageio::imageinput_read_native_deep_scanlines(
            self.inner_mut(),
            level_index(subimage)?,
            level_index(mip_level)?,
            y.start,
            y.end,
            0,
            0,
            spec.channel_count() as i32,
            pinned,
        );
        if !succeeded {
            return Err(self.take_error(OPERATION));
        }

        let band = ImageSpec::new(
            spec.dimensions()[0],
            rows,
            spec.channel_count(),
            spec.format(),
        )?
        .with_origin([spec.origin()[0], y.start, spec.origin()[2]]);
        DeepImage::from_parts(deep, &band)
    }

    /// Read a rectangular block of whole tiles of a deep subimage and mip
    /// level.
    ///
    /// Each range must start on a tile boundary and either end on one or end
    /// at the edge of the data window — OpenEXR locates the block by tile
    /// index, so a misaligned range would place its samples at rows and
    /// columns outside the block. The returned deep image is the block, its
    /// origin at the block's corner, every channel included (as with
    /// [`ImageInput::read_deep_scanlines_at`], OpenImageIO's channel-subset
    /// path mislabels channels, so this crate does not offer it).
    pub fn read_deep_tiles_at(
        &mut self,
        subimage: u32,
        mip_level: u32,
        x: Range<i32>,
        y: Range<i32>,
        z: Range<i32>,
    ) -> Result<DeepImage> {
        const OPERATION: &str = "read deep tiles";
        let spec = self.require_deep_spec(OPERATION, subimage, mip_level)?;
        if !spec.is_tiled() {
            return Err(Error::InvalidImageSpec(
                "tile reads require a tiled file; use read_deep_scanlines_at".to_owned(),
            ));
        }
        let [tile_width, tile_height, tile_depth] = spec.tile_dimensions();
        let origin = spec.origin();
        let dimensions = spec.dimensions();
        let width = validate_axis("x", &x, origin[0], dimensions[0])?;
        let height = validate_axis("y", &y, origin[1], dimensions[1])?;
        let depth = validate_axis("z", &z, origin[2], dimensions[2])?;
        validate_tile_alignment("x", &x, origin[0], dimensions[0], tile_width)?;
        validate_tile_alignment("y", &y, origin[1], dimensions[1], tile_height)?;
        validate_tile_alignment("z", &z, origin[2], dimensions[2], tile_depth.max(1))?;

        let mut deep = sys::deepdata::deepdata_default();
        let Some(pinned) = deep.as_mut() else {
            return Err(Error::operation(
                OPERATION,
                "OpenImageIO could not allocate deep data".to_owned(),
            ));
        };
        let succeeded = sys::imageio::imageinput_read_native_deep_tiles(
            self.inner_mut(),
            level_index(subimage)?,
            level_index(mip_level)?,
            x.start,
            x.end,
            y.start,
            y.end,
            z.start,
            z.end,
            0,
            spec.channel_count() as i32,
            pinned,
        );
        if !succeeded {
            return Err(self.take_error(OPERATION));
        }

        let band = ImageSpec::new(width, height, spec.channel_count(), spec.format())?
            .with_depth(depth)?
            .with_origin([x.start, y.start, z.start]);
        DeepImage::from_parts(deep, &band)
    }

    /// The deep region readers' shared precondition: the subimage exists and
    /// is deep.
    fn require_deep_spec(
        &mut self,
        operation: &'static str,
        subimage: u32,
        mip_level: u32,
    ) -> Result<ImageSpec> {
        let spec = self.image_spec_at(subimage, mip_level)?;
        if !spec.is_deep() {
            return Err(Error::operation(
                operation,
                "this image is not deep; use read_image_into".to_owned(),
            ));
        }
        Ok(spec)
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
        self.inner
            .as_ref()
            .expect("ImageInput invariant violated: null native pointer")
    }

    /// Whether this reader's format supports a named feature, such as
    /// `"mipmap"`, `"multiimage"`, `"exif"` or `"ioproxy"`.
    pub fn supports(&self, feature: &str) -> bool {
        sys::imageio::imageinput_supports(self.inner(), feature)
    }

    /// Whether another file appears readable by this reader's format.
    ///
    /// Formats with a cheap header check answer from the candidate's
    /// magic bytes; the rest fully open and close it. Either way the probe
    /// runs on a throwaway reader of the same format — OpenImageIO's own
    /// fallback would re-open and close *this* reader, silently
    /// invalidating it, which is why the shim never lets it near the live
    /// one.
    pub fn is_valid_file(&self, image_path: &Path) -> Result<bool> {
        let filename = path_to_utf8(image_path)?;
        Ok(sys::imageio::imageinput_valid_file(self.inner(), filename))
    }

    /// The raw reader handle, for wrappers whose shim takes the native
    /// pointer (`ImageOutput::copy_image_from`). `pub(crate)` so the pointer
    /// never crosses the crate boundary.
    pub(crate) fn native_mut(&mut self) -> std::pin::Pin<&mut sys::imageio::ImageInput> {
        self.inner_mut()
    }

    fn inner_mut(&mut self) -> std::pin::Pin<&mut sys::imageio::ImageInput> {
        self.inner
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

/// An image file open for writing.
///
/// A writer is always open: [`ImageOutput::create`] selects the plugin from
/// the file name and opens the file in one step, so there is no state in which
/// a write can be attempted against an unopened file.
///
/// ```no_run
/// use oiio::{f16, ImageOutput, ImageSpec, PixelFormat};
/// use std::path::Path;
///
/// # fn main() -> oiio::Result<()> {
/// let spec = ImageSpec::new(64, 64, 3, PixelFormat::F16)?;
/// let pixels = vec![f16::ZERO; spec.element_count()?];
///
/// let mut output = ImageOutput::create(Path::new("out.exr"), &spec)?;
/// output.write_image(&pixels)?;
/// output.close()?;
/// # Ok(())
/// # }
/// ```
pub struct ImageOutput {
    inner: cxx::UniquePtr<sys::imageio::ImageOutput>,
    path: PathBuf,
    spec: ImageSpec,
    /// The next row [`ImageOutput::write_scanlines`] will accept, for formats
    /// that will only take them in order. See that method.
    next_scanline: i32,
    /// Present only for in-memory writers. Declared last so it drops after
    /// the writer that fills it.
    proxy: Option<cxx::UniquePtr<sys::filesystem::IOProxy>>,
}

impl ImageOutput {
    /// Record the specification the file was actually opened with.
    ///
    /// Not the caller's: `ImageOutput::check_open` rewrites it during open. It
    /// zeroes the origin for any format that does not report `origin`, fills
    /// the display window in from the data window, raises a zero depth to one,
    /// and drops per-channel formats that match the overall one. A caller who
    /// set an origin a PNG cannot carry should be able to see that it is gone,
    /// and the range checks here have to agree with the ones the shim makes
    /// against this same spec.
    fn adopt_open_spec(&mut self, fallback: &ImageSpec) {
        self.spec = ImageSpec::from_sys(sys::imageio::imageoutput_spec(self.inner()))
            .unwrap_or_else(|_| fallback.clone());
        // A fresh image, so the scanline cursor goes back to the top of its
        // data window.
        self.next_scanline = self.spec.origin()[1];
    }

    /// Create and open an image file described by `spec`.
    ///
    /// The output plugin is chosen from the file name's extension. The file is
    /// truncated if it already exists.
    pub fn create(image_path: &Path, spec: &ImageSpec) -> Result<Self> {
        let mut output = Self::open_plugin(image_path)?;
        let native_spec = spec.to_sys()?;
        Self::open_native(
            output.inner_mut(),
            path_to_utf8(image_path)?,
            &native_spec,
            sys::imageio::OpenMode::Create,
        )
        .map_err(|message| Error::OpenImage {
            path: image_path.to_path_buf(),
            message,
        })?;

        output.path = image_path.to_path_buf();
        output.adopt_open_spec(spec);
        Ok(output)
    }

    /// Create and open a multi-part file, declaring every subimage up front.
    ///
    /// Some formats — OpenEXR among them — must know all subimages when the
    /// file is opened and reject [`ImageOutput::append_subimage`]. After this
    /// call the writer is positioned on the first subimage; advance with
    /// [`ImageOutput::append_subimage`], passing the same specifications in
    /// the same order.
    pub fn create_multi_subimage(image_path: &Path, specs: &[ImageSpec]) -> Result<Self> {
        let Some(first) = specs.first() else {
            return Err(Error::InvalidImageSpec(
                "a multi-part file needs at least one subimage".to_owned(),
            ));
        };

        let mut native_specs = sys::imageio::imagespec_vector_new();
        for spec in specs {
            let native_spec = spec.to_sys()?;
            let (Some(list), Some(native_spec)) = (native_specs.as_mut(), native_spec.as_ref())
            else {
                return Err(Error::InvalidImageSpec(
                    "OpenImageIO could not allocate an image specification".to_owned(),
                ));
            };
            sys::imageio::imagespec_vector_push(list, native_spec);
        }

        let mut output = Self::open_plugin(image_path)?;
        let path_str = path_to_utf8(image_path)?;
        let opened =
            sys::imageio::imageoutput_open_specs(output.inner_mut(), path_str, &native_specs);
        if !opened {
            let message = sys::imageio::imageoutput_geterror(output.inner(), true);
            return Err(Error::OpenImage {
                path: image_path.to_path_buf(),
                message: if message.is_empty() {
                    "OpenImageIO did not provide an error message".to_owned()
                } else {
                    message
                },
            });
        }

        output.path = image_path.to_path_buf();
        output.adopt_open_spec(first);
        Ok(output)
    }

    /// Write an image into memory instead of a file.
    ///
    /// `name_hint` is never created on disk; it only selects the writer, so
    /// its extension decides the encoding. Finish with
    /// [`ImageOutput::close_into_bytes`] to take the encoded bytes.
    ///
    /// Not every format can write to memory — those that cannot are reported
    /// here rather than at close time.
    pub fn to_memory(name_hint: &str, spec: &ImageSpec) -> Result<Self> {
        let mut proxy = sys::filesystem::ioproxy_vecoutput_new();
        let Some(proxy_ref) = proxy.as_mut() else {
            return Err(Error::CreateImage {
                path: PathBuf::from(name_hint),
                message: "OpenImageIO could not allocate a memory proxy".to_owned(),
            });
        };

        // SAFETY: the proxy is owned by the value returned here and is
        // declared after the writer, so the writer drops first.
        let inner = unsafe {
            let proxy_ptr = proxy_ref.get_unchecked_mut() as *mut sys::filesystem::IOProxy;
            sys::imageio::imageoutput_create(name_hint, proxy_ptr, "")
        };
        if inner.is_null() {
            return Err(Error::CreateImage {
                path: PathBuf::from(name_hint),
                message: global_error(),
            });
        }

        let mut output = Self {
            inner,
            path: PathBuf::from(name_hint),
            spec: spec.clone(),
            next_scanline: spec.origin()[1],
            proxy: Some(proxy),
        };
        if !output.supports("ioproxy") {
            return Err(Error::CreateImage {
                path: PathBuf::from(name_hint),
                message: format!("the {} writer cannot write to memory", output.format_name()),
            });
        }

        let native_spec = spec.to_sys()?;
        Self::open_native(
            output.inner_mut(),
            name_hint,
            &native_spec,
            sys::imageio::OpenMode::Create,
        )
        .map_err(|message| Error::OpenImage {
            path: PathBuf::from(name_hint),
            message,
        })?;
        output.adopt_open_spec(spec);
        Ok(output)
    }

    /// Finish an in-memory write and take the encoded bytes.
    ///
    /// Returns [`Error::Operation`] if this writer targets a file, since there
    /// are no bytes to hand back; use [`ImageOutput::close`] for those.
    pub fn close_into_bytes(mut self) -> Result<Vec<u8>> {
        if self.proxy.is_none() {
            return Err(Error::operation(
                "take encoded bytes",
                "this writer targets a file, not memory".to_owned(),
            ));
        }
        let succeeded = sys::imageio::imageoutput_close(self.inner_mut());
        self.check("close image", succeeded)?;

        let proxy = self.proxy.as_ref().expect("checked above");
        let proxy_ref = proxy.as_ref().ok_or_else(|| {
            Error::operation(
                "take encoded bytes",
                "the memory proxy was released early".to_owned(),
            )
        })?;
        Ok(sys::filesystem::ioproxy_vecoutput_bytes(proxy_ref))
    }

    /// Ask whether the output plugin for `image_path` supports a feature,
    /// without creating the file.
    ///
    /// Features are OpenImageIO's own names, such as `"tiles"`,
    /// `"mipmap"`, `"multiimage"`, or `"deepdata"`.
    pub fn plugin_supports(image_path: &Path, feature: &str) -> Result<bool> {
        Ok(Self::open_plugin(image_path)?.supports(feature))
    }

    /// Return the output plugin's format name.
    pub fn format_name(&self) -> &str {
        sys::imageio::imageoutput_format_name(self.inner())
    }

    /// Whether the open plugin supports a named feature.
    pub fn supports(&self, feature: &str) -> bool {
        sys::imageio::imageoutput_supports(self.inner(), feature) != 0
    }

    /// The specification the file is currently open with.
    pub fn spec(&self) -> &ImageSpec {
        &self.spec
    }

    /// Write every pixel of the current subimage and mip level.
    ///
    /// The buffer length must exactly equal
    /// `width * height * depth * channels`. Values are converted to the
    /// specification's pixel format by OpenImageIO.
    pub fn write_image<T: Pixel>(&mut self, pixels: &[T]) -> Result<()> {
        self.reject_deep()?;
        let expected = self.spec.element_count()?;
        validate_buffer_len(expected, pixels.len())?;

        // SAFETY: Pixel is sealed to initialized scalar layouts, and the shim
        // re-derives the layout from the open specification before writing.
        let succeeded = unsafe {
            sys::imageio::imageoutput_write_image_span(
                self.inner_mut(),
                pixel::type_desc::<T>(),
                pixel::as_bytes(pixels),
            )
        };
        self.check("write image", succeeded)?;
        // The whole subimage is written now; advancing the scanline cursor
        // makes a later write_scanlines or write_to refuse with this crate's
        // message instead of failing format-dependently inside the writer.
        self.mark_whole_subimage_written();
        Ok(())
    }

    /// Write a contiguous range of scanlines of a two-dimensional image.
    ///
    /// The buffer length must exactly equal
    /// `width * rows * channels`, where `rows` is the length of `y`.
    ///
    /// Unless the format reports the `random_access` feature, rows must be
    /// written in order: the first call starts at the top of the data window
    /// and each one continues where the last ended. This is not a limitation of
    /// this crate but of the formats. OpenEXR's scanline writer computes a
    /// "virtual framebuffer" base by biasing the caller's pointer backwards by
    /// the requested row, then hands it to a writer that keeps its own cursor
    /// starting at the top of the data window and only ever advancing. Ask it
    /// to write rows 512..1024 first and it reads 512 scanlines *before* the
    /// buffer. `supports("random_access")` is how OpenImageIO publishes the
    /// difference. For EXR it is true only for tiled files with a random
    /// line order; several formats that buffer the whole image before
    /// writing — DPX, FITS, GIF, RLA and WebP among them — advertise it
    /// unconditionally, and genuinely accept rows in any order.
    pub fn write_scanlines<T: Pixel>(&mut self, y: Range<i32>, pixels: &[T]) -> Result<()> {
        self.reject_deep()?;
        if !self.supports("random_access") && y.start != self.next_scanline {
            return Err(Error::InvalidRegion {
                axis: "y",
                message: format!(
                    "this format writes scanlines in order: the next row is {}, not {}",
                    self.next_scanline, y.start
                ),
            });
        }
        if self.spec.dimensions()[2] != 1 {
            return Err(Error::InvalidImageSpec(
                "scanline writes require a two-dimensional image".to_owned(),
            ));
        }
        let rows = self.validate_axis("y", &y, self.spec.origin()[1], self.spec.dimensions()[1])?;
        let expected = element_count([self.spec.dimensions()[0], rows, self.spec.channel_count()])?;
        validate_buffer_len(expected, pixels.len())?;

        // SAFETY: as in `write_image`.
        let succeeded = unsafe {
            sys::imageio::imageoutput_write_scanlines_span(
                self.inner_mut(),
                y.start,
                y.end,
                pixel::type_desc::<T>(),
                pixel::as_bytes(pixels),
            )
        };
        self.check("write scanlines", succeeded)?;
        self.next_scanline = y.end;
        Ok(())
    }

    /// Write a rectangular block of whole tiles.
    ///
    /// Each range must start on a tile boundary and either end on one or end
    /// at the edge of the data window. The buffer length must exactly equal
    /// the number of scalar values the block covers.
    pub fn write_tiles<T: Pixel>(
        &mut self,
        x: Range<i32>,
        y: Range<i32>,
        z: Range<i32>,
        pixels: &[T],
    ) -> Result<()> {
        self.reject_deep()?;
        let [tile_width, tile_height, tile_depth] = self.spec.tile_dimensions();
        if !self.spec.is_tiled() {
            return Err(Error::InvalidImageSpec(
                "tile writes require a specification with a tile size".to_owned(),
            ));
        }

        let origin = self.spec.origin();
        let dimensions = self.spec.dimensions();
        let width = self.validate_axis("x", &x, origin[0], dimensions[0])?;
        let height = self.validate_axis("y", &y, origin[1], dimensions[1])?;
        let depth = self.validate_axis("z", &z, origin[2], dimensions[2])?;
        validate_tile_alignment("x", &x, origin[0], dimensions[0], tile_width)?;
        validate_tile_alignment("y", &y, origin[1], dimensions[1], tile_height)?;
        validate_tile_alignment("z", &z, origin[2], dimensions[2], tile_depth.max(1))?;

        let expected = element_count([width, height, depth, self.spec.channel_count()])?;
        validate_buffer_len(expected, pixels.len())?;

        // SAFETY: as in `write_image`.
        let succeeded = unsafe {
            sys::imageio::imageoutput_write_tiles_span(
                self.inner_mut(),
                x.start,
                x.end,
                y.start,
                y.end,
                z.start,
                z.end,
                pixel::type_desc::<T>(),
                pixel::as_bytes(pixels),
            )
        };
        self.check("write tiles", succeeded)?;
        // Tiles touched this subimage; conservatively mark it written so a
        // later write_to refuses rather than interleaving format-dependently.
        self.mark_whole_subimage_written();
        Ok(())
    }

    /// Write a deep image.
    ///
    /// The writer must have been opened with a specification marked deep, and
    /// the image must match the size and channels it declared.
    pub fn write_deep_image(&mut self, deep: &DeepImage) -> Result<()> {
        if !self.spec.is_deep() {
            return Err(Error::operation(
                "write deep image",
                "this writer was opened for flat pixels; see ImageSpec::as_deep".to_owned(),
            ));
        }
        let dimensions = self.spec.dimensions();
        if deep.dimensions() != dimensions {
            return Err(Error::InvalidImageSpec(format!(
                "the deep image is {:?} but the writer expects {dimensions:?}",
                deep.dimensions()
            )));
        }
        if deep.channel_count() != self.spec.channel_count() as usize {
            return Err(Error::InvalidImageSpec(format!(
                "the deep image has {} channels but the writer expects {}",
                deep.channel_count(),
                self.spec.channel_count()
            )));
        }

        let succeeded = sys::imageio::imageoutput_write_deep_image(self.inner_mut(), deep.native());
        self.check("write deep image", succeeded)?;
        // The whole subimage is written; a later streaming call must not run,
        // because OpenEXR's deep scanline writer would bias the new deep
        // image's arrays by a cursor that is already past them.
        self.mark_whole_subimage_written();
        Ok(())
    }

    /// Write a contiguous band of scanlines of a deep image.
    ///
    /// The deep image must be exactly the band: as wide as the data window,
    /// as tall as `y`, one channel per declared channel. Its own origin does
    /// not matter — the samples land at `y`. Channel types that differ from
    /// the declared ones are converted by OpenImageIO.
    ///
    /// Bands must arrive in order, each starting where the previous ended:
    /// OpenEXR's deep scanline writer keeps its own advancing cursor but
    /// trusts the caller's starting row when it lays the sample arrays over
    /// the file, so an out-of-order band would be read outside its arrays.
    pub fn write_deep_scanlines(&mut self, y: Range<i32>, deep: &DeepImage) -> Result<()> {
        const OPERATION: &str = "write deep scanlines";
        self.require_deep_writer(OPERATION)?;
        if self.spec.is_tiled() {
            return Err(Error::InvalidImageSpec(
                "this file is tiled; use write_deep_tiles".to_owned(),
            ));
        }
        if self.spec.dimensions()[2] != 1 {
            return Err(Error::InvalidImageSpec(
                "scanline writes require a two-dimensional image".to_owned(),
            ));
        }
        if !self.supports("random_access") && y.start != self.next_scanline {
            return Err(Error::InvalidRegion {
                axis: "y",
                message: format!(
                    "this format writes scanlines in order: the next row is {}, not {}",
                    self.next_scanline, y.start
                ),
            });
        }
        let rows = validate_axis("y", &y, self.spec.origin()[1], self.spec.dimensions()[1])?;
        self.require_deep_band(OPERATION, deep, [self.spec.dimensions()[0], rows, 1])?;

        let succeeded = sys::imageio::imageoutput_write_deep_scanlines(
            self.inner_mut(),
            y.start,
            y.end,
            0,
            deep.native(),
        );
        self.check(OPERATION, succeeded)?;
        self.next_scanline = y.end;
        Ok(())
    }

    /// Write a rectangular block of whole tiles of a deep image.
    ///
    /// Each range must start on a tile boundary and either end on one or end
    /// at the edge of the data window — OpenEXR positions the block by tile
    /// index, so a misaligned range would place samples at rows and columns
    /// the deep image does not hold. The deep image must be exactly the
    /// block: its size equal to the ranges, one channel per declared
    /// channel; its own origin does not matter.
    pub fn write_deep_tiles(
        &mut self,
        x: Range<i32>,
        y: Range<i32>,
        z: Range<i32>,
        deep: &DeepImage,
    ) -> Result<()> {
        const OPERATION: &str = "write deep tiles";
        self.require_deep_writer(OPERATION)?;
        let [tile_width, tile_height, tile_depth] = self.spec.tile_dimensions();
        if !self.spec.is_tiled() {
            return Err(Error::InvalidImageSpec(
                "tile writes require a specification with a tile size".to_owned(),
            ));
        }

        let origin = self.spec.origin();
        let dimensions = self.spec.dimensions();
        let width = validate_axis("x", &x, origin[0], dimensions[0])?;
        let height = validate_axis("y", &y, origin[1], dimensions[1])?;
        let depth = validate_axis("z", &z, origin[2], dimensions[2])?;
        validate_tile_alignment("x", &x, origin[0], dimensions[0], tile_width)?;
        validate_tile_alignment("y", &y, origin[1], dimensions[1], tile_height)?;
        validate_tile_alignment("z", &z, origin[2], dimensions[2], tile_depth.max(1))?;
        self.require_deep_band(OPERATION, deep, [width, height, depth])?;

        let succeeded = sys::imageio::imageoutput_write_deep_tiles(
            self.inner_mut(),
            x.start,
            x.end,
            y.start,
            y.end,
            z.start,
            z.end,
            deep.native(),
        );
        self.check(OPERATION, succeeded)?;
        // Tiles touched this subimage; conservatively mark it written so a
        // later whole-image or streaming write refuses rather than
        // interleaving format-dependently.
        self.mark_whole_subimage_written();
        Ok(())
    }

    /// The two deep streaming writers' shared preconditions: a writer opened
    /// with a deep specification, on a format that reports `deepdata` —
    /// OpenImageIO's fallback for the rest fails without recording a message.
    fn require_deep_writer(&self, operation: &'static str) -> Result<()> {
        if !self.spec.is_deep() {
            return Err(Error::operation(
                operation,
                "this writer was opened for flat pixels; see ImageSpec::as_deep".to_owned(),
            ));
        }
        if !self.supports("deepdata") {
            return Err(Error::operation(
                operation,
                format!(
                    "the {} format does not support deep images",
                    self.format_name()
                ),
            ));
        }
        Ok(())
    }

    /// Check that a deep image is shaped exactly like the region a streaming
    /// write covers.
    fn require_deep_band(
        &self,
        operation: &'static str,
        deep: &DeepImage,
        expected: [u32; 3],
    ) -> Result<()> {
        if deep.dimensions() != expected {
            return Err(Error::operation(
                operation,
                format!(
                    "the deep image is {:?} but the region covers {expected:?}",
                    deep.dimensions()
                ),
            ));
        }
        if deep.channel_count() != self.spec.channel_count() as usize {
            return Err(Error::operation(
                operation,
                format!(
                    "the deep image has {} channels but the writer expects {}",
                    deep.channel_count(),
                    self.spec.channel_count()
                ),
            ));
        }
        Ok(())
    }

    /// Attach a thumbnail to the image being written.
    ///
    /// Only formats reporting the `thumbnail` capability store one — among
    /// real formats in OpenImageIO 3.1 that is Targa alone; the `null`
    /// testing sink also claims it, then discards the call with
    /// OpenImageIO's messageless fallback — and that fallback fails without
    /// recording anything, so formats not claiming the capability are
    /// refused here with a clear message. The thumbnail's channel count must match the
    /// image's (Targa refuses a mismatch silently), and both dimensions
    /// must be under 256: the TGA postage stamp stores its size in single
    /// bytes, and through 3.1 OpenImageIO's own downsizing clamps to 256 —
    /// one past what the byte holds — silently writing a zero-dimension
    /// thumbnail. (Unreleased 3.2 resizes to 255 itself; the refusal here
    /// is version-independent.) Formats not reporting
    /// `thumbnail_after_write` additionally need the thumbnail set before
    /// any pixels.
    pub fn set_thumbnail(&mut self, thumbnail: &crate::ImageBuf) -> Result<()> {
        const OPERATION: &str = "set thumbnail";
        if !self.supports("thumbnail") {
            return Err(Error::operation(
                OPERATION,
                format!(
                    "the {} format does not store thumbnails",
                    self.format_name()
                ),
            ));
        }
        if !self.supports("thumbnail_after_write") && self.next_scanline != self.spec.origin()[1] {
            return Err(Error::operation(
                OPERATION,
                "this format needs the thumbnail before any pixels are written".to_owned(),
            ));
        }
        let thumb_spec = thumbnail.spec()?;
        let [width, height, _] = thumb_spec.dimensions();
        if width == 0 || height == 0 {
            return Err(Error::operation(
                OPERATION,
                "the thumbnail has no pixels".to_owned(),
            ));
        }
        if width >= 256 || height >= 256 {
            return Err(Error::operation(
                OPERATION,
                format!(
                    "a {width}×{height} thumbnail cannot be stored: dimensions must be \
                     under 256, and OpenImageIO's own downsizing clamps to 256 and \
                     truncates it to zero — resize it first"
                ),
            ));
        }
        if thumb_spec.channel_count() != self.spec.channel_count() {
            return Err(Error::operation(
                OPERATION,
                format!(
                    "the thumbnail has {} channels but the image has {}",
                    thumb_spec.channel_count(),
                    self.spec.channel_count()
                ),
            ));
        }

        let succeeded =
            sys::imageio::imageoutput_set_thumbnail(self.inner_mut(), thumbnail.inner());
        self.check(OPERATION, succeeded)
    }

    /// Begin a new subimage in the same file.
    ///
    /// Only supported by formats that report the `"multiimage"` feature.
    pub fn append_subimage(&mut self, spec: &ImageSpec) -> Result<()> {
        self.append(
            spec,
            sys::imageio::OpenMode::AppendSubimage,
            "append subimage",
        )
    }

    /// Begin a new mip level of the current subimage.
    ///
    /// Only supported by formats that report the `"mipmap"` feature.
    pub fn append_mip_level(&mut self, spec: &ImageSpec) -> Result<()> {
        self.append(
            spec,
            sys::imageio::OpenMode::AppendMIPLevel,
            "append mip level",
        )
    }

    /// Get the current thread count.
    pub fn threads(&self) -> i32 {
        sys::imageio::imageoutput_threads(self.inner())
    }

    /// Set the thread count. Zero requests OpenImageIO's default.
    pub fn set_threads(&mut self, threads: i32) {
        sys::imageio::imageoutput_set_threads(self.inner_mut(), threads);
    }

    /// Finish the file and report any delayed format or I/O errors.
    ///
    /// Dropping an `ImageOutput` without calling this method still closes the
    /// file, but cannot report errors that only surface at close time.
    pub fn close(mut self) -> Result<()> {
        let succeeded = sys::imageio::imageoutput_close(self.inner_mut());
        self.check("close image", succeeded)
    }

    fn open_plugin(image_path: &Path) -> Result<Self> {
        let image_path_str = path_to_utf8(image_path)?;
        // SAFETY: a null IOProxy asks OpenImageIO to open the file itself.
        let inner =
            unsafe { sys::imageio::imageoutput_create(image_path_str, std::ptr::null_mut(), "") };
        if inner.is_null() {
            return Err(Error::CreateImage {
                path: image_path.to_path_buf(),
                message: global_error(),
            });
        }

        Ok(Self {
            inner,
            path: image_path.to_path_buf(),
            spec: ImageSpec::new(1, 1, 1, crate::PixelFormat::U8)?,
            next_scanline: 0,
            proxy: None,
        })
    }

    fn open_native(
        output: std::pin::Pin<&mut sys::imageio::ImageOutput>,
        path: &str,
        spec: &cxx::UniquePtr<sys::imageio::ImageSpec>,
        mode: sys::imageio::OpenMode,
    ) -> std::result::Result<(), String> {
        let Some(native_spec) = spec.as_ref() else {
            return Err("OpenImageIO could not allocate an image specification".to_owned());
        };
        if sys::imageio::imageoutput_open(output, path, native_spec, mode) {
            Ok(())
        } else {
            Err(String::new())
        }
    }

    fn append(
        &mut self,
        spec: &ImageSpec,
        mode: sys::imageio::OpenMode,
        operation: &'static str,
    ) -> Result<()> {
        let native_spec = spec.to_sys()?;
        let path = path_to_utf8(&self.path)?.to_owned();
        let opened = Self::open_native(self.inner_mut(), &path, &native_spec, mode);
        match opened {
            Ok(()) => {
                self.adopt_open_spec(spec);
                Ok(())
            }
            Err(message) if message.is_empty() => Err(self.take_error(operation)),
            Err(message) => Err(Error::operation(operation, message)),
        }
    }

    fn validate_axis(
        &self,
        axis: &'static str,
        range: &Range<i32>,
        origin: i32,
        size: u32,
    ) -> Result<u32> {
        validate_axis(axis, range, origin, size)
    }
}

/// Check that a coordinate range is non-empty and inside the data window on
/// one axis, and return its length.
fn validate_axis(axis: &'static str, range: &Range<i32>, origin: i32, size: u32) -> Result<u32> {
    if range.start >= range.end {
        return Err(Error::InvalidRegion {
            axis,
            message: format!(
                "range must be non-empty and increasing, got {}..{}",
                range.start, range.end
            ),
        });
    }
    let end = i64::from(origin) + i64::from(size);
    if i64::from(range.start) < i64::from(origin) || i64::from(range.end) > end {
        return Err(Error::InvalidRegion {
            axis,
            message: format!(
                "range {}..{} lies outside the data window {origin}..{end}",
                range.start, range.end
            ),
        });
    }
    Ok((i64::from(range.end) - i64::from(range.start)) as u32)
}

impl ImageOutput {
    fn reject_deep(&self) -> Result<()> {
        if self.spec.is_deep() {
            return Err(Error::UnsupportedDeepImage);
        }
        Ok(())
    }

    fn check(&mut self, operation: &'static str, succeeded: bool) -> Result<()> {
        if succeeded {
            Ok(())
        } else {
            Err(self.take_error(operation))
        }
    }

    fn inner(&self) -> &sys::imageio::ImageOutput {
        self.inner
            .as_ref()
            .expect("ImageOutput invariant violated: null native pointer")
    }

    fn inner_mut(&mut self) -> std::pin::Pin<&mut sys::imageio::ImageOutput> {
        self.inner
            .as_mut()
            .expect("ImageOutput invariant violated: null native pointer")
    }

    /// The raw writer handle, for wrappers in other modules whose shim takes
    /// the native pointer (`ImageBuf::write_to`). `pub(crate)` so the
    /// pointer never crosses the crate boundary.
    pub(crate) fn native_mut(&mut self) -> std::pin::Pin<&mut sys::imageio::ImageOutput> {
        self.inner_mut()
    }

    /// Where the in-order scanline cursor stands; equals the data window's
    /// top row while the current subimage is untouched.
    pub(crate) fn scanline_cursor(&self) -> i32 {
        self.next_scanline
    }

    /// Record that a whole-image path (`ImageBuf::write_to`) wrote every row
    /// behind the crate's cursor, so a later in-order `write_scanlines` is
    /// refused with a clear error instead of failing inside the format
    /// writer.
    pub(crate) fn mark_whole_subimage_written(&mut self) {
        let end = i64::from(self.spec.origin()[1]) + i64::from(self.spec.dimensions()[1]);
        self.next_scanline = end.min(i64::from(i32::MAX)) as i32;
    }

    /// Write an arbitrary rectangle of pixels, for formats that can place
    /// pixels at random.
    ///
    /// Only formats reporting the `rectangles` capability accept this —
    /// [`ImageOutput::supports`] tells; OpenImageIO's fallback for the rest
    /// returns failure without recording a message, which this wrapper turns
    /// into a clear error before calling. No format shipped with
    /// OpenImageIO 3.1 reports the capability, so today this is refused for
    /// every built-in writer; it exists for third-party plugins that do.
    /// The rectangle must lie inside the data window and the buffer must
    /// hold exactly its pixels. After a rectangle lands, the subimage
    /// counts as written for the in-order scanline cursor.
    pub fn write_rectangle<T: Pixel>(
        &mut self,
        x: Range<i32>,
        y: Range<i32>,
        pixels: &[T],
    ) -> Result<()> {
        self.reject_deep()?;
        if !self.supports("rectangles") {
            return Err(Error::operation(
                "write rectangle",
                format!(
                    "the {} format cannot place arbitrary rectangles; OpenImageIO's \
                     fallback fails without a message, so it is refused here",
                    self.format_name()
                ),
            ));
        }
        let origin = self.spec.origin();
        let dimensions = self.spec.dimensions();
        let width = self.validate_axis("x", &x, origin[0], dimensions[0])?;
        let height = self.validate_axis("y", &y, origin[1], dimensions[1])?;
        let expected = element_count([width, height, 1, self.spec.channel_count()])?;
        validate_buffer_len(expected, pixels.len())?;

        // SAFETY: Pixel is sealed to initialized scalar layouts, so the byte
        // view holds initialized values of the declared format.
        let succeeded = unsafe {
            sys::imageio::imageoutput_write_rectangle_span(
                self.inner_mut(),
                x.start,
                x.end,
                y.start,
                y.end,
                0,
                1,
                pixel::type_desc::<T>(),
                pixel::as_bytes(pixels),
            )
        };
        self.check("write rectangle", succeeded)?;
        self.mark_whole_subimage_written();
        Ok(())
    }

    /// Copy a reader's current subimage into this writer — the lossless
    /// transcode path `iconvert` is built on.
    ///
    /// Pixel data is carried in the file's native format wherever the
    /// formats allow, without a decode to a caller type in between; deep
    /// images are copied sample-for-sample. OpenImageIO verifies that the
    /// two specifications agree on dimensions and channel count and reports
    /// a clear error when they do not. Nothing may have been written to the
    /// current subimage yet, and the writer stays open afterwards.
    pub fn copy_image_from(&mut self, input: &mut ImageInput) -> Result<()> {
        const OPERATION: &str = "copy image from reader";
        let top = self.spec.origin()[1];
        if self.next_scanline != top {
            return Err(Error::operation(
                OPERATION,
                format!(
                    "rows {top}..{} of this subimage were already written; the copy \
                     needs an untouched subimage",
                    self.next_scanline
                ),
            ));
        }
        // SAFETY: the pointer comes from a live &mut ImageInput, is consumed
        // within this one call, and the pinned reader is not moved.
        let succeeded = unsafe {
            let in_ptr = input.native_mut().get_unchecked_mut() as *mut sys::imageio::ImageInput;
            sys::imageio::imageoutput_copy_image(self.inner_mut(), in_ptr)
        };
        if !succeeded {
            return Err(self.take_error(OPERATION));
        }
        // The whole subimage is written; the cursor keeps write_scanlines
        // and write_to honest about it.
        self.mark_whole_subimage_written();
        Ok(())
    }

    fn take_error(&mut self, operation: &'static str) -> Error {
        Error::operation(
            operation,
            sys::imageio::imageoutput_geterror(self.inner(), true),
        )
    }
}

fn validate_tile_alignment(
    axis: &'static str,
    range: &Range<i32>,
    origin: i32,
    size: u32,
    tile: u32,
) -> Result<()> {
    if tile == 0 {
        return Err(Error::InvalidImageSpec(format!(
            "tile size on the {axis} axis must be non-zero"
        )));
    }
    let tile = i64::from(tile);
    let edge = i64::from(origin) + i64::from(size);

    if (i64::from(range.start) - i64::from(origin)) % tile != 0 {
        return Err(Error::InvalidRegion {
            axis,
            message: format!("start {} is not on the {tile}-pixel tile grid", range.start),
        });
    }
    if i64::from(range.end) != edge && (i64::from(range.end) - i64::from(origin)) % tile != 0 {
        return Err(Error::InvalidRegion {
            axis,
            message: format!(
                "end {} is neither on the {tile}-pixel tile grid nor at the data window edge {edge}",
                range.end
            ),
        });
    }
    Ok(())
}

fn global_error() -> String {
    if sys::imageio::has_error() {
        sys::imageio::get_error(true)
    } else {
        "OpenImageIO did not provide an error message".to_owned()
    }
}

impl std::fmt::Debug for ImageInput {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ImageInput")
            .field("format", &self.format_name())
            .finish_non_exhaustive()
    }
}

impl std::fmt::Debug for ImageOutput {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ImageOutput")
            .field("path", &self.path)
            .field("format", &self.format_name())
            .field("dimensions", &self.spec.dimensions())
            .field("pixel_format", &self.spec.format())
            .finish_non_exhaustive()
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
