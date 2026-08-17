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
    eprintln!("       oiio-viewer <pattern>          e.g. shot.#.exr");
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

/// Turn the command line into the frame sequence: every argument is a file
/// path, or a single argument stands for a whole sequence — a directory
/// holding one, or an oiiotool-style pattern such as `shot.#.exr` naming
/// one directly.
fn resolve_paths(arguments: &[String]) -> Result<Vec<PathBuf>, String> {
    if let [only] = arguments {
        let path = Path::new(only);
        if path.is_dir() {
            return collect_sequence(path);
        }
        if path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.contains('#'))
        {
            return expand_pattern(path);
        }
    }
    Ok(arguments.iter().map(PathBuf::from).collect())
}

/// Expand a `shot.#.exr` argument into the matching frames on disk: files
/// whose name is the pattern's prefix, then digits — any width — then its
/// suffix, in frame order.
fn expand_pattern(pattern: &Path) -> Result<Vec<PathBuf>, String> {
    let name = pattern
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default();
    let first = name.find('#').expect("caller checked for a wildcard");
    let last = name.rfind('#').expect("caller checked for a wildcard");
    let (prefix, suffix) = (&name[..first], &name[last + 1..]);
    if suffix.contains('#') {
        return Err(format!(
            "{name}: only one `#` run can stand for the frame number"
        ));
    }
    let directory = match pattern.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => parent.to_path_buf(),
        _ => PathBuf::from("."),
    };
    let describe =
        |error: std::io::Error| format!("could not read {}: {error}", directory.display());
    let mut paths = Vec::new();
    for entry in std::fs::read_dir(&directory).map_err(describe)? {
        let path = entry.map_err(describe)?.path();
        let matches = path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| frame_of(name, prefix, suffix).is_some());
        if matches && path.is_file() {
            paths.push(path);
        }
    }
    if paths.is_empty() {
        return Err(format!(
            "no frames matching {name} in {}",
            directory.display()
        ));
    }
    paths.sort_by(|a, b| natural_order(a, b));
    Ok(paths)
}

/// The frame digits of `name` under a prefix/suffix pattern, or `None` when
/// the name does not match.
fn frame_of<'a>(name: &'a str, prefix: &str, suffix: &str) -> Option<&'a str> {
    let rest = name.strip_prefix(prefix)?;
    let digits = rest.strip_suffix(suffix)?;
    (!digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit())).then_some(digits)
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

    // Within one extension, distinct name patterns are distinct sequences —
    // beauty.####.exr next to depth.####.exr must not play concatenated.
    // The largest group wins, ties alphabetically; a folder with no
    // sequence structure at all stays browsable whole.
    let mut patterns: Vec<(String, usize)> = Vec::new();
    for path in &paths {
        let pattern = sequence_pattern(path);
        match patterns.iter_mut().find(|(name, _)| *name == pattern) {
            Some((_, count)) => *count += 1,
            None => patterns.push((pattern, 1)),
        }
    }
    patterns.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    if patterns.len() > 1 && patterns[0].1 > 1 {
        let (chosen_pattern, kept) = patterns[0].clone();
        eprintln!(
            "oiio-viewer: sequence is {chosen_pattern} ({kept} frames); ignoring {} other file(s) — pass a pattern or files to view those",
            paths.len() - kept
        );
        paths.retain(|path| sequence_pattern(path) == chosen_pattern);
    }
    paths.sort_by(|a, b| natural_order(a, b));
    Ok(paths)
}

/// The sequence pattern a file name belongs to: the last run of digits in
/// its stem replaced by `#`, so `beauty.0001.exr` and `beauty.0002.exr`
/// share a pattern that `depth.0001.exr` does not. Extension digits — as in
/// `.jp2` — are not frame numbers, and a name with no digits at all is its
/// own pattern.
fn sequence_pattern(path: &Path) -> String {
    let name = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    let (stem, extension) = match name.rfind('.') {
        Some(dot) => name.split_at(dot),
        None => (name.as_str(), ""),
    };
    let bytes = stem.as_bytes();
    let Some(end) = bytes.iter().rposition(|byte| byte.is_ascii_digit()) else {
        return name.clone();
    };
    let mut start = end;
    while start > 0 && bytes[start - 1].is_ascii_digit() {
        start -= 1;
    }
    format!("{}#{}{extension}", &stem[..start], &stem[end + 1..])
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

    /// Frames of one sequence share a pattern; a different stem, a
    /// different extension, or digits only in the extension do not.
    #[test]
    fn patterns_separate_sequences() {
        let pattern = |name: &str| sequence_pattern(Path::new(name));
        assert_eq!(pattern("multipart.0001.exr"), pattern("multipart.0008.exr"));
        assert_eq!(pattern("frame2.exr"), pattern("frame10.exr"));
        assert_ne!(
            pattern("multipart.0001.exr"),
            pattern("singlepart.0001.exr")
        );
        assert_ne!(pattern("beauty.0001.exr"), pattern("beauty.0001.jpg"));
        // The version number is part of the pattern; the frame is the
        // last digit run.
        assert_eq!(pattern("shot_v2.0001.exr"), "shot_v2.#.exr");
        // `.jp2` digits are extension, not frame number.
        assert_eq!(pattern("shot.0001.jp2"), "shot.#.jp2");
        assert_eq!(pattern("nodigits.exr"), "nodigits.exr");
    }

    /// A `#` pattern matches digits of any width between its fixed parts.
    #[test]
    fn pattern_matching() {
        assert_eq!(frame_of("shot.0001.exr", "shot.", ".exr"), Some("0001"));
        assert_eq!(frame_of("shot.12.exr", "shot.", ".exr"), Some("12"));
        assert_eq!(frame_of("shot..exr", "shot.", ".exr"), None);
        assert_eq!(frame_of("shot.12a.exr", "shot.", ".exr"), None);
        assert_eq!(frame_of("other.0001.exr", "shot.", ".exr"), None);
    }
}
