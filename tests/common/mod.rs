//! Helpers shared by the integration tests.

#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};

use oiio::{f16, ImageOutput, ImageSpec, Pixel, Result};

/// A scratch directory that removes itself when the test ends.
pub struct ScratchDir(PathBuf);

impl ScratchDir {
    pub fn new(name: &str) -> Self {
        static COUNTER: AtomicU32 = AtomicU32::new(0);
        let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
        let path =
            std::env::temp_dir().join(format!("oiio-bind-{name}-{}-{unique}", std::process::id()));
        std::fs::create_dir_all(&path).expect("could not create the scratch directory");
        Self(path)
    }

    pub fn file(&self, name: &str) -> PathBuf {
        self.0.join(name)
    }
}

impl Drop for ScratchDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// A deterministic ramp, distinct per channel and per pixel.
pub fn f32_ramp(count: usize) -> Vec<f32> {
    (0..count).map(|index| index as f32 * 0.125 - 4.0).collect()
}

pub fn f16_ramp(count: usize) -> Vec<f16> {
    (0..count)
        .map(|index| f16::from_f32(index as f32 * 0.5 - 2.0))
        .collect()
}

/// Write a whole image in one call.
pub fn write_image<T: Pixel>(path: &Path, spec: &ImageSpec, pixels: &[T]) -> Result<()> {
    let mut output = ImageOutput::create(path, spec)?;
    output.write_image(pixels)?;
    output.close()
}

/// Pull the region `x` by `y` and channels `channels` out of a whole-image
/// buffer, so a partial read can be compared against it.
pub fn crop<T: Copy>(
    pixels: &[T],
    width: u32,
    channels: u32,
    x: std::ops::Range<u32>,
    y: std::ops::Range<u32>,
    wanted_channels: std::ops::Range<u32>,
) -> Vec<T> {
    let mut cropped = Vec::new();
    for row in y {
        for column in x.clone() {
            let pixel = (row as usize * width as usize + column as usize) * channels as usize;
            for channel in wanted_channels.clone() {
                cropped.push(pixels[pixel + channel as usize]);
            }
        }
    }
    cropped
}
