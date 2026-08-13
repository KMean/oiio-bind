use std::ops::Range;

use crate::{sys, Error, ImageSpec, Result};

/// A half-open pixel and channel region.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Roi {
    x_begin: i32,
    x_end: i32,
    y_begin: i32,
    y_end: i32,
    z_begin: i32,
    z_end: i32,
    channel_begin: i32,
    channel_end: i32,
}

impl Roi {
    /// Construct a non-empty, half-open region.
    pub fn new(x: Range<i32>, y: Range<i32>, z: Range<i32>, channels: Range<u32>) -> Result<Self> {
        validate_range("x", &x)?;
        validate_range("y", &y)?;
        validate_range("z", &z)?;
        let (channel_begin, channel_end) = channel_bounds(channels)?;

        Ok(Self {
            x_begin: x.start,
            x_end: x.end,
            y_begin: y.start,
            y_end: y.end,
            z_begin: z.start,
            z_end: z.end,
            channel_begin,
            channel_end,
        })
    }

    /// Narrow or move the x range, keeping every other axis.
    pub fn with_x(mut self, x: Range<i32>) -> Result<Self> {
        validate_range("x", &x)?;
        self.x_begin = x.start;
        self.x_end = x.end;
        Ok(self)
    }

    /// Narrow or move the y range, keeping every other axis.
    pub fn with_y(mut self, y: Range<i32>) -> Result<Self> {
        validate_range("y", &y)?;
        self.y_begin = y.start;
        self.y_end = y.end;
        Ok(self)
    }

    /// Narrow or move the z range, keeping every other axis.
    pub fn with_z(mut self, z: Range<i32>) -> Result<Self> {
        validate_range("z", &z)?;
        self.z_begin = z.start;
        self.z_end = z.end;
        Ok(self)
    }

    /// Narrow the channel range, keeping every axis.
    pub fn with_channels(mut self, channels: Range<u32>) -> Result<Self> {
        let (channel_begin, channel_end) = channel_bounds(channels)?;
        self.channel_begin = channel_begin;
        self.channel_end = channel_end;
        Ok(self)
    }

    pub fn x(&self) -> Range<i32> {
        self.x_begin..self.x_end
    }

    pub fn y(&self) -> Range<i32> {
        self.y_begin..self.y_end
    }

    pub fn z(&self) -> Range<i32> {
        self.z_begin..self.z_end
    }

    pub fn channels(&self) -> Range<u32> {
        self.channel_begin as u32..self.channel_end as u32
    }

    pub fn width(&self) -> usize {
        difference(self.x_begin, self.x_end)
    }

    pub fn height(&self) -> usize {
        difference(self.y_begin, self.y_end)
    }

    pub fn depth(&self) -> usize {
        difference(self.z_begin, self.z_end)
    }

    pub fn channel_count(&self) -> usize {
        difference(self.channel_begin, self.channel_end)
    }

    /// Number of scalar channel values described by this region.
    pub fn element_count(&self) -> Result<usize> {
        [
            self.width(),
            self.height(),
            self.depth(),
            self.channel_count(),
        ]
        .into_iter()
        .try_fold(1usize, |total, dimension| {
            total
                .checked_mul(dimension)
                .ok_or(Error::BufferSizeOverflow)
        })
    }

    pub(crate) fn from_spec(spec: &ImageSpec) -> Result<Self> {
        let x_end = checked_end("x", spec.x(), spec.width())?;
        let y_end = checked_end("y", spec.y(), spec.height())?;
        let z_end = checked_end("z", spec.z(), spec.depth())?;
        Self::new(
            spec.x()..x_end,
            spec.y()..y_end,
            spec.z()..z_end,
            0..spec.channels(),
        )
    }

    /// Check that this region lies inside an image's data window and channels.
    pub(crate) fn validate_within(&self, spec: &ImageSpec) -> Result<()> {
        let origin = spec.origin();
        let dimensions = spec.dimensions();
        for (axis, begin, end, start, size) in [
            ("x", self.x_begin, self.x_end, origin[0], dimensions[0]),
            ("y", self.y_begin, self.y_end, origin[1], dimensions[1]),
            ("z", self.z_begin, self.z_end, origin[2], dimensions[2]),
        ] {
            let window_end = i64::from(start) + i64::from(size);
            if i64::from(begin) < i64::from(start) || i64::from(end) > window_end {
                return Err(Error::InvalidRegion {
                    axis,
                    message: format!(
                        "range {begin}..{end} lies outside the data window {start}..{window_end}"
                    ),
                });
            }
        }
        self.validate_channels(spec)
    }

    pub(crate) fn validate_channels(&self, spec: &ImageSpec) -> Result<()> {
        let channel_end = i32::try_from(spec.channels()).map_err(|_| {
            Error::InvalidImageSpec("channel count does not fit in an i32".to_owned())
        })?;
        if self.channel_end > channel_end {
            return Err(Error::InvalidRoi(
                "channel range extends outside the image's channels".to_owned(),
            ));
        }
        Ok(())
    }

    pub(crate) fn to_sys(self) -> sys::imageio::ROI {
        sys::imageio::ROI {
            xbegin: self.x_begin,
            xend: self.x_end,
            ybegin: self.y_begin,
            yend: self.y_end,
            zbegin: self.z_begin,
            zend: self.z_end,
            chbegin: self.channel_begin,
            chend: self.channel_end,
        }
    }
}

fn channel_bounds(channels: Range<u32>) -> Result<(i32, i32)> {
    if channels.start >= channels.end {
        return Err(Error::InvalidRoi(format!(
            "channel range must be non-empty and increasing, got {}..{}",
            channels.start, channels.end
        )));
    }
    let channel_begin = i32::try_from(channels.start)
        .map_err(|_| Error::InvalidRoi("channel start exceeds i32::MAX".to_owned()))?;
    let channel_end = i32::try_from(channels.end)
        .map_err(|_| Error::InvalidRoi("channel end exceeds i32::MAX".to_owned()))?;
    Ok((channel_begin, channel_end))
}

fn validate_range(name: &str, range: &Range<i32>) -> Result<()> {
    if range.start >= range.end {
        return Err(Error::InvalidRoi(format!(
            "{name} range must be non-empty and increasing, got {}..{}",
            range.start, range.end
        )));
    }
    if i64::from(range.end) - i64::from(range.start) > i64::from(i32::MAX) {
        return Err(Error::InvalidRoi(format!(
            "{name} range is too large for OpenImageIO"
        )));
    }
    Ok(())
}

fn difference(begin: i32, end: i32) -> usize {
    (i64::from(end) - i64::from(begin)) as usize
}

fn checked_end(name: &str, origin: i32, size: u32) -> Result<i32> {
    let size = i32::try_from(size)
        .map_err(|_| Error::InvalidImageSpec(format!("{name} size does not fit in an i32")))?;
    origin.checked_add(size).ok_or_else(|| {
        Error::InvalidImageSpec(format!("{name} data-window endpoint overflows i32"))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_empty_and_reversed_ranges() {
        assert!(Roi::new(0..0, 0..1, 0..1, 0..1).is_err());
        let reversed = Range { start: 1, end: 0 };
        assert!(Roi::new(reversed, 0..1, 0..1, 0..1).is_err());
        assert!(Roi::new(0..1, 0..1, 0..1, 1..1).is_err());
    }

    #[test]
    fn computes_checked_element_count() {
        let roi = Roi::new(-2..2, 4..6, 0..1, 1..4).unwrap();
        assert_eq!(roi.element_count().unwrap(), 24);
    }

    #[test]
    fn detects_element_count_overflow() {
        let roi = Roi::new(i32::MIN..-1, i32::MIN..-1, i32::MIN..-1, 0..i32::MAX as u32).unwrap();
        assert!(matches!(
            roi.element_count(),
            Err(Error::BufferSizeOverflow)
        ));
    }
}
