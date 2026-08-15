use std::path::Path;

use crate::{
    imageio::validate_buffer_len, path_to_utf8, pixel, sys, Error, ImageBuf, ImageSpec, Pixel,
    PixelFormat, Result, Roi,
};

/// A thread-safe OpenImageIO image cache.
///
/// Use [`ImageCache::builder`] to configure a private cache. Private caches do
/// not interfere with process-wide cache state.
pub struct ImageCache {
    inner: cxx::SharedPtr<sys::imagecache::ImageCache>,
}

// SAFETY: OpenImageIO documents the cache's read paths as thread-safe, and
// every method here clones the shared_ptr and uses CXX's opaque-type exception
// for thread-safe non-const C++ methods; no Rust reference to the C++ object
// escapes a call.
//
// Invalidation and statistics are the exceptions and are not thread-safe:
// `invalidate_all` clears each file's subimage and dimension pools while
// another thread may be reading through them, and `getstats`/`reset_stats`
// walk those same pools and the per-thread counters with no lock at all. That
// is why `invalidate`, `invalidate_all`, `stats` and `reset_stats` take
// `&mut self`. A `&mut` borrow cannot be shared across threads, so `Sync` is
// only ever claiming the read paths, which is what OpenImageIO actually
// guarantees. It is
// also why the builder does not offer OpenImageIO's process-wide shared cache:
// two Rust values over one C++ cache would let `&mut` on one alias `&` on the
// other, and every borrow-based guarantee in this module is expressed against a
// single value.
unsafe impl Send for ImageCache {}
unsafe impl Sync for ImageCache {}

impl ImageCache {
    /// Start building a private image cache.
    pub fn builder() -> ImageCacheBuilder {
        ImageCacheBuilder::default()
    }

    /// Create a private cache with OpenImageIO's default settings.
    pub fn new() -> Result<Self> {
        Self::builder().build()
    }

    /// Return the native base-level image specification.
    pub fn image_spec(&self, image_path: &Path) -> Result<ImageSpec> {
        let filename = path_to_utf8(image_path)?;
        let mut error = String::new();
        let spec = self.with_cache(|cache| {
            sys::imagecache::imagecache_get_imagespec_copy_with_error(
                cache, filename, 0, &mut error,
            )
        });
        Self::convert_spec(spec, error, "query image specification")
    }

    /// Return the dimensions used by the cache for a subimage and mip level.
    pub fn image_spec_at(
        &self,
        image_path: &Path,
        subimage: u32,
        mip_level: u32,
    ) -> Result<ImageSpec> {
        let filename = path_to_utf8(image_path)?;
        let subimage = level_index(subimage)?;
        let mip_level = level_index(mip_level)?;
        let mut error = String::new();
        let spec = self.with_cache(|cache| {
            sys::imagecache::imagecache_get_image_spec_at_copy_with_error(
                cache, filename, subimage, mip_level, &mut error,
            )
        });
        Self::convert_spec(spec, error, "query cached image dimensions")
    }

    /// Read a pixel region from the base image into a contiguous buffer.
    pub fn get_pixels_into<T: Pixel>(
        &self,
        image_path: &Path,
        roi: Roi,
        pixels: &mut [T],
    ) -> Result<()> {
        self.get_pixels_into_at(image_path, 0, 0, roi, pixels)
    }

    /// Read a pixel region from a subimage and mip level.
    ///
    /// Spatial coordinates outside the image's data window are accepted and
    /// zero-filled by OpenImageIO. The channel range must exist in the image.
    /// The destination length must exactly equal `roi.element_count()`.
    pub fn get_pixels_into_at<T: Pixel>(
        &self,
        image_path: &Path,
        subimage: u32,
        mip_level: u32,
        roi: Roi,
        pixels: &mut [T],
    ) -> Result<()> {
        let filename = path_to_utf8(image_path)?;
        let subimage_index = level_index(subimage)?;
        let mip_level_index = level_index(mip_level)?;
        let spec = self.image_spec_at(image_path, subimage, mip_level)?;
        if spec.is_deep() {
            return Err(Error::UnsupportedDeepImage);
        }
        roi.validate_channels(&spec)?;
        validate_buffer_len(roi.element_count()?, pixels.len())?;

        let sys_roi = roi.to_sys();
        let mut error = String::new();
        let succeeded = self.with_cache(|cache| {
            // SAFETY: Pixel is sealed to initialized scalar layouts whose
            // type, alignment, element count, and byte extent were validated.
            unsafe {
                sys::imagecache::imagecache_get_pixels_span_with_error(
                    cache,
                    filename,
                    subimage_index,
                    mip_level_index,
                    &sys_roi,
                    pixel::type_desc::<T>(),
                    pixel::as_bytes_mut(pixels),
                    &mut error,
                )
            }
        });
        if succeeded {
            Ok(())
        } else {
            Err(Error::operation("read cached pixels", error))
        }
    }

    /// Resolve a file name to a handle for repeated reads.
    ///
    /// The handle borrows this cache. Reads through it skip the file-name
    /// lookup, which is worth doing when one image is read many times.
    pub fn handle(&self, image_path: &Path) -> Result<ImageHandle<'_>> {
        let filename = path_to_utf8(image_path)?;
        let inner = self.with_cache(|cache| {
            // SAFETY: a null per-thread pointer asks the cache to use its own
            // record, and a null options pointer requests the defaults.
            unsafe {
                sys::imagecache::imagecache_get_image_handle(
                    cache,
                    filename,
                    std::ptr::null_mut(),
                    std::ptr::null(),
                )
            }
        });
        if inner.is_null() {
            return Err(Error::OpenImage {
                path: image_path.to_path_buf(),
                message: "the image cache could not resolve the file".to_owned(),
            });
        }

        let handle = ImageHandle { cache: self, inner };
        if !handle.is_good() {
            return Err(Error::OpenImage {
                path: image_path.to_path_buf(),
                message: "the image cache could not open or read the file".to_owned(),
            });
        }
        Ok(handle)
    }

    /// Create per-thread state for this cache.
    ///
    /// The returned value is neither [`Send`] nor [`Sync`] and is destroyed
    /// when dropped. Using it is optional; see [`Perthread`].
    pub fn thread_state(&self) -> Result<Perthread<'_>> {
        let inner = self.with_cache(|cache| {
            // SAFETY: the record is owned by the caller from here on, and
            // `Perthread`'s Drop destroys it exactly once.
            unsafe { sys::imagecache::imagecache_create_thread_info(cache) }
        });
        if inner.is_null() {
            return Err(Error::operation(
                "create per-thread cache state",
                "OpenImageIO returned no per-thread record".to_owned(),
            ));
        }
        Ok(Perthread {
            cache: self,
            inner,
            _not_thread_safe: std::marker::PhantomData,
        })
    }

    /// Borrow one tile, held until the returned guard is dropped.
    ///
    /// `origin` is any pixel coordinate inside the wanted tile; OpenImageIO
    /// resolves it to the tile that contains it. The tile holds the format
    /// the cache stores, which is the file's own for `uint8`, `uint16` and
    /// `half` files and `f32` for everything else; [`TileGuard::format`]
    /// reports which. An edge tile may extend past the data window.
    ///
    /// The coordinate must lie inside the data window. OpenImageIO itself
    /// returns a tile for coordinates far outside the image — for a 32x32
    /// image, asking at (1000, 1000) yields a tile covering 992..1008 on both
    /// axes — so this rejects the request rather than hand back a region the
    /// image does not have.
    pub fn tile(
        &self,
        image_path: &Path,
        subimage: u32,
        mip_level: u32,
        origin: [i32; 3],
        channels: std::ops::Range<u32>,
    ) -> Result<TileGuard<'_>> {
        let filename = path_to_utf8(image_path)?;
        let subimage_index = level_index(subimage)?;
        let mip_level_index = level_index(mip_level)?;
        if channels.start >= channels.end {
            return Err(Error::InvalidRoi(format!(
                "channel range must be non-empty and increasing, got {}..{}",
                channels.start, channels.end
            )));
        }
        let channel_begin = level_index(channels.start)?;
        let channel_end = level_index(channels.end)?;

        let spec = self.image_spec_at(image_path, subimage, mip_level)?;
        if channels.end > spec.channel_count() {
            return Err(Error::InvalidRoi(format!(
                "channel range {}..{} extends outside the image's {} channels",
                channels.start,
                channels.end,
                spec.channel_count()
            )));
        }
        let window_origin = spec.origin();
        let dimensions = spec.dimensions();
        for (axis, coordinate, start, size) in [
            ("x", origin[0], window_origin[0], dimensions[0]),
            ("y", origin[1], window_origin[1], dimensions[1]),
            ("z", origin[2], window_origin[2], dimensions[2]),
        ] {
            let end = i64::from(start) + i64::from(size);
            if i64::from(coordinate) < i64::from(start) || i64::from(coordinate) >= end {
                return Err(Error::InvalidRegion {
                    axis,
                    message: format!(
                        "tile coordinate {coordinate} lies outside the data window {start}..{end}"
                    ),
                });
            }
        }

        let inner = self.with_cache(|cache| {
            // SAFETY: every argument is a plain value; the returned tile is
            // owned by the cache until released, which TileGuard does on drop.
            unsafe {
                sys::imagecache::imagecache_get_tile(
                    cache,
                    filename,
                    subimage_index,
                    mip_level_index,
                    origin[0],
                    origin[1],
                    origin[2],
                    channel_begin,
                    channel_end,
                )
            }
        });
        if inner.is_null() {
            return Err(Error::operation(
                "borrow cached tile",
                "OpenImageIO returned no tile for that coordinate".to_owned(),
            ));
        }
        // Record the geometry now, while the file's spec is known good.
        // Asking the cache for it later reads file state that invalidation
        // frees, and `pixels` derives a slice length from it.
        // SAFETY: the tile is live from here until the guard is dropped.
        let tile_roi =
            self.with_cache(|cache| unsafe { sys::imagecache::imagecache_tile_roi(cache, inner) });
        let tile_roi = Roi::from_sys(&tile_roi)?;
        // Ask through tile_pixels rather than tile_format. They disagree on
        // purpose: tile_format returns the *file's* pixel format, while the
        // tile is stored in the format the cache chose, which is float for
        // everything except uint8, uint16 and half. `pixels` compares against
        // the second, so reporting the first would have named a type `pixels`
        // refuses for every uint32, int32 or double file.
        let mut tile_format = pixel::type_desc::<f32>();
        // SAFETY: as above; `tile_format` is an out parameter and the returned
        // pointer is not retained here.
        let _ = self.with_cache(|cache| unsafe {
            sys::imagecache::imagecache_tile_pixels(cache, inner, &mut tile_format)
        });
        Ok(TileGuard {
            cache: self,
            inner,
            element_count: tile_roi.element_count()?,
            roi: tile_roi,
            format: PixelFormat::from_sys(&tile_format),
        })
    }

    /// Invalidate one cached image. The next access will re-read it.
    ///
    /// This takes `&mut self` because it is the one cache operation that is not
    /// thread-safe, and because it tears down state that a live [`TileGuard`],
    /// [`ImageHandle`] or [`Perthread`] still points into. The exclusive borrow
    /// makes both into compile errors rather than reads of freed memory.
    pub fn invalidate(&mut self, image_path: &Path, force: bool) -> Result<()> {
        let filename = path_to_utf8(image_path)?;
        self.with_cache(|cache| {
            sys::imagecache::imagecache_invalidate(cache, filename, force);
        });
        Ok(())
    }

    /// Invalidate every image held by this cache.
    ///
    /// Exclusive for the reasons given on [`ImageCache::invalidate`].
    pub fn invalidate_all(&mut self, force: bool) {
        self.with_cache(|cache| {
            sys::imagecache::imagecache_invalidate_all(cache, force);
        });
    }

    /// Read an integer cache setting, `None` when the name is unknown or not
    /// an integer.
    ///
    /// The `stat:` names are deliberately refused here: OpenImageIO gathers
    /// them by merging per-thread counters with no lock — the data race
    /// [`ImageCache::stats`] is exclusive for — so statistics stay on that
    /// exclusive path.
    pub fn setting_int(&self, name: &str) -> Result<Option<i32>> {
        Self::refuse_stat(name)?;
        let mut value = 0_i32;
        let found = self.with_cache(|cache| {
            sys::imagecache::imagecache_getattribute_int(cache, name, &mut value)
        });
        Ok(found.then_some(value))
    }

    /// Read a float cache setting; see [`ImageCache::setting_int`].
    pub fn setting_float(&self, name: &str) -> Result<Option<f32>> {
        Self::refuse_stat(name)?;
        let mut value = 0.0_f32;
        let found = self.with_cache(|cache| {
            sys::imagecache::imagecache_getattribute_float(cache, name, &mut value)
        });
        Ok(found.then_some(value))
    }

    /// Read a string cache setting; see [`ImageCache::setting_int`].
    pub fn setting_string(&self, name: &str) -> Result<Option<String>> {
        Self::refuse_stat(name)?;
        cxx::let_cxx_string!(value = "");
        let found = self.with_cache(|cache| {
            sys::imagecache::imagecache_getattribute_string(cache, name, value.as_mut())
        });
        Ok(found.then(|| value.to_string_lossy().into_owned()))
    }

    fn refuse_stat(name: &str) -> Result<()> {
        if name.starts_with("stat:") {
            return Err(Error::InvalidCacheSetting {
                name: "stat:*",
                value: "statistics are read through ImageCache::stats, which is \
                        exclusive for the reason its documentation gives"
                    .to_owned(),
            });
        }
        Ok(())
    }

    /// Whether a file exists and some format can read it.
    ///
    /// This is the one query that is not an error to ask of a missing or
    /// unreadable file — OpenImageIO answers it instead of complaining, so
    /// probing a candidate path is cheap and quiet.
    pub fn exists(&self, image_path: &Path) -> Result<bool> {
        Ok(self
            .info_int(image_path, 0, 0, "exists", "query existence")?
            .unwrap_or(0)
            != 0)
    }

    /// Whether the name is a UDIM pattern, such as `tex.<UDIM>.exr`.
    ///
    /// The query name is `"UDIM"`, capitalized: OpenImageIO's documentation
    /// spells it `udim`, but its implementation compares interned strings
    /// exactly and only answers the capitalized form — the lowercase name
    /// falls into the per-tile aggregation and answers `false` for every
    /// real UDIM set. A pattern whose tile set is empty on disk is an
    /// error rather than `false`: OpenImageIO treats it as an unreadable
    /// file.
    pub fn is_udim(&self, image_path: &Path) -> Result<bool> {
        Ok(self
            .info_int(image_path, 0, 0, "UDIM", "query UDIM")?
            .unwrap_or(0)
            != 0)
    }

    /// Number of subimages in the file.
    ///
    /// A file that cannot answer is an error, not a zero. The reachable
    /// case is a UDIM pattern whose tiles disagree on the count —
    /// OpenImageIO aggregates per-tile answers and declines, without a
    /// message, when they differ.
    pub fn subimage_count(&self, image_path: &Path) -> Result<u32> {
        const OPERATION: &str = "count subimages";
        let count = self.info_int(image_path, 0, 0, "subimages", OPERATION)?;
        Self::answered(OPERATION, count)
    }

    /// Number of mip levels of a subimage; as with
    /// [`ImageCache::subimage_count`], a file that cannot answer is an
    /// error, not a zero.
    pub fn mip_level_count(&self, image_path: &Path, subimage: u32) -> Result<u32> {
        const OPERATION: &str = "count mip levels";
        let count = self.info_int(image_path, subimage, 0, "miplevels", OPERATION)?;
        Self::answered(OPERATION, count)
    }

    /// The counts' shared refusal to invent a zero for a declined query.
    fn answered(operation: &'static str, count: Option<i32>) -> Result<u32> {
        match count {
            Some(count) => Ok(count.max(0) as u32),
            None => Err(Error::operation(
                operation,
                "the file did not answer; a UDIM pattern answers only when \
                 every tile agrees"
                    .to_owned(),
            )),
        }
    }

    /// The name of the format reading the file, such as `"openexr"`.
    pub fn file_format(&self, image_path: &Path) -> Result<String> {
        self.info_string(image_path, 0, 0, "fileformat", "query file format")
    }

    /// What kind of texture the file is, in OpenImageIO's words: `"Plain
    /// Texture"`, `"Volume Texture"`, `"Shadow"` or `"Environment"`. Plain
    /// images answer `"Plain Texture"` too; this describes how a texture
    /// system would use the file, not whether `maketx` made it.
    pub fn texture_type(&self, image_path: &Path) -> Result<String> {
        self.info_string(image_path, 0, 0, "texturetype", "query texture type")
    }

    /// The texture format, a finer-grained sibling of
    /// [`ImageCache::texture_type`] that distinguishes, for example,
    /// `"CubeFace Environment"` from `"LatLong Environment"`.
    pub fn texture_format(&self, image_path: &Path) -> Result<String> {
        self.info_string(image_path, 0, 0, "textureformat", "query texture format")
    }

    /// The average color of a subimage, one value per channel.
    ///
    /// Two sources can answer: an `oiio:AverageColor` attribute written by
    /// `maketx`-shaped software (which [`make_texture`](crate::make_texture)
    /// writes even without a mip pyramid), or — when the attribute is
    /// absent — sampling a 1×1 coarsest mip level. A plain image with
    /// neither is `None`.
    pub fn average_color(&self, image_path: &Path, subimage: u32) -> Result<Option<Vec<f32>>> {
        let spec = self.image_spec_at(image_path, subimage, 0)?;
        self.info_floats(
            image_path,
            subimage,
            spec.channel_count(),
            "averagecolor",
            "query average color",
        )
    }

    /// The average of the alpha channel; `None` when the image has no alpha
    /// channel or no 1×1 mip level to derive it from.
    pub fn average_alpha(&self, image_path: &Path, subimage: u32) -> Result<Option<f32>> {
        let alpha = self.info_floats(
            image_path,
            subimage,
            1,
            "averagealpha",
            "query average alpha",
        )?;
        Ok(alpha.map(|values| values[0]))
    }

    /// The single color every pixel of the subimage holds, or `None` if the
    /// image is not constant (or was not marked constant by `maketx`).
    pub fn constant_color(&self, image_path: &Path, subimage: u32) -> Result<Option<Vec<f32>>> {
        let spec = self.image_spec_at(image_path, subimage, 0)?;
        self.info_floats(
            image_path,
            subimage,
            spec.channel_count(),
            "constantcolor",
            "query constant color",
        )
    }

    /// The single alpha value every pixel holds; `None` when the image is not
    /// constant or has no alpha channel.
    pub fn constant_alpha(&self, image_path: &Path, subimage: u32) -> Result<Option<f32>> {
        let alpha = self.info_floats(
            image_path,
            subimage,
            1,
            "constantalpha",
            "query constant alpha",
        )?;
        Ok(alpha.map(|values| values[0]))
    }

    /// The thumbnail a file carries for a subimage, or `None` if the file or
    /// its format has none. In OpenImageIO 3.1 the formats that store one are
    /// PSD, camera raw, and Targa.
    ///
    /// A UDIM pattern is refused: OpenImageIO's thumbnail path would try to
    /// open the literal pattern as a file, and the failure permanently marks
    /// the pattern's cache record broken, poisoning every later query on it.
    /// An unreadable file is an error, not `None` — OpenImageIO reports the
    /// brokenness only on the first touch, so it is asked directly here.
    pub fn thumbnail(&self, image_path: &Path, subimage: u32) -> Result<Option<ImageBuf>> {
        const OPERATION: &str = "read thumbnail";
        if self
            .info_int(image_path, 0, 0, "UDIM", OPERATION)?
            .unwrap_or(0)
            != 0
        {
            return Err(Error::operation(
                OPERATION,
                "a UDIM pattern names many files; resolve a concrete tile first \
                 (OpenImageIO would try to open the pattern itself, and the \
                 failure poisons its cache record)"
                    .to_owned(),
            ));
        }
        let filename = path_to_utf8(image_path)?;
        let subimage = level_index(subimage)?;
        let mut thumb = ImageBuf::empty()?;
        let (filled, error) = self.with_cache(|mut cache| {
            // Drain any queued message first, so a failure here reports this
            // call's error and not a predecessor's.
            let _ = sys::imagecache::imagecache_geterror(cache.as_mut(), true);
            let filled = sys::imagecache::imagecache_get_thumbnail(
                cache.as_mut(),
                filename,
                thumb.inner_mut(),
                subimage,
            );
            let error = if filled {
                String::new()
            } else {
                sys::imagecache::imagecache_geterror(cache, true)
            };
            (filled, error)
        });
        match (filled, error) {
            (true, _) => Ok(Some(thumb)),
            (false, error) if error.is_empty() => {
                // False without a message is how OpenImageIO reports both "no
                // thumbnail" and "file already known broken" — it only issues
                // the brokenness error on the first touch. Tell them apart.
                if self
                    .info_int(image_path, 0, 0, "broken", OPERATION)?
                    .unwrap_or(0)
                    != 0
                {
                    return Err(Error::operation(
                        OPERATION,
                        "the file cannot be read".to_owned(),
                    ));
                }
                Ok(None)
            }
            (false, error) => Err(Error::operation(OPERATION, error)),
        }
    }

    /// One integer image query; `Ok(None)` when the file cannot answer it.
    fn info_int(
        &self,
        image_path: &Path,
        subimage: u32,
        mip_level: u32,
        dataname: &'static str,
        operation: &'static str,
    ) -> Result<Option<i32>> {
        let mut value = 0_i32;
        let datatype =
            sys::typedesc::typedesc_from_basetype_arraylen(sys::typedesc::BaseType::Int32, 0);
        // SAFETY: the buffer is one i32 and the declared type is one 32-bit
        // integer, so OpenImageIO writes at most four bytes into it.
        let filled = unsafe {
            self.info_query(
                image_path,
                subimage,
                mip_level,
                dataname,
                datatype,
                (&mut value as *mut i32).cast::<u8>(),
                operation,
            )?
        };
        Ok(filled.then_some(value))
    }

    /// One float-array image query of `count` values; `Ok(None)` when the
    /// file cannot answer it.
    fn info_floats(
        &self,
        image_path: &Path,
        subimage: u32,
        count: u32,
        dataname: &'static str,
        operation: &'static str,
    ) -> Result<Option<Vec<f32>>> {
        let mut values = vec![0.0_f32; count.max(1) as usize];
        // A scalar for one value, because the alpha queries compare against
        // exactly that; an array of `count` otherwise.
        let arraylen = if count == 1 {
            0
        } else {
            count.min(i32::MAX as u32) as i32
        };
        let datatype = sys::typedesc::typedesc_from_basetype_arraylen(
            sys::typedesc::BaseType::Float32,
            arraylen,
        );
        // SAFETY: the buffer holds exactly as many f32 values as the declared
        // type describes, and OpenImageIO zero-pads channels past the image's
        // own rather than reading past either side.
        let filled = unsafe {
            self.info_query(
                image_path,
                subimage,
                0,
                dataname,
                datatype,
                values.as_mut_ptr().cast::<u8>(),
                operation,
            )?
        };
        Ok(filled.then_some(values))
    }

    /// One string image query. The queries answered as strings always have an
    /// answer for a readable file, so absence is reported as an error.
    ///
    /// UDIM patterns are refused before the query: OpenImageIO aggregates a
    /// pattern's answer by copying a stack buffer that, when every populated
    /// tile has become unreadable, was never written — for a string query
    /// that garbage would be dereferenced as a pointer. The aggregate of a
    /// string over many tiles is not a meaningful answer anyway; callers
    /// query a concrete tile.
    fn info_string(
        &self,
        image_path: &Path,
        subimage: u32,
        mip_level: u32,
        dataname: &'static str,
        operation: &'static str,
    ) -> Result<String> {
        if self
            .info_int(image_path, 0, 0, "UDIM", operation)?
            .unwrap_or(0)
            != 0
        {
            return Err(Error::operation(
                operation,
                "a UDIM pattern names many files; query a concrete tile instead".to_owned(),
            ));
        }
        let mut pointer: *const std::os::raw::c_char = std::ptr::null();
        let datatype =
            sys::typedesc::typedesc_from_basetype_arraylen(sys::typedesc::BaseType::String, 0);
        // SAFETY: the buffer is one pointer and the declared type is one
        // string, which OpenImageIO answers by storing one `char` pointer to
        // a `ustring`'s storage — immortal by design, so reading it after the
        // call is sound.
        let filled = unsafe {
            self.info_query(
                image_path,
                subimage,
                mip_level,
                dataname,
                datatype,
                (&mut pointer as *mut *const std::os::raw::c_char).cast::<u8>(),
                operation,
            )?
        };
        if !filled || pointer.is_null() {
            return Err(Error::operation(
                operation,
                "OpenImageIO did not answer the query".to_owned(),
            ));
        }
        // SAFETY: the pointer is non-null and points at a nul-terminated
        // ustring that is never freed.
        let answer = unsafe { std::ffi::CStr::from_ptr(pointer) };
        Ok(answer.to_string_lossy().into_owned())
    }

    /// Ask OpenImageIO one image query. `Ok(true)` means `data` was filled,
    /// `Ok(false)` that the file cannot answer this query (OpenImageIO says
    /// so by failing without a message), and `Err` carries a reported error.
    ///
    /// # Safety
    /// `data` must point at storage laid out exactly as `datatype` describes.
    #[allow(clippy::too_many_arguments)]
    unsafe fn info_query(
        &self,
        image_path: &Path,
        subimage: u32,
        mip_level: u32,
        dataname: &'static str,
        datatype: sys::typedesc::TypeDesc,
        data: *mut u8,
        operation: &'static str,
    ) -> Result<bool> {
        let filename = path_to_utf8(image_path)?;
        let subimage = level_index(subimage)?;
        let mip_level = level_index(mip_level)?;
        let (filled, error) = self.with_cache(|mut cache| {
            // Drain any queued message first: OpenImageIO's UDIM aggregation
            // can queue a tile's error and still succeed, and a later
            // unrelated query must not inherit it.
            let _ = sys::imagecache::imagecache_geterror(cache.as_mut(), true);
            // SAFETY: forwarded from the caller.
            let filled = unsafe {
                sys::imagecache::imagecache_get_image_info(
                    cache.as_mut(),
                    filename,
                    subimage,
                    mip_level,
                    dataname,
                    datatype,
                    data,
                )
            };
            let error = if filled {
                String::new()
            } else {
                sys::imagecache::imagecache_geterror(cache, true)
            };
            (filled, error)
        });
        match (filled, error) {
            (true, _) => Ok(true),
            (false, error) if error.is_empty() => Ok(false),
            (false, error) => Err(Error::operation(operation, error)),
        }
    }

    /// Return basic cache statistics suitable for diagnostics.
    ///
    /// Exclusive for the same reason as [`ImageCache::invalidate`]:
    /// OpenImageIO's statistics gathering is the other operation it does not
    /// synchronize. It walks every file's subimage vector with no lock — a
    /// vector a concurrent first open on another thread is resizing — and it
    /// merges per-thread counters their owner threads update with no lock, so
    /// reading statistics during reads is a data race, not a snapshot.
    pub fn stats(&mut self) -> String {
        self.with_cache(|cache| sys::imagecache::imagecache_getstats(cache, 1))
    }

    /// Reset accumulated cache statistics.
    ///
    /// Exclusive for the reasons given on [`ImageCache::stats`] — this one
    /// writes into every thread's live counters.
    pub fn reset_stats(&mut self) {
        self.with_cache(sys::imagecache::imagecache_reset_stats);
    }

    fn convert_spec(
        spec: cxx::UniquePtr<sys::imageio::ImageSpec>,
        error: String,
        operation: &'static str,
    ) -> Result<ImageSpec> {
        match spec.as_ref() {
            Some(spec) if sys::imageio::imagespec_valid(spec) => ImageSpec::from_sys(spec),
            _ => Err(Error::operation(operation, error)),
        }
    }

    fn with_cache<R>(
        &self,
        operation: impl FnOnce(std::pin::Pin<&mut sys::imagecache::ImageCache>) -> R,
    ) -> R {
        let mut cache = self.inner.clone();
        // SAFETY: ImageCache is an opaque C++ type whose non-const operations
        // are documented as thread-safe. The pinned reference is confined to
        // this call, which is the special case allowed by CXX.
        operation(unsafe { cache.pin_mut_unchecked() })
    }
}

/// Per-thread cache state, which speeds up repeated lookups.
///
/// OpenImageIO states that one of these "should NEVER be shared between
/// running threads", so this type is deliberately neither [`Send`] nor
/// [`Sync`]: it cannot leave the thread that created it. Passing it is always
/// optional — the cache keeps its own per-thread record otherwise.
pub struct Perthread<'cache> {
    cache: &'cache ImageCache,
    inner: *mut sys::imagecache::Perthread,
    /// Belt and braces: the raw pointer already prevents `Send` and `Sync`.
    _not_thread_safe: std::marker::PhantomData<*const ()>,
}

impl Drop for Perthread<'_> {
    fn drop(&mut self) {
        let inner = self.inner;
        self.cache.with_cache(|cache| {
            // SAFETY: this pointer came from create_thread_info on this same
            // cache and is destroyed exactly once, here.
            unsafe { sys::imagecache::imagecache_destroy_thread_info(cache, inner) };
        });
    }
}

impl std::fmt::Debug for Perthread<'_> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.debug_struct("Perthread").finish_non_exhaustive()
    }
}

/// A file name already resolved against an [`ImageCache`].
///
/// Reading through a handle skips the file-name lookup that every by-name call
/// performs, which matters when a single image is read many times.
///
/// A handle borrows the cache, so it cannot outlive it.
pub struct ImageHandle<'cache> {
    cache: &'cache ImageCache,
    inner: *mut sys::imagecache::ImageHandle,
}

// SAFETY: a handle is a resolved reference to file state the cache owns, and
// every operation on it goes through the cache, whose operations OpenImageIO
// documents as thread-safe. OpenImageIO's own API pairs a shared handle with
// per-thread state — see `Perthread`, which is deliberately not shareable.
unsafe impl Send for ImageHandle<'_> {}
unsafe impl Sync for ImageHandle<'_> {}

impl<'cache> ImageHandle<'cache> {
    /// The file name this handle resolved to.
    pub fn filename(&self) -> String {
        let inner = self.inner;
        self.cache.with_cache(|cache| {
            // SAFETY: the handle is live for as long as the borrow of `cache`.
            unsafe { sys::imagecache::imagecache_filename_from_handle(cache, inner) }
        })
    }

    /// Whether the cache could open and read the file.
    pub fn is_good(&self) -> bool {
        let inner = self.inner;
        self.cache.with_cache(|cache| {
            // SAFETY: as in `filename`.
            unsafe { sys::imagecache::imagecache_good(cache, inner) }
        })
    }

    /// Read a pixel region from the base image.
    pub fn get_pixels_into<T: Pixel>(&self, roi: Roi, pixels: &mut [T]) -> Result<()> {
        self.get_pixels_into_at(0, 0, roi, pixels)
    }

    /// Read a pixel region from a subimage and mip level.
    pub fn get_pixels_into_at<T: Pixel>(
        &self,
        subimage: u32,
        mip_level: u32,
        roi: Roi,
        pixels: &mut [T],
    ) -> Result<()> {
        self.read(None, subimage, mip_level, roi, pixels)
    }

    /// Read a pixel region using caller-managed per-thread state.
    pub fn get_pixels_into_with<T: Pixel>(
        &self,
        thread_state: &Perthread<'cache>,
        subimage: u32,
        mip_level: u32,
        roi: Roi,
        pixels: &mut [T],
    ) -> Result<()> {
        // A shared lifetime is not a shared identity. Both types are covariant
        // in `'cache`, so two caches alive in the same scope give a Perthread
        // and a handle a common lifetime and this call type-checks. It must not
        // go through: a record is registered only with the cache that created
        // it, so the other cache's invalidation can never purge it, and at
        // teardown the record still holds a tile reference into a cache that is
        // gone.
        if !std::ptr::eq(self.cache, thread_state.cache) {
            return Err(Error::operation(
                "read cached pixels",
                "the per-thread state belongs to a different image cache".to_owned(),
            ));
        }
        self.read(Some(thread_state), subimage, mip_level, roi, pixels)
    }

    fn read<T: Pixel>(
        &self,
        thread_state: Option<&Perthread<'cache>>,
        subimage: u32,
        mip_level: u32,
        roi: Roi,
        pixels: &mut [T],
    ) -> Result<()> {
        let subimage_index = level_index(subimage)?;
        let mip_level_index = level_index(mip_level)?;
        validate_buffer_len(roi.element_count()?, pixels.len())?;

        let inner = self.inner;
        let thread_info = thread_state.map_or(std::ptr::null_mut(), |state| state.inner);

        // The shim refuses a deep image too, since that is where the guard has
        // to be, but it can only report a message. Ask first so this path
        // returns the same typed error the by-name read does.
        let deep = self.cache.with_cache(|cache| {
            // SAFETY: the handle and per-thread record belong to this cache and
            // outlive the call.
            unsafe {
                sys::imagecache::imagecache_handle_is_deep(
                    cache,
                    inner,
                    thread_info,
                    subimage_index,
                )
            }
        });
        if deep == 1 {
            return Err(Error::UnsupportedDeepImage);
        }
        let sys_roi = roi.to_sys();
        let mut error = String::new();
        let succeeded = self.cache.with_cache(|cache| {
            // SAFETY: the handle and per-thread record both belong to this
            // cache and outlive the call; Pixel is sealed to initialized
            // scalar layouts whose byte extent the shim re-checks.
            unsafe {
                sys::imagecache::imagecache_get_pixels_handle_span_with_error(
                    cache,
                    inner,
                    thread_info,
                    subimage_index,
                    mip_level_index,
                    &sys_roi,
                    pixel::type_desc::<T>(),
                    pixel::as_bytes_mut(pixels),
                    &mut error,
                )
            }
        });
        if succeeded {
            Ok(())
        } else {
            Err(Error::operation("read cached pixels", error))
        }
    }
}

impl std::fmt::Debug for ImageHandle<'_> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ImageHandle")
            .field("filename", &self.filename())
            .field("good", &self.is_good())
            .finish()
    }
}

/// A tile borrowed from an [`ImageCache`], released when dropped.
///
/// Holding a tile pins it in the cache. Dropping the guard is what releases
/// it, so a leaked guard is a leaked tile.
pub struct TileGuard<'cache> {
    cache: &'cache ImageCache,
    inner: *mut sys::imagecache::Tile,
    roi: Roi,
    format: PixelFormat,
    element_count: usize,
}

impl Drop for TileGuard<'_> {
    fn drop(&mut self) {
        let inner = self.inner;
        self.cache.with_cache(|cache| {
            // SAFETY: this tile came from this cache and is released exactly
            // once, here.
            unsafe { sys::imagecache::imagecache_release_tile(cache, inner) };
        });
    }
}

impl TileGuard<'_> {
    /// The region this tile covers, which may extend past the data window.
    ///
    /// Recorded when the tile was borrowed. OpenImageIO derives a tile's region
    /// from the *file's* current spec rather than from the tile itself, so
    /// asking it afterwards would answer out of state that invalidation frees.
    pub fn roi(&self) -> Roi {
        self.roi
    }

    /// The pixel format the tile holds.
    ///
    /// This is the format the *cache* stores the tile in, which is the file's
    /// own only for uint8, uint16 and half; everything else is promoted to
    /// float. It is what [`TileGuard::pixels`] requires, so the two agree by
    /// construction. Recorded alongside [`TileGuard::roi`], for the same
    /// reason.
    pub fn format(&self) -> PixelFormat {
        self.format
    }

    /// Borrow the tile's pixels.
    ///
    /// `T` must match [`TileGuard::format`], which is the format the cache
    /// stores rather than the file's; no conversion happens here. The slice
    /// covers
    /// [`TileGuard::roi`] and borrows the guard, so it cannot outlive the
    /// tile.
    pub fn pixels<T: Pixel>(&self) -> Result<&[T]> {
        let inner = self.inner;
        let mut format = pixel::type_desc::<T>();
        let data = self.cache.with_cache(|cache| {
            // SAFETY: the tile is live, and `format` is an out parameter that
            // reports the format actually stored.
            unsafe { sys::imagecache::imagecache_tile_pixels(cache, inner, &mut format) }
        });

        let actual = PixelFormat::from_sys(&format);
        if actual != T::FORMAT {
            return Err(Error::TilePixelFormat {
                requested: T::FORMAT,
                actual,
            });
        }
        if data.is_null() {
            return Err(Error::operation(
                "borrow tile pixels",
                "OpenImageIO returned no pixel data for the tile".to_owned(),
            ));
        }

        // SAFETY: the format was just confirmed to be T's, the element count
        // was computed from the region recorded when the tile was borrowed, and
        // the slice borrows `self`, so it cannot outlive the tile it points
        // into. Borrowing the guard also borrows the cache, which is what stops
        // `invalidate` from running underneath this slice.
        Ok(unsafe { std::slice::from_raw_parts(data.cast::<T>(), self.element_count) })
    }
}

impl std::fmt::Debug for TileGuard<'_> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("TileGuard")
            .field("roi", &self.roi)
            .field("format", &self.format)
            .finish_non_exhaustive()
    }
}

/// Typed configuration for constructing an [`ImageCache`].
#[derive(Debug, Clone, Default)]
pub struct ImageCacheBuilder {
    max_memory_mb: Option<f32>,
    max_open_files: Option<u32>,
    autotile: Option<u32>,
    unassociated_alpha: Option<bool>,
}

impl ImageCacheBuilder {
    /// Set the approximate memory budget for the internal tile cache, in MB.
    pub fn max_memory_mb(mut self, megabytes: f32) -> Self {
        self.max_memory_mb = Some(megabytes);
        self
    }

    /// Set the approximate maximum number of simultaneously open files.
    pub fn max_open_files(mut self, count: u32) -> Self {
        self.max_open_files = Some(count);
        self
    }

    /// Configure virtual tile dimensions for scanline images. Zero disables
    /// virtual tiling.
    pub fn autotile(mut self, size: u32) -> Self {
        self.autotile = Some(size);
        self
    }

    /// Preserve unassociated-alpha input instead of automatically associating
    /// its color channels during reads.
    pub fn unassociated_alpha(mut self, enabled: bool) -> Self {
        self.unassociated_alpha = Some(enabled);
        self
    }

    /// Construct and configure the cache.
    pub fn build(self) -> Result<ImageCache> {
        if let Some(value) = self.max_memory_mb {
            if !value.is_finite() || value <= 0.0 {
                return Err(invalid_setting("max_memory_mb", value));
            }
        }
        if self.max_open_files == Some(0) {
            return Err(invalid_setting("max_open_files", 0));
        }

        let cache = ImageCache {
            // Always private. The Sync justification above says why the
            // process-wide cache is not offered.
            inner: sys::imagecache::imagecache_create(false),
        };
        if cache.inner.is_null() {
            return Err(Error::operation(
                "create image cache",
                "OpenImageIO returned a null cache".to_owned(),
            ));
        }

        if let Some(value) = self.max_memory_mb {
            cache.set_attribute_float("max_memory_MB", value)?;
        }
        if let Some(value) = self.max_open_files {
            cache.set_attribute_int("max_open_files", setting_i32("max_open_files", value)?)?;
        }
        if let Some(value) = self.autotile {
            cache.set_attribute_int("autotile", setting_i32("autotile", value)?)?;
        }
        if let Some(value) = self.unassociated_alpha {
            cache.set_attribute_int("unassociatedalpha", i32::from(value))?;
        }

        Ok(cache)
    }
}

impl ImageCache {
    fn set_attribute_int(&self, name: &'static str, value: i32) -> Result<()> {
        let mut error = String::new();
        let succeeded = self.with_cache(|cache| {
            sys::imagecache::imagecache_attribute_int_with_error(cache, name, value, &mut error)
        });
        if succeeded {
            Ok(())
        } else {
            Err(Error::operation("configure image cache", error))
        }
    }

    fn set_attribute_float(&self, name: &'static str, value: f32) -> Result<()> {
        let mut error = String::new();
        let succeeded = self.with_cache(|cache| {
            sys::imagecache::imagecache_attribute_float_with_error(cache, name, value, &mut error)
        });
        if succeeded {
            Ok(())
        } else {
            Err(Error::operation("configure image cache", error))
        }
    }
}

fn setting_i32(name: &'static str, value: u32) -> Result<i32> {
    i32::try_from(value).map_err(|_| invalid_setting(name, value))
}

fn invalid_setting(name: &'static str, value: impl ToString) -> Error {
    Error::InvalidCacheSetting {
        name,
        value: value.to_string(),
    }
}

fn level_index(index: u32) -> Result<i32> {
    i32::try_from(index)
        .map_err(|_| Error::InvalidImageSpec("image level index exceeds i32::MAX".to_owned()))
}
