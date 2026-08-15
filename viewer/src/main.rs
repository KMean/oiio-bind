//! A minimal image and image-sequence viewer built on the `oiio` crate.
//!
//! Pass one or more image files, or a single directory whose image files form
//! the sequence. Each frame is decoded to linear `f32` through
//! [`oiio::ImageInput`], multiplied by an exposure gain, sRGB-encoded and
//! presented on the CPU with `winit` and `softbuffer`. Only the frame being
//! shown is held in memory; stepping decodes the next file on demand, and a
//! file that fails to decode shows a dark-red frame with the error in the
//! window title instead of ending the run.

use std::num::NonZeroU32;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::Arc;

use oiio::ImageInput;
use winit::application::ApplicationHandler;
use winit::dpi::LogicalSize;
use winit::event::{ElementState, KeyEvent, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{Key, NamedKey};
use winit::window::{Window, WindowId};

/// Letterbox background, a dark grey in softbuffer's 0RGB pixel layout.
const BACKGROUND: u32 = 0x0020_2020;

/// Whole-frame colour shown when the current file fails to decode.
const ERROR_COLOR: u32 = 0x0040_0000;

/// Extensions accepted when a directory argument stands for a sequence.
const IMAGE_EXTENSIONS: &[&str] = &[
    "bmp", "cin", "dds", "dpx", "exr", "fits", "gif", "hdr", "heic", "heif", "ico", "iff", "j2k",
    "jp2", "jpeg", "jpg", "png", "pnm", "ppm", "pgm", "pbm", "psd", "rla", "sgi", "tga", "tif",
    "tiff", "tx", "webp",
];

fn main() -> ExitCode {
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    if arguments.is_empty() {
        print_usage();
        return ExitCode::from(2);
    }
    if arguments[0] == "--check" {
        return run_check(&arguments[1..]);
    }

    let paths = match resolve_paths(&arguments) {
        Ok(paths) => paths,
        Err(message) => {
            eprintln!("oiio-viewer: {message}");
            return ExitCode::FAILURE;
        }
    };

    let event_loop = match EventLoop::new() {
        Ok(event_loop) => event_loop,
        Err(error) => {
            eprintln!("oiio-viewer: could not start the event loop: {error}");
            return ExitCode::FAILURE;
        }
    };
    // The viewer only reacts to input, so the loop can sleep between events.
    event_loop.set_control_flow(ControlFlow::Wait);

    let mut app = App::new(paths);
    match event_loop.run_app(&mut app) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("oiio-viewer: the event loop failed: {error}");
            ExitCode::FAILURE
        }
    }
}

/// Print how to invoke the viewer and what the keys do.
fn print_usage() {
    eprintln!("usage: oiio-viewer <image> [<image>...]");
    eprintln!("       oiio-viewer <directory>");
    eprintln!("       oiio-viewer --check <image>");
    eprintln!();
    eprintln!("keys:  Right/Left   next/previous frame (wraps around)");
    eprintln!("       + / -        exposure up/down one stop (also = / _)");
    eprintln!("       Home         reset exposure");
    eprintln!("       S            next subimage of a multi-part file");
    eprintln!("       R            reload the current file from disk");
    eprintln!("       Esc or Q     quit");
}

/// Decode one file through the same path the window uses and report its
/// shape, so the pipeline can be exercised without opening a window.
fn run_check(arguments: &[String]) -> ExitCode {
    let [path] = arguments else {
        print_usage();
        return ExitCode::from(2);
    };
    match load_frame(Path::new(path), 0) {
        Ok(frame) => {
            println!("{}x{}x{} {path}", frame.width, frame.height, frame.channels);
            ExitCode::SUCCESS
        }
        Err(message) => {
            eprintln!("oiio-viewer: {path}: {message}");
            ExitCode::FAILURE
        }
    }
}

/// Turn the command line into the frame sequence: either every argument is a
/// file path, or a single directory argument stands for the image files in it.
fn resolve_paths(arguments: &[String]) -> Result<Vec<PathBuf>, String> {
    if let [only] = arguments {
        let path = Path::new(only);
        if path.is_dir() {
            return collect_sequence(path);
        }
    }
    Ok(arguments.iter().map(PathBuf::from).collect())
}

/// List the image files of `directory` in sorted order.
///
/// Shot directories often hold the same frames in several formats — EXR
/// renders next to JPEG previews — and interleaving them makes stepping
/// alternate formats. Only the dominant extension forms the sequence, with
/// EXR winning ties.
fn collect_sequence(directory: &Path) -> Result<Vec<PathBuf>, String> {
    let describe =
        |error: std::io::Error| format!("could not read {}: {error}", directory.display());
    let mut paths = Vec::new();
    for entry in std::fs::read_dir(directory).map_err(describe)? {
        let path = entry.map_err(describe)?.path();
        let is_image = path
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| {
                IMAGE_EXTENSIONS.contains(&extension.to_ascii_lowercase().as_str())
            });
        if is_image && path.is_file() {
            paths.push(path);
        }
    }
    if paths.is_empty() {
        return Err(format!("no image files found in {}", directory.display()));
    }

    let extension_of = |path: &PathBuf| -> String {
        path.extension()
            .and_then(|extension| extension.to_str())
            .map(|extension| extension.to_ascii_lowercase())
            .unwrap_or_default()
    };
    let mut counts: Vec<(String, usize)> = Vec::new();
    for path in &paths {
        let extension = extension_of(path);
        match counts.iter_mut().find(|(name, _)| *name == extension) {
            Some((_, count)) => *count += 1,
            None => counts.push((extension, 1)),
        }
    }
    // Most files wins; EXR breaks ties, then alphabetical order for
    // determinism.
    counts.sort_by(|a, b| {
        b.1.cmp(&a.1)
            .then_with(|| (b.0 == "exr").cmp(&(a.0 == "exr")))
            .then_with(|| a.0.cmp(&b.0))
    });
    let chosen = counts[0].0.clone();
    let dropped = paths.len() - counts[0].1;
    if dropped > 0 {
        eprintln!(
            "oiio-viewer: sequence is the {} .{chosen} file(s); ignoring {dropped} other image file(s)",
            counts[0].1
        );
        paths.retain(|path| extension_of(path) == chosen);
    }
    paths.sort();
    Ok(paths)
}

/// One decoded image: its dimensions, channel count, the pixels as
/// interleaved linear `f32` in scanline order, and where it sits in a
/// multi-part file.
struct Frame {
    width: u32,
    height: u32,
    channels: u32,
    pixels: Vec<f32>,
    /// How many subimages the file holds; EXR multi-part files have several.
    subimage_count: u32,
    /// A label for the part being shown: its recorded name, or its channel
    /// names when it has none.
    part_label: String,
}

/// The sRGB decode curve, the inverse of [`encode_channel`]'s encode half.
fn srgb_to_linear(encoded: f32) -> f32 {
    if encoded <= 0.040_45 {
        encoded / 12.92
    } else {
        ((encoded + 0.055) / 1.055).powf(2.4)
    }
}

/// Decode one subimage of `path` into an `f32` buffer sized from its spec.
///
/// Every failure becomes a string so the caller can show it in the window
/// title and keep stepping through the sequence instead of crashing.
///
/// Files that store display-encoded pixels — anything whose colour space
/// says sRGB, and integer-format files that say nothing — are decoded to
/// linear here, so the one display transform in [`encode_channel`] is right
/// for every source. Without this, an ordinary JPEG would be sRGB-encoded
/// twice and wash out.
fn load_frame(path: &Path, subimage: u32) -> Result<Frame, String> {
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
    let mut pixels = vec![0.0_f32; element_count];
    input
        .read_image_into_at(subimage, 0, &mut pixels)
        .map_err(|error| error.to_string())?;

    // Count the parts by probing specs; multi-part files are the point of
    // the S key. Sixty-four is far beyond any real file and keeps a
    // pathological one from stalling the load.
    let mut subimage_count = subimage + 1;
    while subimage_count < 64 && input.image_spec_at(subimage_count, 0).is_ok() {
        subimage_count += 1;
    }
    input.close().map_err(|error| error.to_string())?;

    // Label the part: EXR parts carry their name; otherwise the channel
    // names say what the part holds (three suffice for a title).
    let part_label = spec
        .attribute("oiio:subimagename")
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
        });

    // Decide whether the pixels arrived display-encoded. The colour space
    // attribute is authoritative when it names sRGB; integer files that say
    // nothing are display-referred in practice, while float files default
    // to linear.
    let colorspace = spec
        .attribute("oiio:ColorSpace")
        .and_then(|value| value.as_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    let integer_format = !matches!(
        spec.format(),
        oiio::PixelFormat::F16 | oiio::PixelFormat::F32 | oiio::PixelFormat::F64
    );
    let display_encoded = colorspace.contains("srgb")
        || (colorspace.is_empty() && integer_format)
        || colorspace.starts_with("g22")
        || colorspace.starts_with("gamma22");
    if display_encoded {
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
        subimage_count,
        part_label,
    })
}

/// Apply the exposure gain and the sRGB transfer curve to one linear channel
/// value, returning an eight-bit result in the low bits of a `u32`.
fn encode_channel(linear: f32, gain: f32) -> u32 {
    let scaled = linear * gain;
    // NaN is squashed to black explicitly, since `clamp` would let it through.
    let v = if scaled.is_nan() {
        0.0
    } else {
        scaled.clamp(0.0, 1.0)
    };
    let encoded = if v <= 0.003_130_8 {
        v * 12.92
    } else {
        1.055 * v.powf(1.0 / 2.4) - 0.055
    };
    (encoded * 255.0 + 0.5) as u32
}

/// Render `frame` into `target`, nearest-neighbour scaled to fit the window
/// while preserving the aspect ratio, letterboxed on a dark grey background.
///
/// Single-channel images are shown as grey; a second channel is treated as
/// alpha and ignored, as is the fourth channel of an RGBA image.
fn draw_frame(target: &mut [u32], win_w: u32, win_h: u32, frame: &Frame, gain: f32) {
    target.fill(BACKGROUND);
    let img_w = frame.width.max(1);
    let img_h = frame.height.max(1);
    let scale = f64::min(
        f64::from(win_w) / f64::from(img_w),
        f64::from(win_h) / f64::from(img_h),
    );
    let dst_w = ((f64::from(img_w) * scale).round() as u32).clamp(1, win_w);
    let dst_h = ((f64::from(img_h) * scale).round() as u32).clamp(1, win_h);
    let x0 = (win_w - dst_w) / 2;
    let y0 = (win_h - dst_h) / 2;
    let channels = frame.channels as usize;

    // Precompute the source column of every destination column, so the inner
    // loop is a table lookup rather than a division per pixel.
    let columns: Vec<usize> = (0..dst_w)
        .map(|dx| {
            ((u64::from(dx) * u64::from(img_w)) / u64::from(dst_w)).min(u64::from(img_w) - 1)
                as usize
        })
        .collect();

    for dy in 0..dst_h {
        let src_y = ((u64::from(dy) * u64::from(img_h)) / u64::from(dst_h))
            .min(u64::from(img_h) - 1) as usize;
        let row_len = img_w as usize * channels;
        let src_row = &frame.pixels[src_y * row_len..][..row_len];
        let row_start = ((y0 + dy) * win_w + x0) as usize;
        let out_row = &mut target[row_start..row_start + dst_w as usize];
        for (out, &src_x) in out_row.iter_mut().zip(&columns) {
            let texel = &src_row[src_x * channels..];
            let (r, g, b) = match channels {
                // One channel is grey; with two, the second is alpha and the
                // first is still grey.
                1 | 2 => (texel[0], texel[0], texel[0]),
                _ => (texel[0], texel[1], texel[2]),
            };
            *out = (encode_channel(r, gain) << 16)
                | (encode_channel(g, gain) << 8)
                | encode_channel(b, gain);
        }
    }
}

/// The viewer state driven by the winit 0.30 [`ApplicationHandler`] callbacks.
struct App {
    /// The sequence in display order. Never empty.
    paths: Vec<PathBuf>,
    /// Index of the frame being shown.
    index: usize,
    /// Which subimage (EXR part) of the current file is shown.
    subimage: u32,
    /// Exposure in stops; the gain applied to the pixels is two to this power.
    exposure_stops: f32,
    /// The decode result of the current frame, kept until the frame changes
    /// or a reload is requested. `None` means it has not been decoded yet;
    /// caching the `Err` too keeps a broken file from being re-read on every
    /// redraw.
    frame: Option<Result<Frame, String>>,
    window: Option<Arc<Window>>,
    surface: Option<softbuffer::Surface<Arc<Window>, Arc<Window>>>,
    /// Kept alive because the surface was created from it; declared after the
    /// surface so it is dropped after it too.
    _context: Option<softbuffer::Context<Arc<Window>>>,
}

impl App {
    fn new(paths: Vec<PathBuf>) -> Self {
        Self {
            paths,
            index: 0,
            subimage: 0,
            exposure_stops: 0.0,
            frame: None,
            window: None,
            surface: None,
            _context: None,
        }
    }

    /// Move `delta` frames through the sequence, wrapping at both ends, and
    /// drop the cache so the new frame is decoded on the next redraw.
    fn step(&mut self, delta: isize) {
        let length = self.paths.len() as isize;
        self.index = (self.index as isize + delta).rem_euclid(length) as usize;
        // A new file starts at its first part; part choice is per-file.
        self.subimage = 0;
        self.reload();
    }

    /// Show the next subimage of a multi-part file, wrapping around.
    fn next_part(&mut self) {
        if let Some(Ok(frame)) = &self.frame {
            if frame.subimage_count > 1 {
                self.subimage = (self.subimage + 1) % frame.subimage_count;
                self.reload();
            }
        }
    }

    /// Forget the decoded frame so the next redraw reads the file again.
    fn reload(&mut self) {
        self.frame = None;
        self.request_redraw();
    }

    fn set_exposure(&mut self, stops: f32) {
        self.exposure_stops = stops;
        self.request_redraw();
    }

    fn request_redraw(&self) {
        if let Some(window) = &self.window {
            window.request_redraw();
        }
    }

    /// The window title: name, position in the sequence, exposure, and the
    /// load error if the current file could not be decoded.
    fn title(&self) -> String {
        let current = &self.paths[self.index];
        let name = current
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| current.display().to_string());
        let mut title = format!(
            "oiio-viewer — {name} [frame {}/{}] [exposure {:+.1}]",
            self.index + 1,
            self.paths.len(),
            self.exposure_stops
        );
        if let Some(Ok(frame)) = &self.frame {
            if frame.subimage_count > 1 {
                title.push_str(&format!(
                    " [part {}/{}: {}]",
                    self.subimage + 1,
                    frame.subimage_count,
                    frame.part_label
                ));
            }
        }
        if let Some(Err(message)) = &self.frame {
            // Titles are one line, so keep only the start of the error.
            let brief: String = message
                .lines()
                .next()
                .unwrap_or("unknown error")
                .chars()
                .take(120)
                .collect();
            title.push_str(&format!(" — load failed: {brief}"));
        }
        title
    }

    /// Decode the current frame if needed, then draw and present it.
    fn redraw(&mut self) {
        // Decoding happens here, lazily, so stepping ten frames costs one read.
        if self.frame.is_none() {
            self.frame = Some(load_frame(&self.paths[self.index], self.subimage));
        }
        let Some(window) = self.window.clone() else {
            return;
        };
        window.set_title(&self.title());

        let size = window.inner_size();
        let (Some(width), Some(height)) =
            (NonZeroU32::new(size.width), NonZeroU32::new(size.height))
        else {
            // A minimised window has no pixels to draw into.
            return;
        };
        let Some(surface) = self.surface.as_mut() else {
            return;
        };
        if let Err(error) = surface.resize(width, height) {
            eprintln!("oiio-viewer: could not resize the surface: {error}");
            return;
        }
        let mut buffer = match surface.buffer_mut() {
            Ok(buffer) => buffer,
            Err(error) => {
                eprintln!("oiio-viewer: could not map the surface: {error}");
                return;
            }
        };
        match &self.frame {
            Some(Ok(frame)) => draw_frame(
                &mut buffer,
                size.width,
                size.height,
                frame,
                self.exposure_stops.exp2(),
            ),
            // A file that failed to load shows as a solid dark red frame.
            _ => buffer.fill(ERROR_COLOR),
        }
        if let Err(error) = buffer.present() {
            eprintln!("oiio-viewer: could not present the frame: {error}");
        }
    }

    fn handle_key(&mut self, event_loop: &ActiveEventLoop, event: KeyEvent) {
        if event.state != ElementState::Pressed {
            return;
        }
        match event.logical_key.as_ref() {
            Key::Named(NamedKey::ArrowRight) => self.step(1),
            Key::Named(NamedKey::ArrowLeft) => self.step(-1),
            Key::Named(NamedKey::Home) => self.set_exposure(0.0),
            Key::Named(NamedKey::Escape) => event_loop.exit(),
            // Plus shares a key with equals, and minus with underscore, so
            // both spellings work without the shift key mattering.
            Key::Character("+") | Key::Character("=") => {
                self.set_exposure(self.exposure_stops + 1.0);
            }
            Key::Character("-") | Key::Character("_") => {
                self.set_exposure(self.exposure_stops - 1.0);
            }
            Key::Character("s") | Key::Character("S") => self.next_part(),
            Key::Character("r") | Key::Character("R") => self.reload(),
            Key::Character("q") | Key::Character("Q") => event_loop.exit(),
            _ => {}
        }
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        // Desktop platforms call this once when the loop starts; the window
        // and the surface are created here because winit 0.30 only allows
        // window creation while the event loop is active.
        if self.window.is_some() {
            return;
        }
        let attributes = Window::default_attributes()
            .with_title("oiio-viewer")
            .with_inner_size(LogicalSize::new(1024.0, 768.0));
        let window = match event_loop.create_window(attributes) {
            Ok(window) => Arc::new(window),
            Err(error) => {
                eprintln!("oiio-viewer: could not create a window: {error}");
                event_loop.exit();
                return;
            }
        };
        let context = match softbuffer::Context::new(window.clone()) {
            Ok(context) => context,
            Err(error) => {
                eprintln!("oiio-viewer: could not create a graphics context: {error}");
                event_loop.exit();
                return;
            }
        };
        let surface = match softbuffer::Surface::new(&context, window.clone()) {
            Ok(surface) => surface,
            Err(error) => {
                eprintln!("oiio-viewer: could not create a surface: {error}");
                event_loop.exit();
                return;
            }
        };
        self.window = Some(window);
        self.surface = Some(surface);
        self._context = Some(context);
        self.request_redraw();
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        event: WindowEvent,
    ) {
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(_) => self.request_redraw(),
            WindowEvent::RedrawRequested => self.redraw(),
            WindowEvent::KeyboardInput { event, .. } => self.handle_key(event_loop, event),
            _ => {}
        }
    }
}
