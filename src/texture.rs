//! Making textures, and looking them up.
//!
//! [`make_texture`] turns an ordinary image into the tiled, MIP-mapped file a
//! renderer wants — the `.tx` that `maketx` and `oiiotool -otex` produce. A
//! [`TextureSystem`] then answers "what colour is this texture at this point",
//! doing the mip selection and filtering that a renderer would otherwise write
//! itself. It reads through an image cache, so the same file serves many
//! lookups without being re-read.

use std::path::{Path, PathBuf};

use crate::{path_to_utf8, sys, AttributeValue, Error, ImageBuf, PixelFormat, Result};

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

    /// The OpenImageIO name for this mode, as a texture file records it.
    pub fn name(self) -> &'static str {
        match self {
            Self::Default => "default",
            Self::Black => "black",
            Self::Clamp => "clamp",
            Self::Periodic => "periodic",
            Self::Mirror => "mirror",
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

/// What kind of texture [`make_texture`] should write.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[non_exhaustive]
pub enum TextureMode {
    /// An ordinary texture, for surfaces.
    #[default]
    Texture,
    /// A shadow map.
    Shadow,
    /// A latitude-longitude environment map, from an image already in that
    /// layout.
    LatLongEnvironment,
    /// A latitude-longitude environment map, from a light probe image, which
    /// is reprojected on the way.
    LightProbe,
    /// A bump map that also carries the slopes derived from it.
    BumpWithSlopes,
}

impl TextureMode {
    fn code(self) -> i32 {
        match self {
            Self::Texture => 0,
            Self::Shadow => 1,
            Self::LatLongEnvironment => 2,
            Self::LightProbe => 3,
            Self::BumpWithSlopes => 4,
        }
    }
}

/// How [`make_texture`] should write a texture.
///
/// Every setting has a default that matches OpenImageIO's own, so
/// `TextureConfig::default()` writes what `maketx` with no options would.
///
/// ```no_run
/// use oiio::{make_texture, PixelFormat, TextureConfig, TextureMode, WrapMode};
/// use std::path::Path;
///
/// # fn main() -> oiio::Result<()> {
/// let config = TextureConfig::new()
///     .with_format(PixelFormat::F16)
///     .with_wrap_modes(WrapMode::Periodic, WrapMode::Periodic)
///     .with_filter("lanczos3");
///
/// make_texture(
///     TextureMode::Texture,
///     Path::new("source.exr"),
///     Path::new("texture.tx"),
///     &config,
/// )?;
/// # Ok(())
/// # }
/// ```
#[derive(Debug, Clone, Default, PartialEq)]
pub struct TextureConfig {
    format: Option<PixelFormat>,
    tile: Option<[u32; 3]>,
    attributes: Vec<(String, AttributeValue)>,
}

impl TextureConfig {
    /// A configuration with every OpenImageIO default in place.
    pub fn new() -> Self {
        Self::default()
    }

    /// The data format to write. Unset, the texture keeps the input's.
    ///
    /// The output format has the last word, and takes it silently. A `.tx` is
    /// a TIFF, and OpenImageIO's TIFF writer turns a request for `half` into
    /// `float` unless the `tiff:half` attribute is set; an OpenEXR turns a
    /// request for any integer format into `half`. Neither is an error, and
    /// neither is reported, so read the written file's specification back if
    /// the format matters.
    pub fn with_format(mut self, format: PixelFormat) -> Self {
        self.format = Some(format);
        self
    }

    /// Tile dimensions. OpenImageIO's default is 64x64x1.
    pub fn with_tile_size(mut self, dimensions: [u32; 3]) -> Self {
        self.tile = Some(dimensions);
        self
    }

    /// The compression the output format should use. Default: `"zip"`.
    pub fn with_compression(self, compression: &str) -> Self {
        self.with_attribute("compression", compression)
    }

    /// The wrap modes to record in the texture, which a lookup that asks for
    /// [`WrapMode::Default`] will then use. Default: black in both directions.
    pub fn with_wrap_modes(self, s_wrap: WrapMode, t_wrap: WrapMode) -> Self {
        let modes = format!("{},{}", s_wrap.name(), t_wrap.name());
        self.with_attribute("wrapmodes", modes)
    }

    /// The filter to resample with when building mip levels, such as
    /// `"lanczos3"` or `"box"`. Unset, OpenImageIO resamples bilinearly.
    pub fn with_filter(self, filter_name: &str) -> Self {
        self.with_attribute("maketx:filtername", filter_name)
    }

    /// Whether to build a mip pyramid at all. Default: true.
    ///
    /// A texture without one still reads, but every lookup pays for
    /// filtering the full resolution, which is the cost the pyramid exists to
    /// avoid.
    pub fn with_mipmap(self, enabled: bool) -> Self {
        self.with_attribute("maketx:nomipmap", i32::from(!enabled))
    }

    /// Whether to resize up to a power of two first. Default: false.
    pub fn with_resize_to_power_of_two(self, enabled: bool) -> Self {
        self.with_attribute("maketx:resize", i32::from(enabled))
    }

    /// Whether to shrink an image that is entirely one colour down to a tiny
    /// one. Default: false.
    pub fn with_constant_color_detect(self, enabled: bool) -> Self {
        self.with_attribute("maketx:constant_color_detect", i32::from(enabled))
    }

    /// Whether to collapse an RGB image whose channels are equal everywhere
    /// to a single channel. Default: false.
    pub fn with_monochrome_detect(self, enabled: bool) -> Self {
        self.with_attribute("maketx:monochrome_detect", i32::from(enabled))
    }

    /// Whether to drop an alpha channel that is 1.0 in every pixel.
    /// Default: false.
    pub fn with_opaque_detect(self, enabled: bool) -> Self {
        self.with_attribute("maketx:opaque_detect", i32::from(enabled))
    }

    /// Whether to divide colour by alpha before converting colour space and
    /// multiply it back after. Default: false.
    pub fn with_unpremult(self, enabled: bool) -> Self {
        self.with_attribute("maketx:unpremult", i32::from(enabled))
    }

    /// Convert from one colour space to another while building the texture.
    ///
    /// The names are the ones the active OpenColorIO configuration defines;
    /// [`ColorConfig`](crate::ColorConfig) reports them.
    pub fn with_color_conversion(self, from_space: &str, to_space: &str) -> Self {
        self.with_attribute("maketx:incolorspace", from_space)
            .with_attribute("maketx:outcolorspace", to_space)
    }

    /// How many channels the texture should have, padding with zeroes or
    /// dropping channels. Unset, it keeps the input's.
    ///
    /// Clamped to [`TextureConfig::MAX_CHANNELS`]. `make_texture` reaches
    /// `ImageBufAlgo::channels`, which builds the default channel order with
    /// `alloca`, so the count is four bytes of stack each and an unbounded one
    /// overflows the stack rather than failing.
    pub fn with_channel_count(self, channels: u32) -> Self {
        let channels = i32::try_from(channels.min(Self::MAX_CHANNELS)).unwrap_or(i32::MAX);
        self.with_attribute("maketx:nchannels", channels)
    }

    /// The largest channel count [`TextureConfig::with_channel_count`] will
    /// pass on. Far above any real texture, and far below what would put a
    /// stack-allocated channel order in danger.
    pub const MAX_CHANNELS: u32 = 1024;

    /// Sharpen by this contrast metric when building mip levels.
    /// Default: 0.0, meaning no sharpening.
    pub fn with_sharpen(self, amount: f32) -> Self {
        self.with_attribute("maketx:sharpen", amount)
    }

    /// Whether to compress and expand the range around each resize, which
    /// reduces the ringing a filter with negative lobes causes on high dynamic
    /// range images. Default: false.
    pub fn with_highlight_compensation(self, enabled: bool) -> Self {
        self.with_attribute("maketx:highlightcomp", i32::from(enabled))
    }

    /// Whether a NaN anywhere in the input is an error. Default: false.
    pub fn with_nan_check(self, enabled: bool) -> Self {
        self.with_attribute("maketx:checknan", i32::from(enabled))
    }

    /// Set any other configuration attribute by name.
    ///
    /// This is the escape hatch for the `maketx:` settings the methods above
    /// do not cover. Setting the same name twice keeps the last value.
    pub fn with_attribute(
        mut self,
        name: impl Into<String>,
        value: impl Into<AttributeValue>,
    ) -> Self {
        let name = name.into();
        let value = value.into();
        match self.attributes.iter_mut().find(|(key, _)| *key == name) {
            Some(existing) => existing.1 = value,
            None => self.attributes.push((name, value)),
        }
        self
    }

    /// Build the specification OpenImageIO reads the configuration out of.
    ///
    /// Only the data format, the tile size and the attributes are read; a
    /// configuration describes how to write a texture, not what the texture
    /// contains, so its resolution and channel count are left at zero.
    fn to_sys(&self) -> Result<cxx::UniquePtr<sys::imageio::ImageSpec>> {
        let mut spec = sys::imageio::imagespec_from_resolution(0, 0, 0);
        let Some(mut pinned) = spec.as_mut() else {
            return Err(Error::InvalidImageSpec(
                "OpenImageIO could not allocate an image specification".to_owned(),
            ));
        };

        if let Some(format) = self.format {
            if format == PixelFormat::Other {
                return Err(Error::InvalidImageSpec(
                    "a texture needs a concrete pixel format; leave it unset to keep the input's"
                        .to_owned(),
                ));
            }
            sys::imageio::imagespec_set_format(pinned.as_mut(), format.to_sys());
        }

        if let Some(tile) = self.tile {
            let dimension = |name: &'static str, value: u32| {
                i32::try_from(value).map_err(|_| {
                    Error::InvalidImageSpec(format!("{name} {value} exceeds i32::MAX"))
                })
            };
            sys::imageio::imagespec_set_tile_size(
                pinned.as_mut(),
                dimension("tile width", tile[0])?,
                dimension("tile height", tile[1])?,
                dimension("tile depth", tile[2])?,
            );
        }

        for (name, value) in &self.attributes {
            value.write(pinned.as_mut(), name)?;
        }

        // The image cache only honours a texture's `oiio:ConstantColor` and
        // `oiio:AverageColor` when the file's `Software` tag starts with
        // "OpenImageIO" or "maketx". The maketx and oiiotool executables
        // stamp one; the library call these wrappers use does not, so
        // API-made textures would silently lose their constant-color
        // metadata (`contrib/upstream-issues.md`, issue 14). Stamp it here
        // unless the caller chose their own.
        if !self.attributes.iter().any(|(name, _)| name == "Software") {
            AttributeValue::from("OpenImageIO oiio-bind").write(pinned.as_mut(), "Software")?;
        }

        Ok(spec)
    }
}

/// Write `input_path` as a texture at `output_path`.
///
/// The input is read with OpenImageIO's own texture pipeline, which is the
/// difference between this and reading the file yourself: it can stream an
/// image too large to hold in memory.
///
/// Errors are reported as OpenImageIO gives them — it has no destination image
/// to record them in, so the message comes from the global error channel and
/// from whatever the operation printed while refusing.
// SAFETY, for both `unsafe` blocks below: the `imagebufalgo_*` shims are
// declared `unsafe fn` because OpenImageIO trusts their arguments. These two
// take only file names, an already-built configuration spec, and an ImageBuf
// reference that cannot be null; there is no region involved.
pub fn make_texture(
    mode: TextureMode,
    input_path: &Path,
    output_path: &Path,
    config: &TextureConfig,
) -> Result<()> {
    let input = path_to_utf8(input_path)?;
    let output = path_to_utf8(output_path)?;
    let config = config.to_sys()?;
    let Some(config) = config.as_ref() else {
        return Err(Error::InvalidImageSpec(
            "OpenImageIO could not allocate an image specification".to_owned(),
        ));
    };

    let mut message = String::new();
    let succeeded = unsafe {
        sys::imagebufalgo::imagebufalgo_make_texture_from_file(
            mode.code(),
            input,
            output,
            config,
            &mut message,
        )
    };
    if succeeded {
        Ok(())
    } else {
        Err(Error::operation("make texture", message))
    }
}

/// Write an image already in memory as a texture at `output_path`.
///
/// Use [`make_texture`] instead when the source is a file: it streams, and so
/// does not need the whole image resident.
pub fn make_texture_from_buffer(
    mode: TextureMode,
    source: &ImageBuf,
    output_path: &Path,
    config: &TextureConfig,
) -> Result<()> {
    let output = path_to_utf8(output_path)?;
    let config = config.to_sys()?;
    let Some(config) = config.as_ref() else {
        return Err(Error::InvalidImageSpec(
            "OpenImageIO could not allocate an image specification".to_owned(),
        ));
    };

    let mut message = String::new();
    let succeeded = unsafe {
        sys::imagebufalgo::imagebufalgo_make_texture_from_buffer(
            mode.code(),
            source.inner(),
            output,
            config,
            &mut message,
        )
    };
    if succeeded {
        Ok(())
    } else {
        Err(Error::operation("make texture", message))
    }
}

/// How one lookup should be filtered.
#[derive(Debug, Clone, PartialEq)]
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
    /// What a missing or broken texture returns instead of an error.
    ///
    /// Set, a lookup against a file that does not exist or cannot be read
    /// fills the result with these values and succeeds — the mechanism
    /// renderers use so one lost texture does not kill a frame. It needs
    /// one value per requested channel. Unset, missing textures are errors.
    ///
    /// One caveat survives from OpenImageIO: when the *file* is missing the
    /// fill is exact, but when an existing UDIM set's individual tile is
    /// unpopulated, lookups wider than four channels receive the color's
    /// first four values repeated — OpenImageIO fills per four-channel
    /// chunk and never advances the color (drafted as upstream issue 18 in
    /// `contrib/upstream-issues.md`).
    pub missing_color: Option<Vec<f32>>,
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
            missing_color: None,
        }
    }
}

impl TextureOptions {
    fn to_sys(&self) -> Result<sys::texture::TextureLookupOptions> {
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

/// A texture name already resolved against a [`TextureSystem`].
///
/// Looking up through a handle skips the name-table hash every by-name call
/// performs — the difference renderers care about at millions of lookups per
/// frame. A handle borrows the system, so it cannot outlive it, and
/// invalidation (which destroys the state handles point into, and takes
/// `&mut self` on the system) cannot happen while any handle is alive.
pub struct TextureHandle<'system> {
    system: &'system TextureSystem,
    inner: *mut sys::texture::TextureHandle,
}

// SAFETY: a handle is a resolved reference to file state the texture system
// owns, and every operation on it goes through the system, whose lookup
// paths are the thread-safe surface `TextureSystem`'s own Send/Sync argument
// rests on. OpenImageIO's API pairs shared handles with per-thread state it
// manages itself when none is passed, which is what these wrappers do.
unsafe impl Send for TextureHandle<'_> {}
unsafe impl Sync for TextureHandle<'_> {}

impl TextureHandle<'_> {
    /// The texture name this handle resolved to.
    pub fn filename(&self) -> String {
        let inner = self.inner;
        self.system.with_system(|system| {
            // SAFETY: the handle is live for as long as the borrow of the
            // system.
            unsafe { sys::texture::texturesystem_handle_filename(system, inner) }
        })
    }

    /// Whether the texture is still readable — false once the file has
    /// become broken since the handle was made.
    pub fn is_good(&self) -> bool {
        let inner = self.inner;
        self.system.with_system(|system| {
            // SAFETY: as in `filename`.
            unsafe { sys::texture::texturesystem_handle_good(system, inner) }
        })
    }

    fn exists(&self) -> bool {
        let inner = self.inner;
        self.system.with_system(|system| {
            // SAFETY: as in `filename`.
            unsafe { sys::texture::texturesystem_handle_exists(system, inner) }
        })
    }

    /// A filtered lookup through the handle; identical semantics to
    /// [`TextureSystem::texture`], without the per-call name lookup.
    pub fn texture(
        &self,
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
        let missing_color = TextureSystem::validate_missing_color(options, result.len())?;
        let options = options.to_sys()?;
        let inner = self.inner;

        let mut error = String::new();
        let succeeded = self.system.with_system(|system| {
            // SAFETY: the handle is live for as long as the borrow of the
            // system, and the slices are exactly as validated above.
            unsafe {
                sys::texture::texturesystem_texture_by_handle(
                    system,
                    inner,
                    &options,
                    missing_color,
                    s,
                    t,
                    derivatives.dsdx,
                    derivatives.dtdx,
                    derivatives.dsdy,
                    derivatives.dtdy,
                    result,
                    &mut error,
                )
            }
        });
        if succeeded {
            Ok(())
        } else {
            if error.is_empty() {
                error = self
                    .system
                    .with_system(sys::texture::texturesystem_geterror);
            }
            Err(Error::operation("texture lookup", error))
        }
    }

    /// An environment lookup through the handle; identical semantics to
    /// [`TextureSystem::environment`].
    pub fn environment(
        &self,
        options: &TextureOptions,
        direction: [f32; 3],
        d_dx: [f32; 3],
        d_dy: [f32; 3],
        result: &mut [f32],
    ) -> Result<()> {
        if result.is_empty() {
            return Err(Error::InvalidRoi(
                "an environment lookup needs at least one channel".to_owned(),
            ));
        }
        let missing_color = TextureSystem::validate_missing_color(options, result.len())?;
        let options = options.to_sys()?;
        let inner = self.inner;

        let mut error = String::new();
        let succeeded = self.system.with_system(|system| {
            // SAFETY: as in `texture`.
            unsafe {
                sys::texture::texturesystem_environment_by_handle(
                    system,
                    inner,
                    &options,
                    missing_color,
                    direction[0],
                    direction[1],
                    direction[2],
                    d_dx[0],
                    d_dx[1],
                    d_dx[2],
                    d_dy[0],
                    d_dy[1],
                    d_dy[2],
                    result,
                    &mut error,
                )
            }
        });
        if succeeded {
            Ok(())
        } else {
            if error.is_empty() {
                error = self
                    .system
                    .with_system(sys::texture::texturesystem_geterror);
            }
            Err(Error::operation("environment lookup", error))
        }
    }
}

impl std::fmt::Debug for TextureHandle<'_> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("TextureHandle")
            .field("filename", &self.filename())
            .finish_non_exhaustive()
    }
}

/// The concrete files of a UDIM set; see [`TextureSystem::inventory_udim`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UdimInventory {
    /// One entry per grid cell, `None` where the set has no tile — UDIM
    /// sets may be sparse.
    ///
    /// Indexed `u + v * u_tiles`, u varying fastest, which is the layout
    /// OpenImageIO builds. (Its header documents a `v_tiles` stride; the
    /// implementation says otherwise.)
    pub tiles: Vec<Option<PathBuf>>,
    /// Columns of the tile grid.
    pub u_tiles: u32,
    /// Rows of the tile grid.
    pub v_tiles: u32,
}

impl UdimInventory {
    /// The tile at column `u`, row `v`, if populated.
    pub fn tile(&self, u: u32, v: u32) -> Option<&Path> {
        if u >= self.u_tiles || v >= self.v_tiles {
            return None;
        }
        self.tiles
            .get((u + v * self.u_tiles) as usize)
            .and_then(|tile| tile.as_deref())
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

// SAFETY: OpenImageIO does not actually state a thread-safety contract for
// TextureSystem anywhere -- texture.h says nothing, and the theory of operation
// in the manual is still a placeholder. What it does say, in imagecache.rst, is
// that the ImageCache underneath is thread-safe, and the lookup paths are built
// on it: they take the cache's locks, confine their pinned reference to a
// single call, and let no Rust reference to the C++ object escape.
//
// Invalidation is not on that footing and never was. `invalidate_all` calls
// `ImageCacheFile::invalidate`, which clears the file's subimage and dimension
// pools, while `TextureSystemImpl::texture` is holding a `const SubimageInfo&`
// and a `const ImageSpec&` straight into that vector; the two paths do not
// share a lock. Statistics gathering is on the same bad footing: `getstats`
// merges per-thread counters that lookups update with no lock, and its
// embedded cache report walks the same subimage vectors a first open resizes.
// So invalidation, the attribute setters and `stats` take `&mut self`, and
// `Sync` is only ever claiming the lookup paths. A caller who wants both across
// threads writes `Arc<RwLock<TextureSystem>>`, which is exactly the contract.
unsafe impl Send for TextureSystem {}
unsafe impl Sync for TextureSystem {}

impl TextureSystem {
    /// A private texture system, which does not share state with others.
    ///
    /// OpenImageIO's process-wide shared texture system is deliberately not
    /// offered, for the reason [`ImageCache`](crate::ImageCache) does not offer
    /// its shared cache: it is one C++ object behind however many Rust values
    /// ask for it, so `&mut self` on one -- which is what `invalidate` and the
    /// setters take, and the whole reason those are exclusive -- does not
    /// exclude a `&self` lookup on another. Two `shared()` handles across two
    /// threads would reach exactly the free-under-read that the exclusive
    /// receivers were added to prevent. A private system shared through `Arc`
    /// has no such ambiguity, since every borrow is a borrow of the one value.
    pub fn new() -> Result<Self> {
        Self::create()
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
        let missing_color = Self::validate_missing_color(options, result.len())?;
        let filename = path_to_utf8(texture_path)?;
        let options = options.to_sys()?;

        let mut error = String::new();
        let succeeded = self.with_system(|system| {
            sys::texture::texturesystem_texture(
                system,
                filename,
                &options,
                missing_color,
                s,
                t,
                derivatives.dsdx,
                derivatives.dtdx,
                derivatives.dsdy,
                derivatives.dtdy,
                result,
                &mut error,
            )
        });
        if succeeded {
            Ok(())
        } else {
            if error.is_empty() {
                error = self.with_system(sys::texture::texturesystem_geterror);
            }
            Err(Error::operation("texture lookup", error))
        }
    }

    /// Look up an environment map by direction.
    ///
    /// `direction` need not be normalized. The two derivative vectors say how
    /// the direction moves per screen pixel and choose the filter width, like
    /// the plain [`TextureSystem::texture`] derivatives; zeros ask for an
    /// unfiltered sample. One value per channel is written to `result`, and
    /// channels past the file's take the options' fill value — supplied by
    /// this crate, since OpenImageIO zero-fills environment lookups instead
    /// of honouring the fill.
    pub fn environment(
        &self,
        texture_path: &Path,
        options: &TextureOptions,
        direction: [f32; 3],
        d_dx: [f32; 3],
        d_dy: [f32; 3],
        result: &mut [f32],
    ) -> Result<()> {
        if result.is_empty() {
            return Err(Error::InvalidRoi(
                "an environment lookup needs at least one channel".to_owned(),
            ));
        }
        let missing_color = Self::validate_missing_color(options, result.len())?;
        let filename = path_to_utf8(texture_path)?;
        let options = options.to_sys()?;

        let mut error = String::new();
        let succeeded = self.with_system(|system| {
            sys::texture::texturesystem_environment(
                system,
                filename,
                &options,
                missing_color,
                direction[0],
                direction[1],
                direction[2],
                d_dx[0],
                d_dx[1],
                d_dx[2],
                d_dy[0],
                d_dy[1],
                d_dy[2],
                result,
                &mut error,
            )
        });
        if succeeded {
            Ok(())
        } else {
            if error.is_empty() {
                error = self.with_system(sys::texture::texturesystem_geterror);
            }
            Err(Error::operation("environment lookup", error))
        }
    }

    /// Resolve a texture name to a handle for repeated lookups.
    ///
    /// Every by-name lookup pays a name-table hash; a handle pays it once,
    /// which is why renderers resolve their textures at scene load. The
    /// handle borrows this system, so invalidation — which destroys the
    /// state handles point into and takes `&mut self` — cannot happen while
    /// one is alive; the borrow checker refuses it.
    ///
    /// A name whose file cannot be opened is an error here rather than a
    /// handle that fails every lookup. A UDIM pattern is a valid handle:
    /// its lookups resolve the concrete tile per call, exactly as by-name
    /// lookups do. A file that becomes unreadable *after* the handle was
    /// made behaves like any lost texture — lookups error, or fill with
    /// [`TextureOptions::missing_color`] when one is set.
    pub fn handle(&self, texture_path: &Path) -> Result<TextureHandle<'_>> {
        let filename = path_to_utf8(texture_path)?;
        let inner = self.with_system(|system| {
            // SAFETY: the name is a plain string; the returned pointer is
            // owned by the texture system, not the caller.
            unsafe { sys::texture::texturesystem_get_texture_handle(system, filename) }
        });
        if inner.is_null() {
            return Err(Error::OpenImage {
                path: texture_path.to_path_buf(),
                message: "the texture system could not resolve the name".to_owned(),
            });
        }
        let handle = TextureHandle {
            system: self,
            inner,
        };
        // good() alone is only OpenImageIO's broken flag, which a
        // never-opened missing file has not earned yet; the exists probe
        // verifies the file for real (and answers true for UDIM patterns,
        // whose virtual record is never opened).
        if !handle.is_good() || !handle.exists() {
            return Err(Error::OpenImage {
                path: texture_path.to_path_buf(),
                message: "the texture system could not open or read the file".to_owned(),
            });
        }
        Ok(handle)
    }

    /// Whether the name is a UDIM pattern, such as `tex.<UDIM>.exr` or the
    /// `%(UDIM)d`, `<u>`/`<v>`/`<U>`/`<V>` and `_u##v##` spellings.
    pub fn is_udim(&self, texture_path: &Path) -> Result<bool> {
        let filename = path_to_utf8(texture_path)?;
        Ok(self.with_system(|system| sys::texture::texturesystem_is_udim(system, filename)))
    }

    /// The concrete tile file a UDIM pattern refers to at these texture
    /// coordinates, or `None` where no tile exists — UDIM sets may be
    /// sparse. The integer part of `s` selects the column and of `t` the
    /// row, as UDIM numbering does, except that OpenImageIO clamps negative
    /// coordinates to column and row zero — `s = -1.5` answers the 1001
    /// column's tile, not `None`.
    pub fn resolve_udim(&self, pattern: &Path, s: f32, t: f32) -> Result<Option<PathBuf>> {
        let filename = path_to_utf8(pattern)?;
        let resolved = self
            .with_system(|system| sys::texture::texturesystem_resolve_udim(system, filename, s, t));
        Ok((!resolved.is_empty()).then(|| PathBuf::from(resolved)))
    }

    /// Every concrete file of a UDIM set, with `None` for unpopulated tiles.
    pub fn inventory_udim(&self, pattern: &Path) -> Result<UdimInventory> {
        let filename = path_to_utf8(pattern)?;
        let mut filenames = Vec::new();
        let mut u_tiles = 0_i32;
        let mut v_tiles = 0_i32;
        self.with_system(|system| {
            sys::texture::texturesystem_inventory_udim(
                system,
                filename,
                &mut filenames,
                &mut u_tiles,
                &mut v_tiles,
            );
        });
        Ok(UdimInventory {
            tiles: filenames
                .into_iter()
                .map(|name| (!name.is_empty()).then(|| PathBuf::from(name)))
                .collect(),
            u_tiles: u_tiles.max(0) as u32,
            v_tiles: v_tiles.max(0) as u32,
        })
    }

    fn validate_missing_color(options: &TextureOptions, channels: usize) -> Result<&[f32]> {
        match &options.missing_color {
            None => Ok(&[]),
            Some(color) if color.len() == channels => Ok(color),
            Some(color) => Err(Error::InvalidRoi(format!(
                "the missing color holds {} values for a {channels}-channel lookup",
                color.len()
            ))),
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
    pub fn set_max_memory_mb(&mut self, megabytes: f32) -> Result<()> {
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
    pub fn set_max_open_files(&mut self, count: u32) -> Result<()> {
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
    /// Exclusive because invalidation frees state a concurrent lookup is
    /// holding references into; see the `Sync` justification on this type.
    pub fn invalidate(&mut self, texture_path: &Path, force: bool) -> Result<()> {
        let filename = path_to_utf8(texture_path)?;
        self.with_system(|system| {
            sys::texture::texturesystem_invalidate(system, filename, force);
        });
        Ok(())
    }

    /// Forget every texture.
    /// Exclusive for the reasons given on [`TextureSystem::invalidate`].
    pub fn invalidate_all(&mut self, force: bool) {
        self.with_system(|system| {
            sys::texture::texturesystem_invalidate_all(system, force);
        });
    }

    /// Statistics suitable for diagnostics.
    ///
    /// Exclusive for the reason invalidation is: OpenImageIO gathers these
    /// with no lock. The merge reads per-thread counters that concurrent
    /// lookups update, and the embedded cache report walks every file's
    /// subimage vector while a first open on another thread may be resizing
    /// it — the same free-under-read invalidation can cause, reached by a
    /// read-only path.
    pub fn stats(&mut self) -> String {
        let inner = self.inner.clone();
        let Some(system) = inner.as_ref() else {
            return String::new();
        };
        sys::texture::texturesystem_getstats(system, 1)
    }

    fn create() -> Result<Self> {
        // Always private; see the note on `new`.
        let inner = sys::texture::texturesystem_create(false);
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
