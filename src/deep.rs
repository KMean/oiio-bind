use crate::{sys, Error, ImageSpec, PixelFormat, Result};

/// One channel of a [`DeepImage`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeepChannel {
    name: String,
    format: PixelFormat,
}

impl DeepChannel {
    /// The channel's name, such as `"R"`, `"A"` or `"Z"`.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The type its samples are stored as.
    pub fn format(&self) -> PixelFormat {
        self.format
    }
}

/// A deep image: every pixel holds a list of samples rather than one value.
///
/// Deep images are how a renderer records what is behind what — each sample
/// carries its own depth alongside its colour, so a pixel covered by three
/// overlapping objects keeps all three. The contiguous
/// [`Pixel`](crate::Pixel) API cannot express that and refuses deep files;
/// this is how they are read instead.
///
/// Samples are addressed by pixel coordinate, then channel, then sample
/// index. Every accessor is bounds-checked, so an out-of-range coordinate is
/// an error rather than a read of whatever was next in memory.
///
/// ```no_run
/// use oiio::ImageInput;
/// use std::path::Path;
///
/// # fn main() -> oiio::Result<()> {
/// let mut input = ImageInput::from_path(Path::new("deep.exr"))?;
/// let deep = input.read_deep_image()?;
///
/// let depth = deep.z_channel().expect("a deep image usually has Z");
/// for (x, y) in [(0, 0), (1, 0)] {
///     for sample in 0..deep.sample_count(x, y)? {
///         println!("({x}, {y}) sample {sample} at z {}", deep.value(x, y, depth, sample)?);
///     }
/// }
/// # Ok(())
/// # }
/// ```
pub struct DeepImage {
    inner: cxx::UniquePtr<sys::deepdata::DeepData>,
    origin: [i32; 3],
    dimensions: [u32; 3],
    channels: Vec<DeepChannel>,
}

impl DeepImage {
    /// Build an empty deep image matching a specification.
    ///
    /// The specification must be marked deep, and describes the size and the
    /// channels. Every pixel starts with no samples; give them some with
    /// [`DeepImage::set_sample_count`], then fill them in with
    /// [`DeepImage::set_value`].
    ///
    /// ```no_run
    /// use oiio::{DeepImage, ImageOutput, ImageSpec, PixelFormat};
    /// use std::path::Path;
    ///
    /// # fn main() -> oiio::Result<()> {
    /// let spec = ImageSpec::new(64, 64, 5, PixelFormat::F32)?
    ///     .with_channel_names(["R", "G", "B", "A", "Z"])?
    ///     .as_deep();
    /// let mut deep = DeepImage::new(&spec)?;
    ///
    /// // One sample at the origin, at depth 10.
    /// deep.set_sample_count(0, 0, 1)?;
    /// deep.set_value(0, 0, 4, 0, 10.0)?;
    ///
    /// let mut output = ImageOutput::create(Path::new("deep.exr"), &spec)?;
    /// output.write_deep_image(&deep)?;
    /// output.close()?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn new(spec: &ImageSpec) -> Result<Self> {
        if !spec.is_deep() {
            return Err(Error::InvalidImageSpec(
                "a deep image needs a specification marked deep; see ImageSpec::as_deep".to_owned(),
            ));
        }

        let native_spec = spec.to_sys()?;
        let Some(native_spec) = native_spec.as_ref() else {
            return Err(Error::InvalidImageSpec(
                "OpenImageIO could not allocate an image specification".to_owned(),
            ));
        };

        let mut inner = sys::deepdata::deepdata_default();
        let Some(pinned) = inner.as_mut() else {
            return Err(Error::operation(
                "create deep image",
                "OpenImageIO could not allocate deep data".to_owned(),
            ));
        };
        sys::deepdata::deepdata_init_from_spec(pinned, native_spec);

        Self::from_parts(inner, spec)
    }

    /// Give a pixel a number of samples, discarding any it already had.
    ///
    /// Samples start zeroed; set their values with [`DeepImage::set_value`].
    pub fn set_sample_count(&mut self, x: i32, y: i32, count: usize) -> Result<()> {
        let pixel = self.pixel_index(x, y)?;
        let count = i32::try_from(count)
            .map_err(|_| Error::InvalidRoi("sample count exceeds i32::MAX".to_owned()))?;
        sys::deepdata::deepdata_set_samples(self.inner_mut(), pixel, count);
        Ok(())
    }

    /// Set one sample's value.
    pub fn set_value(
        &mut self,
        x: i32,
        y: i32,
        channel: usize,
        sample: usize,
        value: f32,
    ) -> Result<()> {
        let (pixel, channel, sample) = self.address(x, y, channel, sample)?;
        sys::deepdata::deepdata_set_deep_value(self.inner_mut(), pixel, channel, sample, value);
        Ok(())
    }

    /// Set one sample's value in a channel stored as an unsigned integer.
    pub fn set_value_uint(
        &mut self,
        x: i32,
        y: i32,
        channel: usize,
        sample: usize,
        value: u32,
    ) -> Result<()> {
        let (pixel, channel, sample) = self.address(x, y, channel, sample)?;
        sys::deepdata::deepdata_set_deep_value_uint(
            self.inner_mut(),
            pixel,
            channel,
            sample,
            value,
        );
        Ok(())
    }

    pub(crate) fn native(&self) -> &sys::deepdata::DeepData {
        self.inner()
    }

    fn inner_mut(&mut self) -> std::pin::Pin<&mut sys::deepdata::DeepData> {
        self.inner
            .as_mut()
            .expect("DeepImage invariant violated: null native pointer")
    }

    pub(crate) fn from_parts(
        inner: cxx::UniquePtr<sys::deepdata::DeepData>,
        spec: &ImageSpec,
    ) -> Result<Self> {
        if inner.is_null() {
            return Err(Error::operation(
                "read deep image",
                "OpenImageIO returned no deep data".to_owned(),
            ));
        }

        let channel_count = sys::deepdata::deepdata_channels(inner.as_ref().expect("non-null"));
        let mut channels = Vec::with_capacity(channel_count.max(0) as usize);
        for index in 0..channel_count {
            let native = inner.as_ref().expect("non-null");
            channels.push(DeepChannel {
                name: sys::deepdata::deepdata_channelname(native, index).to_owned(),
                format: PixelFormat::from_sys(&sys::deepdata::deepdata_channeltype(native, index)),
            });
        }

        Ok(Self {
            inner,
            origin: spec.origin(),
            dimensions: spec.dimensions(),
            channels,
        })
    }

    /// Size of the image as `[width, height, depth]`.
    pub fn dimensions(&self) -> [u32; 3] {
        self.dimensions
    }

    /// Origin of the data window as `[x, y, z]`.
    pub fn origin(&self) -> [i32; 3] {
        self.origin
    }

    /// The channels every sample carries.
    pub fn channels(&self) -> &[DeepChannel] {
        &self.channels
    }

    /// Number of channels per sample.
    pub fn channel_count(&self) -> usize {
        self.channels.len()
    }

    /// Total number of pixels, each of which holds its own sample list.
    pub fn pixel_count(&self) -> u64 {
        sys::deepdata::deepdata_pixels(self.inner()).max(0) as u64
    }

    /// Index of the depth channel, conventionally `Z`.
    pub fn z_channel(&self) -> Option<usize> {
        self.channel_index(sys::deepdata::deepdata_z_channel(self.inner()))
    }

    /// Index of the far-depth channel, conventionally `Zback`.
    pub fn z_back_channel(&self) -> Option<usize> {
        self.channel_index(sys::deepdata::deepdata_z_back_channel(self.inner()))
    }

    /// Index of the alpha channel.
    pub fn alpha_channel(&self) -> Option<usize> {
        self.channel_index(sys::deepdata::deepdata_a_channel(self.inner()))
    }

    /// How many samples the pixel at `(x, y)` holds, which may be zero.
    pub fn sample_count(&self, x: i32, y: i32) -> Result<usize> {
        let pixel = self.pixel_index(x, y)?;
        Ok(sys::deepdata::deepdata_samples(self.inner(), pixel).max(0) as usize)
    }

    /// One sample's value, converted to `f32`.
    ///
    /// Use [`DeepImage::value_uint`] for channels whose format is an
    /// unsigned integer, where converting would lose the top bits.
    pub fn value(&self, x: i32, y: i32, channel: usize, sample: usize) -> Result<f32> {
        let (pixel, channel, sample) = self.address(x, y, channel, sample)?;
        Ok(sys::deepdata::deepdata_deep_value(
            self.inner(),
            pixel,
            channel,
            sample,
        ))
    }

    /// One sample's value as an unsigned integer.
    pub fn value_uint(&self, x: i32, y: i32, channel: usize, sample: usize) -> Result<u32> {
        let (pixel, channel, sample) = self.address(x, y, channel, sample)?;
        Ok(sys::deepdata::deepdata_deep_value_uint(
            self.inner(),
            pixel,
            channel,
            sample,
        ))
    }

    /// Every sample of one channel at `(x, y)`, in depth order as stored.
    pub fn samples(&self, x: i32, y: i32, channel: usize) -> Result<Vec<f32>> {
        let count = self.sample_count(x, y)?;
        (0..count)
            .map(|sample| self.value(x, y, channel, sample))
            .collect()
    }

    fn channel_index(&self, index: i32) -> Option<usize> {
        let index = usize::try_from(index).ok()?;
        (index < self.channels.len()).then_some(index)
    }

    /// Map an image coordinate onto the linear pixel index OpenImageIO uses.
    fn pixel_index(&self, x: i32, y: i32) -> Result<i64> {
        for (axis, coordinate, start, size) in [
            ("x", x, self.origin[0], self.dimensions[0]),
            ("y", y, self.origin[1], self.dimensions[1]),
        ] {
            let end = i64::from(start) + i64::from(size);
            if i64::from(coordinate) < i64::from(start) || i64::from(coordinate) >= end {
                return Err(Error::InvalidRegion {
                    axis,
                    message: format!(
                        "coordinate {coordinate} lies outside the data window {start}..{end}"
                    ),
                });
            }
        }

        let column = i64::from(x) - i64::from(self.origin[0]);
        let row = i64::from(y) - i64::from(self.origin[1]);
        Ok(row * i64::from(self.dimensions[0]) + column)
    }

    fn address(&self, x: i32, y: i32, channel: usize, sample: usize) -> Result<(i64, i32, i32)> {
        let pixel = self.pixel_index(x, y)?;
        if channel >= self.channels.len() {
            return Err(Error::InvalidRoi(format!(
                "channel {channel} is outside the image's {} channels",
                self.channels.len()
            )));
        }
        let count = sys::deepdata::deepdata_samples(self.inner(), pixel).max(0) as usize;
        if sample >= count {
            return Err(Error::InvalidRoi(format!(
                "sample {sample} is outside the {count} samples at ({x}, {y})"
            )));
        }

        let channel = i32::try_from(channel)
            .map_err(|_| Error::InvalidRoi("channel index exceeds i32::MAX".to_owned()))?;
        let sample = i32::try_from(sample)
            .map_err(|_| Error::InvalidRoi("sample index exceeds i32::MAX".to_owned()))?;
        Ok((pixel, channel, sample))
    }

    fn inner(&self) -> &sys::deepdata::DeepData {
        self.inner
            .as_ref()
            .expect("DeepImage invariant violated: null native pointer")
    }
}

impl std::fmt::Debug for DeepImage {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("DeepImage")
            .field("dimensions", &self.dimensions)
            .field("channels", &self.channels.len())
            .field("pixels", &self.pixel_count())
            .finish_non_exhaustive()
    }
}
