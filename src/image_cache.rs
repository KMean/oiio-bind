use std::path::Path;

use crate::{
    imageio::validate_buffer_len, path_to_utf8, pixel, sys, Error, ImageSpec, Pixel, Result, Roi,
};

/// A thread-safe OpenImageIO image cache.
///
/// Use [`ImageCache::builder`] to configure a private cache. Private caches do
/// not interfere with process-wide cache state.
pub struct ImageCache {
    inner: cxx::SharedPtr<sys::imagecache::ImageCache>,
}

// OIIO documents ImageCache operations as thread-safe. Every method clones the
// shared_ptr and uses CXX's opaque-type exception for thread-safe non-const C++
// methods; no Rust reference to the C++ object escapes a call.
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

    /// Invalidate one cached image. The next access will re-read it.
    pub fn invalidate(&self, image_path: &Path, force: bool) -> Result<()> {
        let filename = path_to_utf8(image_path)?;
        self.with_cache(|cache| {
            sys::imagecache::imagecache_invalidate(cache, filename, force);
        });
        Ok(())
    }

    /// Invalidate every image held by this cache.
    pub fn invalidate_all(&self, force: bool) {
        self.with_cache(|cache| {
            sys::imagecache::imagecache_invalidate_all(cache, force);
        });
    }

    /// Return basic cache statistics suitable for diagnostics.
    pub fn stats(&self) -> String {
        self.with_cache(|cache| sys::imagecache::imagecache_getstats(cache, 1))
    }

    /// Reset accumulated cache statistics.
    pub fn reset_stats(&self) {
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

/// Typed configuration for constructing an [`ImageCache`].
#[derive(Debug, Clone, Default)]
pub struct ImageCacheBuilder {
    shared: bool,
    max_memory_mb: Option<f32>,
    max_open_files: Option<u32>,
    autotile: Option<u32>,
    unassociated_alpha: Option<bool>,
}

impl ImageCacheBuilder {
    /// Opt into OpenImageIO's process-wide shared cache.
    ///
    /// The default is a private cache. Shared cache settings and invalidation
    /// may affect unrelated users in the same process.
    pub fn shared(mut self, shared: bool) -> Self {
        self.shared = shared;
        self
    }

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
            inner: sys::imagecache::imagecache_create(self.shared),
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
