//! Filtered texture lookups.
//!
//! A [`TextureSystem`] answers "what colour is this texture at this point",
//! doing the mip selection and filtering that a renderer would otherwise
//! write itself. It reads through an image cache, so the same file serves
//! many lookups without being re-read.

use std::path::Path;

use crate::{path_to_utf8, sys, Error, Result};

/// What happens to texture coordinates outside the unit square.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[non_exhaustive]
pub enum WrapMode {
    /// Whatever the file itself asks for.
    #[default]
    Default,
    /// Black outside the image.
    Black,
    /// Repeat the edge pixel.
    Clamp,
    /// Tile the image.
    Periodic,
    /// Tile, mirroring alternate copies.
    Mirror,
}

impl WrapMode {
    fn code(self) -> i32 {
        match self {
            Self::Default => 0,
            Self::Black => 1,
            Self::Clamp => 2,
            Self::Periodic => 3,
            Self::Mirror => 4,
        }
    }
}

/// How mip levels are chosen.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[non_exhaustive]
pub enum MipMode {
    /// OpenImageIO's own choice, which is the anisotropic path.
    #[default]
    Default,
    /// Ignore the mip pyramid and always read the highest resolution.
    None,
    /// One level, chosen per lookup.
    OneLevel,
    /// Blend two levels.
    Trilinear,
    /// Blend two levels with anisotropic filtering.
    Anisotropic,
}

impl MipMode {
    fn code(self) -> i32 {
        match self {
            Self::Default => 0,
            Self::None => 1,
            Self::OneLevel => 2,
            Self::Trilinear => 3,
            Self::Anisotropic => 4,
        }
    }
}

/// How samples are interpolated inside one mip level.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[non_exhaustive]
pub enum InterpolationMode {
    /// Nearest texel.
    Closest,
    /// Bilinear.
    Bilinear,
    /// Bicubic.
    Bicubic,
    /// Bicubic when magnifying, bilinear otherwise.
    #[default]
    SmartBicubic,
}

impl InterpolationMode {
    fn code(self) -> i32 {
        match self {
            Self::Closest => 0,
            Self::Bilinear => 1,
            Self::Bicubic => 2,
            Self::SmartBicubic => 3,
        }
    }
}

/// How one lookup should be filtered.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TextureOptions {
    /// First channel of the texture to read.
    pub first_channel: u32,
    /// Which subimage or cube face to read.
    pub subimage: u32,
    /// Wrapping in the s direction.
    pub s_wrap: WrapMode,
    /// Wrapping in the t direction.
    pub t_wrap: WrapMode,
    /// How mip levels are chosen.
    pub mip_mode: MipMode,
    /// How samples are interpolated within a level.
    pub interpolation: InterpolationMode,
    /// Extra blur in s, in texture coordinates.
    pub s_blur: f32,
    /// Extra blur in t, in texture coordinates.
    pub t_blur: f32,
    /// Multiplier on the filter width in s.
    pub s_width: f32,
    /// Multiplier on the filter width in t.
    pub t_width: f32,
    /// Value for channels the texture does not have.
    pub fill: f32,
}

impl Default for TextureOptions {
    fn default() -> Self {
        Self {
            first_channel: 0,
            subimage: 0,
            s_wrap: WrapMode::default(),
            t_wrap: WrapMode::default(),
            mip_mode: MipMode::default(),
            interpolation: InterpolationMode::default(),
            s_blur: 0.0,
            t_blur: 0.0,
            s_width: 1.0,
            t_width: 1.0,
            fill: 0.0,
        }
    }
}

impl TextureOptions {
    fn to_sys(self) -> Result<sys::texture::TextureLookupOptions> {
        Ok(sys::texture::TextureLookupOptions {
            first_channel: i32::try_from(self.first_channel).map_err(|_| {
                Error::InvalidImageSpec("first channel exceeds i32::MAX".to_owned())
            })?,
            subimage: i32::try_from(self.subimage)
                .map_err(|_| Error::InvalidImageSpec("subimage exceeds i32::MAX".to_owned()))?,
            s_wrap: self.s_wrap.code(),
            t_wrap: self.t_wrap.code(),
            mip_mode: self.mip_mode.code(),
            interp_mode: self.interpolation.code(),
            s_blur: self.s_blur,
            t_blur: self.t_blur,
            s_width: self.s_width,
            t_width: self.t_width,
            fill: self.fill,
        })
    }
}

/// How far the texture coordinate moves per screen pixel.
///
/// This is what tells a texture system how much of the texture one pixel
/// covers, and so which mip level to read and how wide to filter. A renderer
/// has these from its own differentials; for simpler uses,
/// [`Derivatives::uniform`] describes a square footprint.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Derivatives {
    /// Change in s per pixel across the screen's x axis.
    pub dsdx: f32,
    /// Change in t per pixel across the screen's x axis.
    pub dtdx: f32,
    /// Change in s per pixel across the screen's y axis.
    pub dsdy: f32,
    /// Change in t per pixel across the screen's y axis.
    pub dtdy: f32,
}

impl Derivatives {
    /// A square footprint `width` across in both directions.
    ///
    /// For a texture drawn at its own resolution this is one texel, which is
    /// `1.0 / width_in_pixels`.
    pub fn uniform(width: f32) -> Self {
        Self {
            dsdx: width,
            dtdx: 0.0,
            dsdy: 0.0,
            dtdy: width,
        }
    }

    /// No footprint at all, which asks for a point sample of the highest
    /// resolution level.
    pub fn point() -> Self {
        Self::default()
    }
}

/// A filtered texture reader.
///
/// ```no_run
/// use oiio::{Derivatives, TextureOptions, TextureSystem};
/// use std::path::Path;
///
/// # fn main() -> oiio::Result<()> {
/// let textures = TextureSystem::new()?;
/// let options = TextureOptions::default();
///
/// // The derivatives say how far the texture coordinate moves per screen
/// // pixel, which is what selects a mip level and a filter width.
/// let mut rgb = [0.0_f32; 3];
/// textures.texture(
///     Path::new("texture.tx"),
///     &options,
///     0.5, 0.5,
///     Derivatives::uniform(1.0 / 512.0),
///     &mut rgb,
/// )?;
/// # Ok(())
/// # }
/// ```
pub struct TextureSystem {
    inner: cxx::SharedPtr<sys::texture::TextureSystem>,
}

// SAFETY: as with ImageCache, OpenImageIO documents texture lookups as
// thread-safe, every method here confines its pinned reference to one call,
// and no Rust reference to the C++ object escapes.
unsafe impl Send for TextureSystem {}
unsafe impl Sync for TextureSystem {}

impl TextureSystem {
    /// A private texture system, which does not share state with others.
    pub fn new() -> Result<Self> {
        Self::create(false)
    }

    /// The process-wide shared texture system.
    ///
    /// Its settings and invalidations affect every other user in the process.
    pub fn shared() -> Result<Self> {
        Self::create(true)
    }

    /// Look up one filtered sample.
    ///
    /// `s` and `t` are texture coordinates, conventionally in `0..1`. The four
    /// derivatives say how far those coordinates move per screen pixel, and
    /// are what choose a mip level and filter width; passing zeros asks for
    /// an unfiltered sample of the highest resolution.
    ///
    /// One value per channel is written to `result`, whose length decides how
    /// many channels are read.
    pub fn texture(
        &self,
        texture_path: &Path,
        options: &TextureOptions,
        s: f32,
        t: f32,
        derivatives: Derivatives,
        result: &mut [f32],
    ) -> Result<()> {
        if result.is_empty() {
            return Err(Error::InvalidRoi(
                "a texture lookup needs at least one channel".to_owned(),
            ));
        }
        let filename = path_to_utf8(texture_path)?;
        let options = options.to_sys()?;

        let succeeded = self.with_system(|system| {
            sys::texture::texturesystem_texture(
                system,
                filename,
                &options,
                s,
                t,
                derivatives.dsdx,
                derivatives.dtdx,
                derivatives.dsdy,
                derivatives.dtdy,
                result,
            )
        });
        if succeeded {
            Ok(())
        } else {
            Err(Error::operation(
                "texture lookup",
                self.with_system(sys::texture::texturesystem_geterror),
            ))
        }
    }

    /// The texture's resolution, as the texture system sees it.
    pub fn resolution(&self, texture_path: &Path) -> Result<[u32; 2]> {
        let filename = path_to_utf8(texture_path)?;
        let mut resolution = [0_i32; 2];
        let succeeded = self.with_system(|system| {
            sys::texture::texturesystem_resolution(system, filename, &mut resolution)
        });
        if !succeeded {
            return Err(Error::OpenImage {
                path: texture_path.to_path_buf(),
                message: self.with_system(sys::texture::texturesystem_geterror),
            });
        }
        Ok([resolution[0].max(0) as u32, resolution[1].max(0) as u32])
    }

    /// Set the approximate memory budget for the underlying cache, in MB.
    pub fn set_max_memory_mb(&self, megabytes: f32) -> Result<()> {
        if !megabytes.is_finite() || megabytes <= 0.0 {
            return Err(Error::InvalidCacheSetting {
                name: "max_memory_MB",
                value: megabytes.to_string(),
            });
        }
        let succeeded = self.with_system(|system| {
            sys::texture::texturesystem_attribute_float(system, "max_memory_MB", megabytes)
        });
        if succeeded {
            Ok(())
        } else {
            Err(Error::operation(
                "configure texture system",
                self.with_system(sys::texture::texturesystem_geterror),
            ))
        }
    }

    /// Set the maximum number of simultaneously open files.
    pub fn set_max_open_files(&self, count: u32) -> Result<()> {
        let count = i32::try_from(count).map_err(|_| Error::InvalidCacheSetting {
            name: "max_open_files",
            value: count.to_string(),
        })?;
        let succeeded = self.with_system(|system| {
            sys::texture::texturesystem_attribute_int(system, "max_open_files", count)
        });
        if succeeded {
            Ok(())
        } else {
            Err(Error::operation(
                "configure texture system",
                self.with_system(sys::texture::texturesystem_geterror),
            ))
        }
    }

    /// Forget one texture, so the next lookup re-reads it.
    pub fn invalidate(&self, texture_path: &Path, force: bool) -> Result<()> {
        let filename = path_to_utf8(texture_path)?;
        self.with_system(|system| {
            sys::texture::texturesystem_invalidate(system, filename, force);
        });
        Ok(())
    }

    /// Forget every texture.
    pub fn invalidate_all(&self, force: bool) {
        self.with_system(|system| {
            sys::texture::texturesystem_invalidate_all(system, force);
        });
    }

    /// Statistics suitable for diagnostics.
    pub fn stats(&self) -> String {
        let inner = self.inner.clone();
        let Some(system) = inner.as_ref() else {
            return String::new();
        };
        sys::texture::texturesystem_getstats(system, 1)
    }

    fn create(shared: bool) -> Result<Self> {
        let inner = sys::texture::texturesystem_create(shared);
        if inner.is_null() {
            return Err(Error::operation(
                "create texture system",
                "OpenImageIO returned a null texture system".to_owned(),
            ));
        }
        Ok(Self { inner })
    }

    fn with_system<R>(
        &self,
        operation: impl FnOnce(std::pin::Pin<&mut sys::texture::TextureSystem>) -> R,
    ) -> R {
        let mut system = self.inner.clone();
        // SAFETY: TextureSystem is an opaque C++ type whose operations are
        // documented as thread-safe. The pinned reference is confined to this
        // call, which is the special case CXX allows.
        operation(unsafe { system.pin_mut_unchecked() })
    }
}

impl std::fmt::Debug for TextureSystem {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("TextureSystem")
            .finish_non_exhaustive()
    }
}
