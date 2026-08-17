//! An image and image-sequence viewer built on the `oiio` crate.
//!
//! Frames decode to linear `f32` on a background thread through
//! [`oiio::ImageInput`] and land in a small LRU cache, with the frames ahead
//! of the playhead prefetched so playback does not stall on I/O. The UI
//! thread applies the display transform — exposure, channel isolation, sRGB
//! encode — to the cached linear pixels and shows the result through
//! `eframe`/`egui`, with playback controls, cursor-centred zoom and pan, an
//! inspector for the spec and its metadata, and multi-part (EXR) support. A
//! file that fails to decode shows as a placeholder carrying the error text,
//! and playback runs on past it.
//!
//! Pass one or more image files, or a single directory whose image files
//! form the sequence. `--check <file>` decodes one file through the same
//! path and prints its shape without opening a window, so the pipeline can
//! be validated in headless environments.

mod app;
mod decode;

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use eframe::egui;

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

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("oiio-viewer")
            .with_inner_size([1280.0, 800.0])
            .with_min_inner_size([480.0, 320.0]),
        ..Default::default()
    };
    let result = eframe::run_native(
        "oiio-viewer",
        options,
        Box::new(move |cc| Ok(Box::new(app::ViewerApp::new(cc, paths)))),
    );
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("oiio-viewer: the window could not be run: {error}");
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
    eprintln!("keys:  Space        play/pause");
    eprintln!("       Right/Left   next/previous frame (wraps around)");
    eprintln!("       Up/Down, S   next/previous subimage of a multi-part file");
    eprintln!("       + / -        exposure up/down one stop (also = / _)");
    eprintln!("       Home         reset exposure");
    eprintln!("       F            fit the image to the window");
    eprintln!("       1            one image pixel per screen pixel");
    eprintln!("       R            reload the current frame from disk");
    eprintln!("       Esc or Q     quit");
}

/// Decode one file through the same path the window uses and report its
/// shape, so the pipeline can be exercised without opening a window.
fn run_check(arguments: &[String]) -> ExitCode {
    let [path] = arguments else {
        print_usage();
        return ExitCode::from(2);
    };
    match decode::load_frame(Path::new(path), 0) {
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
    paths.sort_by(|a, b| natural_order(a, b));
    Ok(paths)
}

/// Order paths so embedded frame numbers compare numerically — `frame2`
/// before `frame10` — which plain byte order gets wrong for unpadded
/// numbering. Equal names fall back to the full path for determinism.
fn natural_order(a: &Path, b: &Path) -> std::cmp::Ordering {
    use std::cmp::Ordering;
    let a_name = a
        .file_name()
        .map(|name| name.to_string_lossy())
        .unwrap_or_default();
    let b_name = b
        .file_name()
        .map(|name| name.to_string_lossy())
        .unwrap_or_default();
    let mut a_rest = a_name.as_bytes();
    let mut b_rest = b_name.as_bytes();
    loop {
        match (a_rest.first(), b_rest.first()) {
            // Names that tie — equal, or equal but for zero padding — are
            // settled by the full path so the order stays total.
            (None, None) => return a.cmp(b),
            (None, Some(_)) => return Ordering::Less,
            (Some(_), None) => return Ordering::Greater,
            (Some(&x), Some(&y)) if x.is_ascii_digit() && y.is_ascii_digit() => {
                fn digits(bytes: &[u8]) -> usize {
                    bytes.iter().take_while(|b| b.is_ascii_digit()).count()
                }
                // Strip leading zeros — keeping one so a run of zeros still
                // has a value — then longer runs are larger numbers and
                // equal-length runs compare digit by digit.
                fn strip(run: &[u8]) -> &[u8] {
                    let zeros = run.iter().take_while(|&&b| b == b'0').count();
                    &run[zeros.min(run.len() - 1)..]
                }
                let a_run = digits(a_rest);
                let b_run = digits(b_rest);
                let a_digits = strip(&a_rest[..a_run]);
                let b_digits = strip(&b_rest[..b_run]);
                let numeric = a_digits
                    .len()
                    .cmp(&b_digits.len())
                    .then_with(|| a_digits.cmp(b_digits));
                if numeric != Ordering::Equal {
                    return numeric;
                }
                a_rest = &a_rest[a_run..];
                b_rest = &b_rest[b_run..];
            }
            (Some(&x), Some(&y)) => {
                if x != y {
                    return x.cmp(&y);
                }
                a_rest = &a_rest[1..];
                b_rest = &b_rest[1..];
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Frame numbers sort numerically whether padded or not; plain byte
    /// order would put frame10 before frame2.
    #[test]
    fn frames_sort_numerically() {
        fn sorted(mut names: Vec<&str>) -> Vec<&str> {
            names.sort_by(|a, b| natural_order(Path::new(a), Path::new(b)));
            names
        }
        assert_eq!(
            sorted(vec!["frame10.exr", "frame2.exr", "frame1.exr"]),
            vec!["frame1.exr", "frame2.exr", "frame10.exr"],
        );
        assert_eq!(
            sorted(vec!["shot.0010.exr", "shot.0009.exr", "shot.0100.exr"]),
            vec!["shot.0009.exr", "shot.0010.exr", "shot.0100.exr"],
        );
        // Numbers embedded mid-name and multiple runs both compare by value.
        assert_eq!(
            sorted(vec!["v2_frame10.exr", "v2_frame9.exr", "v10_frame1.exr"]),
            vec!["v2_frame9.exr", "v2_frame10.exr", "v10_frame1.exr"],
        );
        // Zero padding alone never makes two names unordered.
        use std::cmp::Ordering;
        assert_ne!(
            natural_order(Path::new("f01.exr"), Path::new("f1.exr")),
            Ordering::Equal,
        );
        assert_eq!(
            natural_order(Path::new("same.exr"), Path::new("same.exr")),
            Ordering::Equal,
        );
    }
}
