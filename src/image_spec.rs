use crate::{sys, Error, Result, Roi};

/// A value-owned subset of OpenImageIO's image specification.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImageSpec {
    x: i32,
    y: i32,
    z: i32,
    width: u32,
    height: u32,
    depth: u32,
    full_x: i32,
    full_y: i32,
    full_z: i32,
    full_width: u32,
    full_height: u32,
    full_depth: u32,
    tile_width: u32,
    tile_height: u32,
    tile_depth: u32,
    channels: u32,
    channel_names: Vec<String>,
    alpha_channel: Option<u32>,
    z_channel: Option<u32>,
    deep: bool,
}

impl ImageSpec {
    pub(crate) fn from_sys(spec: &sys::imageio::ImageSpec) -> Result<Self> {
        let channels = positive("channel count", sys::imageio::imagespec_nchannels(spec))?;
        let channel_names = sys::imageio::imagespec_channel_names(spec)
            .as_ref()
            .map(|names| {
                names
                    .iter()
                    .map(|name| name.to_string_lossy().into_owned())
                    .collect()
            })
            .unwrap_or_default();

        Ok(Self {
            x: sys::imageio::imagespec_x(spec),
            y: sys::imageio::imagespec_y(spec),
            z: sys::imageio::imagespec_z(spec),
            width: positive("width", sys::imageio::imagespec_width(spec))?,
            height: positive("height", sys::imageio::imagespec_height(spec))?,
            depth: positive("depth", sys::imageio::imagespec_depth(spec))?,
            full_x: sys::imageio::imagespec_full_x(spec),
            full_y: sys::imageio::imagespec_full_y(spec),
            full_z: sys::imageio::imagespec_full_z(spec),
            full_width: nonnegative("full width", sys::imageio::imagespec_full_width(spec))?,
            full_height: nonnegative("full height", sys::imageio::imagespec_full_height(spec))?,
            full_depth: nonnegative("full depth", sys::imageio::imagespec_full_depth(spec))?,
            tile_width: nonnegative("tile width", sys::imageio::imagespec_tile_width(spec))?,
            tile_height: nonnegative("tile height", sys::imageio::imagespec_tile_height(spec))?,
            tile_depth: nonnegative("tile depth", sys::imageio::imagespec_tile_depth(spec))?,
            channels,
            channel_names,
            alpha_channel: channel_index(sys::imageio::imagespec_alpha_channel(spec), channels),
            z_channel: channel_index(sys::imageio::imagespec_z_channel(spec), channels),
            deep: sys::imageio::imagespec_deep(spec),
        })
    }

    /// Number of scalar channel values in the data window.
    pub fn element_count(&self) -> Result<usize> {
        [self.width, self.height, self.depth, self.channels]
            .into_iter()
            .try_fold(1usize, |total, dimension| {
                total
                    .checked_mul(dimension as usize)
                    .ok_or(Error::BufferSizeOverflow)
            })
    }

    /// The image's data window, including all channels.
    pub fn data_window(&self) -> Result<Roi> {
        Roi::from_spec(self)
    }

    /// Origin of the pixel data window as `[x, y, z]`.
    pub fn origin(&self) -> [i32; 3] {
        [self.x, self.y, self.z]
    }

    /// Size of the pixel data window as `[width, height, depth]`.
    pub fn dimensions(&self) -> [u32; 3] {
        [self.width, self.height, self.depth]
    }

    /// Origin of the full (display) window as `[x, y, z]`.
    pub fn full_origin(&self) -> [i32; 3] {
        [self.full_x, self.full_y, self.full_z]
    }

    /// Size of the full (display) window as `[width, height, depth]`.
    pub fn full_dimensions(&self) -> [u32; 3] {
        [self.full_width, self.full_height, self.full_depth]
    }

    /// Tile size as `[width, height, depth]`; zero width means scanline data.
    pub fn tile_dimensions(&self) -> [u32; 3] {
        [self.tile_width, self.tile_height, self.tile_depth]
    }

    /// Number of channels per pixel.
    pub fn channel_count(&self) -> u32 {
        self.channels
    }

    /// Channel names in file order.
    pub fn channel_names(&self) -> &[String] {
        &self.channel_names
    }

    /// Index of the alpha channel when one is designated.
    pub fn alpha_channel(&self) -> Option<u32> {
        self.alpha_channel
    }

    /// Index of the depth channel when one is designated.
    pub fn z_channel(&self) -> Option<u32> {
        self.z_channel
    }

    /// Whether this describes a deep image with per-pixel sample counts.
    pub fn is_deep(&self) -> bool {
        self.deep
    }

    pub(crate) fn x(&self) -> i32 {
        self.x
    }

    pub(crate) fn y(&self) -> i32 {
        self.y
    }

    pub(crate) fn z(&self) -> i32 {
        self.z
    }

    pub(crate) fn width(&self) -> u32 {
        self.width
    }

    pub(crate) fn height(&self) -> u32 {
        self.height
    }

    pub(crate) fn depth(&self) -> u32 {
        self.depth
    }

    pub(crate) fn channels(&self) -> u32 {
        self.channels
    }
}

fn positive(name: &str, value: i32) -> Result<u32> {
    if value <= 0 {
        return Err(Error::InvalidImageSpec(format!(
            "{name} must be positive, got {value}"
        )));
    }
    Ok(value as u32)
}

fn nonnegative(name: &str, value: i32) -> Result<u32> {
    if value < 0 {
        return Err(Error::InvalidImageSpec(format!(
            "{name} must be non-negative, got {value}"
        )));
    }
    Ok(value as u32)
}

fn channel_index(value: i32, channels: u32) -> Option<u32> {
    let value = u32::try_from(value).ok()?;
    (value < channels).then_some(value)
}
