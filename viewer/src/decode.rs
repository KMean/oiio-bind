//! Decoding: from a file on disk to a linear `f32` frame, and the worker
//! thread that keeps that work off the UI thread.
//!
//! [`load_frame`] is the one decode path — the window and `--check` both use
//! it. [`spawn_decoder`] wraps it in a thread fed by a request channel, so
//! the interface keeps drawing while OpenImageIO reads. Requests carry a
//! generation number; a reply stamped with an old generation arrives after
//! the user has reloaded or jumped elsewhere, and the UI drops it.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Arc;

use eframe::egui;
use oiio::{ImageInput, ImageSpec};

/// Parts are counted by probing specs until one fails; this cap is far
/// beyond any real file and keeps a pathological one from stalling a load.
const MAX_SUBIMAGES: u32 = 64;

/// Refuse a decode whose header claims more than this many bytes of linear
/// `f32`. A corrupt or hostile header should become an error placeholder,
/// not an allocation the process cannot survive.
const MAX_FRAME_BYTES: usize = 4 << 30;

/// One decoded image: its shape, the pixels as interleaved linear `f32` in
/// scanline order, its spec for the inspector, and how it sits in a
/// multi-part file.
pub struct Frame {
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// Interleaved channels per pixel.
    pub channels: u32,
    /// The linear pixel data, `width * height * channels` values long.
    pub pixels: Vec<f32>,
    /// The spec of the decoded subimage, kept for the inspector panels.
    pub spec: ImageSpec,
    /// How many subimages the file holds; EXR multi-part files have several.
    pub subimage_count: u32,
    /// One label per part: its recorded name, or its channel names when it
    /// has none.
    part_labels: Vec<String>,
}

impl Frame {
    /// The pixel buffer's size, the currency of the cache's byte budget.
    pub fn byte_size(&self) -> usize {
        self.pixels.len() * std::mem::size_of::<f32>()
    }

    /// The label of one part, or nothing when the probe fell short of it.
    pub fn part_label(&self, subimage: u32) -> &str {
        self.part_labels
            .get(subimage as usize)
            .map_or("", String::as_str)
    }
}

/// The sRGB decode curve, the inverse of [`linear_to_srgb`].
pub fn srgb_to_linear(encoded: f32) -> f32 {
    if encoded <= 0.040_45 {
        encoded / 12.92
    } else {
        ((encoded + 0.055) / 1.055).powf(2.4)
    }
}

/// The sRGB encode curve applied for display.
pub fn linear_to_srgb(linear: f32) -> f32 {
    if linear <= 0.003_130_8 {
        linear * 12.92
    } else {
        1.055 * linear.powf(1.0 / 2.4) - 0.055
    }
}

/// Apply the exposure gain and the sRGB transfer curve to one linear channel
/// value, returning the eight-bit display value.
pub fn encode_channel(linear: f32, gain: f32) -> u8 {
    let scaled = linear * gain;
    // NaN is squashed to black explicitly, since `clamp` would let it through.
    let value = if scaled.is_nan() {
        0.0
    } else {
        scaled.clamp(0.0, 1.0)
    };
    (linear_to_srgb(value) * 255.0 + 0.5) as u8
}

/// Label one part: EXR parts carry their recorded name; otherwise the
/// channel names say what the part holds (three suffice for a label).
fn part_label_of(spec: &ImageSpec) -> String {
    spec.attribute("oiio:subimagename")
        .and_then(|value| value.as_str())
        .map(str::to_owned)
        .unwrap_or_else(|| {
            let names = spec.channel_names();
            let mut label = names
                .iter()
                .take(3)
                .map(String::as_str)
                .collect::<Vec<_>>()
                .join(",");
            if names.len() > 3 {
                label.push('…');
            }
            label
        })
}

/// Decode one subimage of `path` into an `f32` buffer sized from its spec.
///
/// Every failure becomes a string so the caller can show it as a placeholder
/// and keep stepping through the sequence instead of crashing.
///
/// Files that store display-encoded pixels — anything whose colour space
/// says sRGB, and integer-format files that say nothing — are decoded to
/// linear here, so the one display transform in the UI is right for every
/// source. Without this, an ordinary JPEG would be sRGB-encoded twice and
/// wash out.
pub fn load_frame(path: &Path, subimage: u32) -> Result<Frame, String> {
    let mut input = ImageInput::from_path(path).map_err(|error| error.to_string())?;
    let spec = input
        .image_spec_at(subimage, 0)
        .map_err(|error| error.to_string())?;
    let [width, height, depth] = spec.dimensions();
    if depth > 1 {
        return Err(format!(
            "volumetric images are not supported (depth {depth})"
        ));
    }
    let channels = spec.channel_count();
    if channels == 0 {
        return Err("the image reports zero channels".to_owned());
    }
    let element_count = spec.element_count().map_err(|error| error.to_string())?;
    let bytes = element_count.saturating_mul(std::mem::size_of::<f32>());
    if bytes > MAX_FRAME_BYTES {
        return Err(format!(
            "the header claims {width}x{height}x{channels} pixels — {} GiB as f32; refusing",
            bytes >> 30
        ));
    }
    // A fallible reservation turns an allocation the machine cannot make
    // into an ordinary error instead of an abort.
    let mut pixels: Vec<f32> = Vec::new();
    pixels
        .try_reserve_exact(element_count)
        .map_err(|_| format!("out of memory allocating {element_count} samples"))?;
    pixels.resize(element_count, 0.0);
    input
        .read_image_into_at(subimage, 0, &mut pixels)
        .map_err(|error| error.to_string())?;

    // Collect every part's label by probing specs; multi-part files are the
    // point of the part selector, and a spec probe reads only headers.
    let mut part_labels = Vec::new();
    for part in 0..MAX_SUBIMAGES {
        if part == subimage {
            part_labels.push(part_label_of(&spec));
        } else {
            match input.image_spec_at(part, 0) {
                Ok(part_spec) => part_labels.push(part_label_of(&part_spec)),
                Err(_) => break,
            }
        }
    }
    // The part being shown always counts, even if a format refused to seek
    // backwards while probing the ones before it.
    let subimage_count = (part_labels.len() as u32).max(subimage + 1);
    input.close().map_err(|error| error.to_string())?;

    let colorspace = spec
        .attribute("oiio:ColorSpace")
        .and_then(|value| value.as_str())
        .unwrap_or("");
    let integer_format = !matches!(
        spec.format(),
        oiio::PixelFormat::F16 | oiio::PixelFormat::F32 | oiio::PixelFormat::F64
    );
    if is_display_encoded(colorspace, integer_format) {
        let alpha = spec.alpha_channel();
        let stride = channels as usize;
        for pixel in pixels.chunks_exact_mut(stride) {
            for (index, value) in pixel.iter_mut().enumerate() {
                if Some(index as u32) != alpha {
                    *value = srgb_to_linear(*value);
                }
            }
        }
    }

    Ok(Frame {
        width,
        height,
        channels,
        pixels,
        spec,
        subimage_count,
        part_labels,
    })
}

/// Whether pixels labelled with `colorspace` arrived display-encoded and
/// need decoding to linear.
///
/// A name that says sRGB means display-encoded — unless it also declares a
/// linear response, because linear spaces commonly embed "srgb" to name
/// their *primaries* rather than their transfer curve: `lin_srgb`, "Linear
/// Rec.709 (sRGB)". Treating those as encoded would linearise already-linear
/// pixels a second time and crush the image. A file that names no colour
/// space at all is display-referred in practice when its format is integer,
/// while float files default to linear.
fn is_display_encoded(colorspace: &str, integer_format: bool) -> bool {
    let colorspace = colorspace.to_ascii_lowercase();
    let linear_tagged = colorspace.starts_with("lin") || colorspace.contains("linear");
    (colorspace.contains("srgb") && !linear_tagged)
        || (colorspace.is_empty() && integer_format)
        || colorspace.starts_with("g22")
        || colorspace.starts_with("gamma22")
}

/// What the UI asks the decode thread for: one subimage of one file.
pub struct DecodeRequest {
    /// The file to read.
    pub path: PathBuf,
    /// Where the file sits in the sequence, echoed back as the cache key.
    pub index: usize,
    /// Which part of the file to decode.
    pub subimage: u32,
    /// Stamps the request so a reply outlived by a reload or a jump is
    /// recognised as stale and dropped.
    pub generation: u64,
}

/// The decode thread's answer to one [`DecodeRequest`].
pub struct DecodeReply {
    /// The sequence index the request named.
    pub index: usize,
    /// The subimage the request named.
    pub subimage: u32,
    /// The generation the request carried.
    pub generation: u64,
    /// The decoded frame, or the reason there is none.
    pub result: Result<Frame, String>,
}

/// Start the decode thread. Every reply pokes the UI awake through `ctx` so
/// a finished decode shows without waiting for the next input event, and the
/// thread ends when the request sender is dropped.
///
/// Two rules keep a backlog from burying the frame the user wants. A request
/// whose generation no longer matches `current_generation` is dead — its
/// reply would be dropped anyway — so it is skipped for the cost of an
/// atomic load instead of a full decode; scrubbing across a sequence leaves
/// a queue of these and they all vanish at once. And within the live
/// requests, the queue is drained and served newest first: the UI requests
/// the on-screen frame after its prefetches, so the most recent request is
/// always the one the user is looking at.
pub fn spawn_decoder(
    ctx: egui::Context,
    current_generation: Arc<AtomicU64>,
) -> (Sender<DecodeRequest>, Receiver<DecodeReply>) {
    let (request_sender, request_receiver) = mpsc::channel::<DecodeRequest>();
    let (reply_sender, reply_receiver) = mpsc::channel::<DecodeReply>();
    std::thread::spawn(move || {
        'serve: while let Ok(first) = request_receiver.recv() {
            let mut batch = vec![first];
            while let Ok(request) = request_receiver.try_recv() {
                batch.push(request);
            }
            while let Some(request) = batch.pop() {
                if request.generation != current_generation.load(Ordering::Relaxed) {
                    // Skipping sends no reply; the bump that retired this
                    // generation also cleared its pending entries.
                    continue;
                }
                // A panic inside the decode must not take the worker down —
                // a dead worker would wedge the viewer at "decoding…" — so
                // it is caught and reported like any other failed load.
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    load_frame(&request.path, request.subimage)
                }))
                .unwrap_or_else(|_| Err("the decoder panicked reading this file".to_owned()));
                let reply = DecodeReply {
                    index: request.index,
                    subimage: request.subimage,
                    generation: request.generation,
                    result,
                };
                if reply_sender.send(reply).is_err() {
                    // The UI is gone, and with it the point of decoding.
                    break 'serve;
                }
                ctx.request_repaint();
            }
        }
    });
    (request_sender, reply_receiver)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Display-encoded names route to linearisation; names that declare a
    /// linear response do not, even when they mention sRGB primaries.
    #[test]
    fn colorspace_routing() {
        for encoded in ["sRGB", "srgb_texture", "srgb_rec709_scene", "g22_rec709"] {
            assert!(is_display_encoded(encoded, false), "{encoded}");
        }
        for linear in [
            "lin_srgb",
            "lin_rec709_srgb",
            "Linear Rec.709 (sRGB)",
            "Utility - Linear - sRGB",
            "ACEScg",
            "scene_linear",
        ] {
            assert!(!is_display_encoded(linear, false), "{linear}");
            assert!(!is_display_encoded(linear, true), "{linear}");
        }
        // No label at all: integer files are display-referred in practice,
        // float files default to linear.
        assert!(is_display_encoded("", true));
        assert!(!is_display_encoded("", false));
    }

    /// The transfer pair round-trips and squashes NaN to black.
    #[test]
    fn transfer_curves() {
        for value in [0.0_f32, 0.001, 0.02, 0.18, 0.5, 1.0] {
            let there_and_back = srgb_to_linear(linear_to_srgb(value));
            assert!((there_and_back - value).abs() < 1.0e-6, "{value}");
        }
        assert_eq!(encode_channel(f32::NAN, 1.0), 0);
        assert_eq!(encode_channel(1.0, 1.0), 255);
        assert_eq!(encode_channel(0.0, 1.0), 0);
    }
}
