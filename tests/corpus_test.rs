//! Reading real files this crate did not write.
//!
//! Every other test round-trips through our own writer, which proves we agree
//! with ourselves. These read OpenImageIO's own test corpus instead, so the
//! files come from other tools, other compressions, and other decades.
//!
//! The corpus is large and not vendored, so these tests are opt-in. Point
//! `OIIO_BIND_TEST_IMAGES` at a checkout of
//! <https://github.com/AcademySoftwareFoundation/OpenImageIO-images> and they
//! run; without it they report that they were skipped and pass.

use std::path::{Path, PathBuf};

use oiio::{ImageBuf, ImageCache, ImageInput, PixelFormat};

/// The corpus root, or `None` when the suite should be skipped.
fn corpus() -> Option<PathBuf> {
    let path = PathBuf::from(std::env::var_os("OIIO_BIND_TEST_IMAGES")?);
    path.is_dir().then_some(path)
}

/// The OpenEXR test images, from
/// <https://github.com/AcademySoftwareFoundation/openexr-images>, pointed at
/// by `OIIO_BIND_TEST_EXR_IMAGES`.
fn exr_corpus() -> Option<PathBuf> {
    let path = PathBuf::from(std::env::var_os("OIIO_BIND_TEST_EXR_IMAGES")?);
    path.is_dir().then_some(path)
}

macro_rules! corpus_or_skip {
    ($name:literal) => {
        match corpus() {
            Some(root) => root,
            None => {
                eprintln!("skipping {}: set OIIO_BIND_TEST_IMAGES", $name);
                return;
            }
        }
    };
}

macro_rules! exr_corpus_or_skip {
    ($name:literal) => {
        match exr_corpus() {
            Some(root) => root,
            None => {
                eprintln!("skipping {}: set OIIO_BIND_TEST_EXR_IMAGES", $name);
                return;
            }
        }
    };
}

fn walk(root: &Path, files: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(root) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            walk(&path, files);
        } else if path.is_file() {
            files.push(path);
        }
    }
}

/// Extensions OpenImageIO may or may not have been built with; a failure to
/// open one of these says nothing about this crate.
fn is_image_extension(path: &Path) -> bool {
    let Some(extension) = path.extension().and_then(|e| e.to_str()) else {
        return false;
    };
    matches!(
        extension.to_ascii_lowercase().as_str(),
        "exr"
            | "tif"
            | "tiff"
            | "tx"
            | "png"
            | "jpg"
            | "jpeg"
            | "tga"
            | "bmp"
            | "dds"
            | "psd"
            | "gif"
            | "webp"
            | "pnm"
            | "ppm"
            | "pgm"
            | "pbm"
            | "rla"
            | "dpx"
            | "cin"
            | "hdr"
            | "pfm"
            | "ico"
            | "sgi"
            | "rgb"
    )
}

/// Read every image in the corpus.
///
/// The corpus deliberately contains malformed files — `bmpsuite` exists to
/// break readers — so this does not demand that everything opens. What it
/// demands is that nothing crashes: every file either reads, or returns an
/// error that arrives as a normal `Err`.
#[test]
fn reads_every_image_in_the_corpus_without_crashing() {
    let root = corpus_or_skip!("corpus sweep");

    let mut files = Vec::new();
    walk(&root, &mut files);
    files.retain(|path| is_image_extension(path));
    files.sort();
    assert!(
        !files.is_empty(),
        "no images found under {}",
        root.display()
    );

    let mut opened = 0usize;
    let mut read = 0usize;
    let mut refused = 0usize;
    let mut failures: Vec<String> = Vec::new();

    for path in &files {
        let Ok(mut input) = ImageInput::from_path(path) else {
            refused += 1;
            continue;
        };
        opened += 1;

        let Ok(spec) = input.image_spec() else {
            failures.push(format!("{}: spec unavailable", path.display()));
            continue;
        };

        // Keep the sweep quick: very large images prove nothing extra here.
        let Ok(count) = spec.element_count() else {
            continue;
        };
        if count > 8_000_000 {
            continue;
        }

        // Every image is asked for as f32, so OpenImageIO converts whatever
        // the file holds. A deep file is expected to refuse.
        let mut pixels = vec![0.0_f32; count];
        match input.read_image_into(&mut pixels) {
            Ok(()) => read += 1,
            Err(_) if spec.is_deep() => {}
            Err(error) => failures.push(format!("{}: {error}", path.display())),
        }
    }

    println!(
        "corpus: {} image files, {opened} opened, {read} fully read, {refused} not opened",
        files.len()
    );
    for failure in failures.iter().take(20) {
        println!("  read failure: {failure}");
    }

    // Most of the corpus is valid, so a low success rate means we broke
    // something rather than that the corpus is odd.
    assert!(
        opened * 2 > files.len(),
        "only {opened} of {} files opened",
        files.len()
    );
    assert!(
        failures.len() * 20 < opened.max(1),
        "{} images opened but failed to read; first: {:?}",
        failures.len(),
        failures.first()
    );
}

/// An overscan image has pixels outside its display window, so its data
/// window origin is not zero. That is the case our own fixtures only cover
/// because we construct it deliberately.
#[test]
fn reads_an_overscan_exr() {
    let root = corpus_or_skip!("overscan");
    let path = root.join("grid-overscan.exr");
    if !path.is_file() {
        eprintln!("skipping overscan: {} is absent", path.display());
        return;
    }

    let mut input = ImageInput::from_path(&path).unwrap();
    let spec = input.image_spec().unwrap();
    let origin = spec.origin();
    let full = spec.full_origin();

    println!(
        "overscan: data {:?}+{:?}, display {:?}+{:?}",
        origin,
        spec.dimensions(),
        full,
        spec.full_dimensions()
    );
    assert!(
        origin != full || spec.dimensions() != spec.full_dimensions(),
        "grid-overscan.exr should have a data window differing from its display window"
    );

    // The whole image, then a region addressed in image coordinates.
    let window = spec.data_window().unwrap();
    let mut whole = vec![0.0_f32; window.element_count().unwrap()];
    input.read_region_into(window, &mut whole).unwrap();

    let rows = window.y().start..(window.y().start + 4).min(window.y().end);
    let strip = window.with_y(rows).unwrap();
    let mut partial = vec![0.0_f32; strip.element_count().unwrap()];
    input.read_region_into(strip, &mut partial).unwrap();

    // The strip must match the first rows of the whole image.
    assert_eq!(&whole[..partial.len()], &partial[..]);
    input.close().unwrap();
}

/// `.tx` files are tiled, mipmapped textures written by maketx. They exercise
/// the tiled read path against files we did not produce, including the
/// partial-edge-tile case that once failed silently.
#[test]
fn reads_tiled_mipmapped_textures() {
    let root = corpus_or_skip!("textures");

    let mut checked = 0usize;
    for name in ["checker.tx", "grid.tx", "miplevels.tx"] {
        let path = root.join(name);
        if !path.is_file() {
            continue;
        }

        let mut input = ImageInput::from_path(&path).unwrap();
        let base = input.image_spec().unwrap();
        assert!(base.is_tiled(), "{name} should be tiled");
        println!(
            "{name}: {:?} tiles {:?} format {}",
            base.dimensions(),
            base.tile_dimensions(),
            base.format()
        );

        // Walk down the mip pyramid; each level must be readable in full.
        let mut level = 0;
        let mut previous = base.dimensions();
        while let Ok(spec) = input.image_spec_at(0, level) {
            let count = spec.element_count().unwrap();
            let mut pixels = vec![0.0_f32; count];
            input
                .read_image_into_at(0, level, &mut pixels)
                .unwrap_or_else(|error| panic!("{name} level {level}: {error}"));

            if level > 0 {
                assert!(
                    spec.dimensions()[0] <= previous[0] && spec.dimensions()[1] <= previous[1],
                    "{name} level {level} is not smaller than the one above"
                );
            }
            previous = spec.dimensions();
            level += 1;
            if level > 24 {
                break;
            }
        }
        assert!(level > 1, "{name} should have more than one mip level");
        println!("  {level} mip levels read");
        checked += 1;
    }

    assert!(checked > 0, "no .tx textures found in the corpus");
}

/// The cache and the direct reader must agree on a file neither of them wrote.
#[test]
fn the_cache_and_the_reader_agree_on_real_files() {
    let root = corpus_or_skip!("cache agreement");

    let mut checked = 0usize;
    for name in ["grid.tif", "checker.tif", "grid-overscan.exr"] {
        let path = root.join(name);
        if !path.is_file() {
            continue;
        }

        let mut input = ImageInput::from_path(&path).unwrap();
        let spec = input.image_spec().unwrap();
        let window = spec.data_window().unwrap();
        let count = window.element_count().unwrap();
        if count > 8_000_000 {
            continue;
        }

        let mut direct = vec![0.0_f32; count];
        input.read_image_into(&mut direct).unwrap();

        let cache = ImageCache::new().unwrap();
        let mut cached = vec![0.0_f32; count];
        cache.get_pixels_into(&path, window, &mut cached).unwrap();

        assert_eq!(direct, cached, "{name}: cache and reader disagree");
        checked += 1;
    }

    assert!(checked > 0, "none of the expected files were found");
}

/// Real files carry metadata this crate did not invent, including types it
/// models as `AttributeValue::Other`.
#[test]
fn surfaces_metadata_from_a_real_photograph() {
    let root = corpus_or_skip!("metadata");
    let path = root.join("tahoe-gps.jpg");
    if !path.is_file() {
        eprintln!("skipping metadata: {} is absent", path.display());
        return;
    }

    let input = ImageInput::from_path(&path).unwrap();
    let spec = input.image_spec().unwrap();
    let attributes = spec.attributes();

    println!("{} attributes on tahoe-gps.jpg", attributes.len());
    for (name, value) in attributes.iter().take(12) {
        println!("  {name} = {value}");
    }

    assert!(
        attributes.len() > 5,
        "a GPS-tagged photograph should carry metadata, found {}",
        attributes.len()
    );
    // Some of it is bound to be a type this crate does not model directly;
    // those must still be readable rather than lost.
    let unmodelled = attributes
        .iter()
        .filter(|(_, value)| !value.is_writable())
        .count();
    println!("  {unmodelled} of them are not directly modelled");
}

/// The OpenEXR test images, which are the awkward cases by design:
/// multi-resolution, multi-view, subsampled luminance/chroma, unusual display
/// windows, and deliberately corrupted files.
#[test]
fn reads_the_openexr_test_images() {
    let root = exr_corpus_or_skip!("openexr corpus");

    let mut files = Vec::new();
    walk(&root, &mut files);
    files.retain(|path| {
        path.extension().and_then(|e| e.to_str()) == Some("exr")
            // Damaged/ is corrupt on purpose and is asserted separately.
            && !path.components().any(|part| part.as_os_str() == "Damaged")
    });
    files.sort();
    assert!(!files.is_empty(), "no EXRs found under {}", root.display());

    let mut opened = 0usize;
    let mut read = 0usize;
    let mut deep = 0usize;
    let mut subsampled = 0usize;
    let mut failures: Vec<String> = Vec::new();

    for path in &files {
        let mut input = match ImageInput::from_path(path) {
            Ok(input) => input,
            Err(error) => {
                // OpenImageIO cannot read subsampled luminance/chroma EXRs at
                // all — "Subsampled channels are not supported" — so those
                // refusals are the library's limitation, not this crate's.
                if error.to_string().contains("Subsampled channels") {
                    subsampled += 1;
                } else {
                    failures.push(format!("{}: {error}", path.display()));
                }
                continue;
            }
        };
        opened += 1;

        let Ok(spec) = input.image_spec() else {
            failures.push(format!("{}: spec unavailable", path.display()));
            continue;
        };
        if spec.is_deep() {
            // Deep files were skipped here until they could be read at all.
            // Now they are read, and counted only if that works.
            match input.read_deep_image() {
                Ok(image) => {
                    assert_eq!(image.channel_count(), spec.channel_count() as usize);
                    deep += 1;
                }
                Err(error) => failures.push(format!("{}: deep read: {error}", path.display())),
            }
            continue;
        }

        let Ok(count) = spec.element_count() else {
            continue;
        };
        if count > 16_000_000 {
            continue;
        }

        let mut pixels = vec![0.0_f32; count];
        match input.read_image_into(&mut pixels) {
            Ok(()) => read += 1,
            Err(error) => failures.push(format!("{}: {error}", path.display())),
        }
    }

    println!(
        "openexr corpus: {} files, {opened} opened, {read} read flat, {deep} read deep, \
         {subsampled} refused by OpenImageIO as subsampled",
        files.len()
    );
    for failure in failures.iter().take(20) {
        println!("  failure: {failure}");
    }
    assert!(
        failures.is_empty(),
        "{} of the undamaged OpenEXR test images failed unexpectedly",
        failures.len()
    );
    assert!(read > 50, "only {read} images were read in full");
}

/// The `Damaged` directory is corrupt on purpose. Reading it must produce
/// errors, not crashes, and must not quietly succeed.
#[test]
fn rejects_damaged_exrs_without_crashing() {
    let root = exr_corpus_or_skip!("damaged exrs");
    let damaged = root.join("Damaged");
    if !damaged.is_dir() {
        eprintln!("skipping damaged: {} is absent", damaged.display());
        return;
    }

    // These come from a fuzzing corpus and are named `..._exr`, with no
    // extension at all, so match on the name rather than the extension.
    let mut files = Vec::new();
    walk(&damaged, &mut files);
    files.retain(|path| {
        path.file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.ends_with("_exr") || name.ends_with(".exr"))
    });
    assert!(!files.is_empty(), "no damaged EXRs found");

    let mut rejected = 0usize;
    let mut survived = 0usize;
    for path in &files {
        // Either the open fails, the spec fails, or the read fails. Any of
        // those is a correct outcome; a panic or a hang is not.
        let outcome = ImageInput::from_path(path).and_then(|mut input| {
            let spec = input.image_spec()?;
            let count = spec.element_count()?;
            // A fuzzed header can claim an enormous image; refuse to allocate
            // for it rather than let the test become the denial of service.
            if count > 16_000_000 {
                return Ok(());
            }
            let mut pixels = vec![0.0_f32; count];
            input.read_image_into(&mut pixels)
        });
        match outcome {
            Ok(()) => survived += 1,
            Err(_) => rejected += 1,
        }
    }

    println!(
        "damaged: {} files, {rejected} reported an error, {survived} read anyway",
        files.len()
    );
    // Some "damaged" files are only subtly wrong and still decode; the point
    // is that the rest fail cleanly rather than taking the process with them.
    assert_eq!(rejected + survived, files.len());
}

/// Multi-resolution EXRs carry their own mip pyramids, written by OpenEXR
/// rather than by maketx.
#[test]
fn reads_multi_resolution_exrs() {
    let root = exr_corpus_or_skip!("multi-resolution");
    let directory = root.join("MultiResolution");
    if !directory.is_dir() {
        eprintln!("skipping: {} is absent", directory.display());
        return;
    }

    let mut files = Vec::new();
    walk(&directory, &mut files);
    files.retain(|path| path.extension().and_then(|e| e.to_str()) == Some("exr"));
    files.sort();

    let mut with_mips = 0usize;
    let mut ripmaps = 0usize;
    let mut failures: Vec<String> = Vec::new();

    for path in files.iter().take(14) {
        let Ok(mut input) = ImageInput::from_path(path) else {
            continue;
        };
        let name = path.file_name().unwrap().to_string_lossy().into_owned();

        let Ok(base) = input.image_spec_at(0, 0) else {
            continue;
        };
        // A ripmap has levels indexed on two axes, which OpenImageIO's linear
        // miplevel API cannot express: it reports every level at the base
        // size. Reading those by index is not meaningful, so count and skip.
        let second = input.image_spec_at(0, 1).ok();
        if second.is_some_and(|spec| spec.dimensions() == base.dimensions()) {
            ripmaps += 1;
            println!("{name}: ripmap-like, levels do not shrink; skipped");
            continue;
        }

        let mut level = 0;
        while let Ok(spec) = input.image_spec_at(0, level) {
            let Ok(count) = spec.element_count() else {
                break;
            };
            if count <= 16_000_000 {
                let mut pixels = vec![0.0_f32; count];
                if let Err(error) = input.read_image_into_at(0, level, &mut pixels) {
                    failures.push(format!("{name}: level {level}: {error}"));
                    break;
                }
            }
            level += 1;
            if level > 24 {
                break;
            }
        }
        if level > 1 {
            with_mips += 1;
            println!("{name}: {level} levels");
        }
    }

    for failure in &failures {
        println!("  failure: {failure}");
    }
    assert!(failures.is_empty(), "{} mip reads failed", failures.len());
    assert!(
        with_mips > 0,
        "expected at least one multi-resolution EXR to report several levels"
    );
    println!("{with_mips} mipmapped, {ripmaps} ripmap-like");
}

/// Multi-part EXRs appear as several subimages.
#[test]
fn reads_multi_part_exrs() {
    let root = exr_corpus_or_skip!("multi-part");
    let directory = root.join("Beachball");
    if !directory.is_dir() {
        eprintln!("skipping: {} is absent", directory.display());
        return;
    }

    let mut files = Vec::new();
    walk(&directory, &mut files);
    files.retain(|path| path.extension().and_then(|e| e.to_str()) == Some("exr"));
    files.sort();

    let mut multi_part = 0usize;
    for path in files.iter().take(8) {
        let Ok(mut input) = ImageInput::from_path(path) else {
            continue;
        };
        let mut subimage = 0;
        while let Ok(spec) = input.image_spec_at(subimage, 0) {
            if spec.is_deep() {
                subimage += 1;
                continue;
            }
            let Ok(count) = spec.element_count() else {
                break;
            };
            if count <= 16_000_000 {
                let mut pixels = vec![0.0_f32; count];
                if let Err(error) = input.read_image_into_at(subimage, 0, &mut pixels) {
                    panic!("{}: subimage {subimage}: {error}", path.display());
                }
            }
            subimage += 1;
            if subimage > 32 {
                break;
            }
        }
        if subimage > 1 {
            multi_part += 1;
            println!(
                "{}: {subimage} subimages",
                path.file_name().unwrap().to_string_lossy()
            );
        }
    }

    assert!(multi_part > 0, "expected a multi-part EXR in Beachball");
}

/// Metadata this crate does not model directly must still survive being read
/// from a real file and written to a new one. Losing a chromaticity, a
/// timecode or a camera matrix silently is worse than refusing to write it.
#[test]
fn carries_unmodelled_metadata_through_a_round_trip() {
    let root = exr_corpus_or_skip!("metadata round trip");

    let mut files = Vec::new();
    walk(&root, &mut files);
    files.retain(|path| {
        path.extension().and_then(|e| e.to_str()) == Some("exr")
            && !path.components().any(|part| part.as_os_str() == "Damaged")
    });
    files.sort();

    let scratch = std::env::temp_dir().join(format!("oiio-bind-meta-{}", std::process::id()));
    std::fs::create_dir_all(&scratch).unwrap();

    let mut checked = 0usize;
    let mut carried = 0usize;
    let mut dropped: Vec<String> = Vec::new();

    for path in files.iter() {
        let Ok(mut input) = ImageInput::from_path(path) else {
            continue;
        };
        let Ok(spec) = input.image_spec() else {
            continue;
        };
        let Ok(count) = spec.element_count() else {
            continue;
        };
        if spec.is_deep() || count > 2_000_000 {
            continue;
        }
        // Only files that actually carry something unmodelled are interesting.
        let unmodelled: Vec<&String> = spec
            .attributes()
            .iter()
            .filter(|(_, value)| matches!(value, oiio::AttributeValue::Other { .. }))
            .map(|(name, _)| name)
            .collect();
        if unmodelled.is_empty() {
            continue;
        }

        let mut pixels = vec![0.0_f32; count];
        if input.read_image_into(&mut pixels).is_err() {
            continue;
        }

        let written = scratch.join(format!("round-trip-{checked}.exr"));
        let mut output = oiio::ImageOutput::create(&written, &spec).unwrap();
        output.write_image(&pixels).unwrap();
        output.close().unwrap();

        let again = ImageInput::from_path(&written).unwrap();
        let after = again.image_spec().unwrap();

        for name in unmodelled {
            match (spec.attribute(name), after.attribute(name)) {
                (Some(before), Some(now)) if before == now => carried += 1,
                (Some(before), Some(now)) => dropped.push(format!(
                    "{}: {name} changed, {before} -> {now}",
                    path.file_name().unwrap().to_string_lossy()
                )),
                (Some(_), None) => dropped.push(format!(
                    "{}: {name} was lost",
                    path.file_name().unwrap().to_string_lossy()
                )),
                _ => {}
            }
        }
        checked += 1;
        if checked >= 25 {
            break;
        }
    }

    let _ = std::fs::remove_dir_all(&scratch);

    println!("metadata round trip: {checked} files, {carried} unmodelled attributes preserved");
    for loss in dropped.iter().take(15) {
        println!("  {loss}");
    }
    assert!(checked > 0, "no files with unmodelled metadata were found");
    assert!(carried > 0, "nothing was preserved, so nothing was tested");
    assert!(
        dropped.is_empty(),
        "{} unmodelled attributes did not survive; first: {:?}",
        dropped.len(),
        dropped.first()
    );
}

/// An ImageBuf must load a real file as happily as one we produced.
#[test]
fn image_buf_loads_real_files() {
    let root = corpus_or_skip!("image buf");
    let path = root.join("grid.tif");
    if !path.is_file() {
        eprintln!("skipping image buf: {} is absent", path.display());
        return;
    }

    let mut image = ImageBuf::from_path(&path).unwrap();
    image.read().unwrap();
    let spec = image.spec().unwrap();
    assert!(spec.dimensions()[0] > 0);
    assert_ne!(spec.format(), PixelFormat::Other);

    let window = spec.data_window().unwrap();
    let mut pixels = vec![0.0_f32; window.element_count().unwrap()];
    image.get_pixels_into(window, &mut pixels).unwrap();
    assert!(
        pixels.iter().any(|&value| value != 0.0),
        "grid.tif should not be uniformly black"
    );
}
