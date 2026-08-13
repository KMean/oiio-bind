use crate::{sys, AttributeValue, Error, PixelFormat, Result, Roi};

/// A value-owned description of an image.
///
/// An `ImageSpec` is both what a reader reports about a file and what a writer
/// is opened with. It owns its data, so it stays valid after the file it came
/// from is closed.
#[derive(Debug, Clone, PartialEq)]
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
    format: PixelFormat,
    attributes: Vec<(String, AttributeValue)>,
}

impl ImageSpec {
    /// Describe a two-dimensional image.
    ///
    /// Channel names, the display window, and the alpha channel are seeded
    /// with OpenImageIO's own defaults for the given channel count.
    pub fn new(width: u32, height: u32, channels: u32, format: PixelFormat) -> Result<Self> {
        let width_i32 = dimension("width", width)?;
        let height_i32 = dimension("height", height)?;
        let channels_i32 = dimension("channel count", channels)?;
        if width == 0 || height == 0 || channels == 0 {
            return Err(Error::InvalidImageSpec(
                "width, height, and channel count must be non-zero".to_owned(),
            ));
        }

        let spec =
            sys::imageio::imagespec_new(width_i32, height_i32, channels_i32, format.to_sys());
        let spec = spec.as_ref().ok_or_else(|| {
            Error::InvalidImageSpec(
                "OpenImageIO could not allocate an image specification".to_owned(),
            )
        })?;
        Self::from_sys(spec)
    }

    /// Set the pixel data type.
    pub fn with_format(mut self, format: PixelFormat) -> Self {
        self.format = format;
        self
    }

    /// Set the origin of the pixel data window.
    pub fn with_origin(mut self, origin: [i32; 3]) -> Self {
        [self.x, self.y, self.z] = origin;
        self
    }

    /// Set the depth of a volumetric image.
    pub fn with_depth(mut self, depth: u32) -> Result<Self> {
        if depth == 0 {
            return Err(Error::InvalidImageSpec("depth must be non-zero".to_owned()));
        }
        dimension("depth", depth)?;
        self.depth = depth;
        Ok(self)
    }

    /// Set the full (display) window.
    pub fn with_full_window(mut self, origin: [i32; 3], dimensions: [u32; 3]) -> Result<Self> {
        for (name, value) in ["full width", "full height", "full depth"]
            .into_iter()
            .zip(dimensions)
        {
            dimension(name, value)?;
        }
        [self.full_x, self.full_y, self.full_z] = origin;
        [self.full_width, self.full_height, self.full_depth] = dimensions;
        Ok(self)
    }

    /// Request tiled storage of the given size.
    ///
    /// A width of zero requests scanline storage. Whether a file format
    /// honours tiles is reported by
    /// [`ImageOutput::supports`](crate::ImageOutput::supports).
    pub fn with_tile_size(mut self, dimensions: [u32; 3]) -> Result<Self> {
        for (name, value) in ["tile width", "tile height", "tile depth"]
            .into_iter()
            .zip(dimensions)
        {
            dimension(name, value)?;
        }
        [self.tile_width, self.tile_height, self.tile_depth] = dimensions;
        Ok(self)
    }

    /// Replace the channel names, which must match the channel count.
    pub fn with_channel_names<I>(mut self, names: I) -> Result<Self>
    where
        I: IntoIterator,
        I::Item: Into<String>,
    {
        let names: Vec<String> = names.into_iter().map(Into::into).collect();
        if names.len() != self.channels as usize {
            return Err(Error::InvalidImageSpec(format!(
                "expected {} channel names, got {}",
                self.channels,
                names.len()
            )));
        }
        self.channel_names = names;
        Ok(self)
    }

    /// Designate which channel holds alpha, if any.
    pub fn with_alpha_channel(mut self, index: Option<u32>) -> Result<Self> {
        self.alpha_channel = self.validate_channel_index("alpha channel", index)?;
        Ok(self)
    }

    /// Designate which channel holds depth, if any.
    pub fn with_z_channel(mut self, index: Option<u32>) -> Result<Self> {
        self.z_channel = self.validate_channel_index("depth channel", index)?;
        Ok(self)
    }

    /// Mark this specification as describing a deep image.
    ///
    /// Deep pixels hold a list of samples rather than one value, so a deep
    /// specification is written with
    /// [`ImageOutput::write_deep_image`](crate::ImageOutput::write_deep_image)
    /// and read with
    /// [`ImageInput::read_deep_image`](crate::ImageInput::read_deep_image);
    /// the contiguous pixel calls refuse it.
    pub fn as_deep(mut self) -> Self {
        self.deep = true;
        self
    }

    /// Attach a metadata attribute, replacing any attribute of the same name.
    pub fn with_attribute(
        mut self,
        name: impl Into<String>,
        value: impl Into<AttributeValue>,
    ) -> Self {
        self.set_attribute(name, value);
        self
    }

    /// Attach a metadata attribute, replacing any attribute of the same name.
    pub fn set_attribute(&mut self, name: impl Into<String>, value: impl Into<AttributeValue>) {
        let name = name.into();
        let value = value.into();
        match self.attributes.iter_mut().find(|(key, _)| *key == name) {
            Some(entry) => entry.1 = value,
            None => self.attributes.push((name, value)),
        }
    }

    /// Remove a metadata attribute, returning it when it was present.
    pub fn remove_attribute(&mut self, name: &str) -> Option<AttributeValue> {
        let index = self.attributes.iter().position(|(key, _)| key == name)?;
        Some(self.attributes.remove(index).1)
    }

    /// Look up a metadata attribute by name.
    pub fn attribute(&self, name: &str) -> Option<&AttributeValue> {
        self.attributes
            .iter()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value)
    }

    /// All metadata attributes, in the order the file or caller supplied them.
    pub fn attributes(&self) -> &[(String, AttributeValue)] {
        &self.attributes
    }

    /// The pixel data type.
    pub fn format(&self) -> PixelFormat {
        self.format
    }

    /// Number of scalar channel values in the data window.
    pub fn element_count(&self) -> Result<usize> {
        element_count([self.width, self.height, self.depth, self.channels])
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

    /// Whether the image is stored as tiles rather than scanlines.
    pub fn is_tiled(&self) -> bool {
        self.tile_width > 0
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
        let attributes = sys::imageio::imagespec_attribute_names(spec)
            .into_iter()
            .map(|name| {
                let value = AttributeValue::read(spec, &name);
                (name, value)
            })
            .collect();

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
            format: PixelFormat::from_sys(&sys::imageio::imagespec_format(spec)),
            attributes,
        })
    }

    /// Build the OpenImageIO specification this describes.
    ///
    /// Attributes that are not [writable](AttributeValue::is_writable) are
    /// omitted.
    pub(crate) fn to_sys(&self) -> Result<cxx::UniquePtr<sys::imageio::ImageSpec>> {
        if self.format == PixelFormat::Other {
            return Err(Error::InvalidImageSpec(
                "a writable specification needs a concrete pixel format".to_owned(),
            ));
        }
        if self.channel_names.len() != self.channels as usize {
            return Err(Error::InvalidImageSpec(format!(
                "expected {} channel names, got {}",
                self.channels,
                self.channel_names.len()
            )));
        }

        let mut spec = sys::imageio::imagespec_new(
            dimension("width", self.width)?,
            dimension("height", self.height)?,
            dimension("channel count", self.channels)?,
            self.format.to_sys(),
        );
        let Some(mut pinned) = spec.as_mut() else {
            return Err(Error::InvalidImageSpec(
                "OpenImageIO could not allocate an image specification".to_owned(),
            ));
        };

        sys::imageio::imagespec_set_origin(pinned.as_mut(), self.x, self.y, self.z);
        sys::imageio::imagespec_set_dimensions(
            pinned.as_mut(),
            dimension("width", self.width)?,
            dimension("height", self.height)?,
            dimension("depth", self.depth)?,
        );
        sys::imageio::imagespec_set_full(
            pinned.as_mut(),
            self.full_x,
            self.full_y,
            self.full_z,
            dimension("full width", self.full_width)?,
            dimension("full height", self.full_height)?,
            dimension("full depth", self.full_depth)?,
        );
        sys::imageio::imagespec_set_tile_size(
            pinned.as_mut(),
            dimension("tile width", self.tile_width)?,
            dimension("tile height", self.tile_height)?,
            dimension("tile depth", self.tile_depth)?,
        );
        sys::imageio::imagespec_set_channel_names(pinned.as_mut(), &self.channel_names);
        sys::imageio::imagespec_set_alpha_channel(
            pinned.as_mut(),
            optional_channel_index(self.alpha_channel)?,
        );
        sys::imageio::imagespec_set_z_channel(
            pinned.as_mut(),
            optional_channel_index(self.z_channel)?,
        );
        sys::imageio::imagespec_set_deep(pinned.as_mut(), self.deep);

        for (name, value) in &self.attributes {
            value.write(pinned.as_mut(), name);
        }

        Ok(spec)
    }

    fn validate_channel_index(&self, name: &str, index: Option<u32>) -> Result<Option<u32>> {
        match index {
            Some(index) if index >= self.channels => Err(Error::InvalidImageSpec(format!(
                "{name} index {index} is outside the image's {} channels",
                self.channels
            ))),
            other => Ok(other),
        }
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

pub(crate) fn element_count<const N: usize>(dimensions: [u32; N]) -> Result<usize> {
    dimensions.into_iter().try_fold(1usize, |total, dimension| {
        total
            .checked_mul(dimension as usize)
            .ok_or(Error::BufferSizeOverflow)
    })
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

fn dimension(name: &str, value: u32) -> Result<i32> {
    i32::try_from(value)
        .map_err(|_| Error::InvalidImageSpec(format!("{name} {value} exceeds i32::MAX")))
}

fn optional_channel_index(index: Option<u32>) -> Result<i32> {
    match index {
        None => Ok(-1),
        Some(index) => i32::try_from(index)
            .map_err(|_| Error::InvalidImageSpec("channel index exceeds i32::MAX".to_owned())),
    }
}

fn channel_index(value: i32, channels: u32) -> Option<u32> {
    let value = u32::try_from(value).ok()?;
    (value < channels).then_some(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seeds_openimageio_defaults() {
        let spec = ImageSpec::new(64, 32, 4, PixelFormat::F16).unwrap();
        assert_eq!(spec.dimensions(), [64, 32, 1]);
        assert_eq!(spec.full_dimensions(), [64, 32, 1]);
        assert_eq!(spec.format(), PixelFormat::F16);
        assert_eq!(spec.channel_names(), ["R", "G", "B", "A"]);
        assert_eq!(spec.alpha_channel(), Some(3));
        assert!(!spec.is_tiled());
        assert!(spec.attributes().is_empty());
    }

    #[test]
    fn rejects_degenerate_dimensions() {
        assert!(ImageSpec::new(0, 32, 3, PixelFormat::U8).is_err());
        assert!(ImageSpec::new(64, 0, 3, PixelFormat::U8).is_err());
        assert!(ImageSpec::new(64, 32, 0, PixelFormat::U8).is_err());
    }

    #[test]
    fn round_trips_through_openimageio() {
        let spec = ImageSpec::new(8, 4, 2, PixelFormat::F32)
            .unwrap()
            .with_origin([2, -3, 0])
            .with_full_window([0, 0, 0], [16, 16, 1])
            .unwrap()
            .with_tile_size([8, 8, 1])
            .unwrap()
            .with_channel_names(["Y", "A"])
            .unwrap()
            .with_alpha_channel(Some(1))
            .unwrap()
            .with_attribute("Software", "oiio-bind")
            .with_attribute("Orientation", 1)
            .with_attribute("PixelAspectRatio", 2.0_f32);

        let native = spec.to_sys().unwrap();
        let restored = ImageSpec::from_sys(native.as_ref().unwrap()).unwrap();

        assert_eq!(restored, spec);
    }

    #[test]
    fn rejects_channel_names_that_do_not_match_the_channel_count() {
        let spec = ImageSpec::new(4, 4, 3, PixelFormat::U8).unwrap();
        assert!(spec.clone().with_channel_names(["R", "G"]).is_err());
        assert!(spec.with_channel_names(["R", "G", "B"]).is_ok());
    }

    #[test]
    fn rejects_out_of_range_channel_designations() {
        let spec = ImageSpec::new(4, 4, 3, PixelFormat::U8).unwrap();
        assert!(spec.clone().with_alpha_channel(Some(3)).is_err());
        assert!(spec.clone().with_z_channel(Some(9)).is_err());
        assert!(spec.with_alpha_channel(None).is_ok());
    }

    #[test]
    fn replaces_and_removes_attributes() {
        let mut spec = ImageSpec::new(4, 4, 1, PixelFormat::U8)
            .unwrap()
            .with_attribute("Artist", "first")
            .with_attribute("Artist", "second");
        assert_eq!(spec.attributes().len(), 1);
        assert_eq!(
            spec.attribute("Artist").and_then(AttributeValue::as_str),
            Some("second")
        );

        let removed = spec.remove_attribute("Artist");
        assert_eq!(removed, Some(AttributeValue::String("second".to_owned())));
        assert!(spec.attribute("Artist").is_none());
        assert!(spec.remove_attribute("Artist").is_none());
    }

    #[test]
    fn refuses_to_build_a_specification_without_a_pixel_format() {
        let spec = ImageSpec::new(4, 4, 1, PixelFormat::U8)
            .unwrap()
            .with_format(PixelFormat::Other);
        assert!(matches!(spec.to_sys(), Err(Error::InvalidImageSpec(_))));
    }
}
