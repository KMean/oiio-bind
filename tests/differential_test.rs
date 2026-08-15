//! The binding compared against the C++ API it wraps, byte for byte.
//!
//! Every test here runs the same operation twice — once through the safe
//! crate, once through the raw `oiio-sys` bridge, whose read shims are
//! plain pass-throughs to the C++ entry points — and requires the results
//! to be identical to the last byte. Both sides run the same linked
//! OpenImageIO in the same process, so any difference is the wrapper
//! layer's: region plumbing, stride math, conversion glue. That is exactly
//! the layer this crate adds, and exactly where its bugs would live.
//!
//! Generated fixtures run everywhere; the OpenImageIO and OpenEXR corpora
//! add the awkward real-world geometry (overscan windows at negative
//! origins, partial edge tiles, mip levels smaller than their own tiles)
//! when `OIIO_BIND_TEST_IMAGES` / `OIIO_BIND_TEST_EXR_IMAGES` are set.

mod common;

use common::{f32_ramp, write_image, ScratchDir};
use oiio::{f16, ImageInput, ImageSpec, Pixel, PixelFormat};
use oiio_sys::imageio as sysio;
use oiio_sys::typedesc::{typedesc_from_basetype_arraylen, BaseType};
use std::path::{Path, PathBuf};

fn oiio_corpus() -> Option<PathBuf> {
    let path = PathBuf::from(std::env::var_os("OIIO_BIND_TEST_IMAGES")?);
    path.is_dir().then_some(path)
}

fn exr_corpus() -> Option<PathBuf> {
    let path = PathBuf::from(std::env::var_os("OIIO_BIND_TEST_EXR_IMAGES")?);
    path.is_dir().then_some(path)
}

/// An initialized scalar slice viewed as its bytes, for exact comparison.
fn as_bytes<T: Pixel>(values: &[T]) -> &[u8] {
    // SAFETY: Pixel is sealed to plain scalar layouts; same-process,
    // same-endianness comparison of initialized memory.
    unsafe {
        std::slice::from_raw_parts(values.as_ptr().cast::<u8>(), std::mem::size_of_val(values))
    }
}

/// The whole image read through the raw C++ API in `base` format.
/// Returns (width, height, channels, bytes).
fn sys_read_whole(
    path: &Path,
    subimage: i32,
    miplevel: i32,
    base: BaseType,
    bytes_per_value: usize,
) -> (usize, usize, usize, Vec<u8>) {
    let name = path.to_str().expect("corpus paths are UTF-8");
    let mut input = sysio::imageinput_open_without_config(name)
        .unwrap_or_else(|error| panic!("open {name}: {error}"));
    let (width, height, channels) = {
        let spec = sysio::imageinput_spec(input.as_ref().unwrap());
        // The requested level's own dimensions, when past the base.
        if subimage != 0 || miplevel != 0 {
            let level = sysio::imageinput_spec_dimensions(input.pin_mut(), subimage, miplevel);
            let level = level.as_ref().expect("level spec");
            (
                sysio::imagespec_width(level) as usize,
                sysio::imagespec_height(level) as usize,
                sysio::imagespec_nchannels(level) as usize,
            )
        } else {
            (
                sysio::imagespec_width(spec) as usize,
                sysio::imagespec_height(spec) as usize,
                sysio::imagespec_nchannels(spec) as usize,
            )
        }
    };
    let mut data = vec![0_u8; width * height * channels * bytes_per_value];
    let format = typedesc_from_basetype_arraylen(base, 0);
    // SAFETY: the buffer measures exactly the requested image extent in the
    // requested format.
    let read = unsafe {
        sysio::imageinput_read_image_span(
            input.pin_mut(),
            subimage,
            miplevel,
            0,
            channels as i32,
            format,
            &mut data,
        )
    };
    assert!(read, "raw read of {name} failed");
    assert!(sysio::imageinput_close(input.pin_mut()));
    (width, height, channels, data)
}

/// The whole image read through the safe crate as `T`.
fn safe_read_whole<T: Pixel>(path: &Path, subimage: u32, miplevel: u32) -> Vec<T> {
    let mut input = ImageInput::from_path(path).unwrap();
    let spec = if subimage == 0 && miplevel == 0 {
        input.image_spec().unwrap()
    } else {
        input.image_spec_at(subimage, miplevel).unwrap()
    };
    let mut pixels = vec![T::default(); spec.element_count().unwrap()];
    if subimage == 0 && miplevel == 0 {
        input.read_image_into(&mut pixels).unwrap();
    } else {
        input
            .read_image_into_at(subimage, miplevel, &mut pixels)
            .unwrap();
    }
    input.close().unwrap();
    pixels
}

/// Both sides read the file whole in its native scalar type; byte-equal.
fn assert_whole_image_matches<T: Pixel>(path: &Path, base: BaseType) {
    let safe = safe_read_whole::<T>(path, 0, 0);
    let (_, _, _, raw) = sys_read_whole(path, 0, 0, base, std::mem::size_of::<T>());
    assert_eq!(
        as_bytes(&safe),
        &raw[..],
        "safe and raw reads of {} disagree",
        path.display()
    );
}

#[test]
fn whole_image_decodes_match_the_raw_api() {
    // Generated fixtures, so this differential always runs.
    let scratch = ScratchDir::new("diffwhole");
    let exr = scratch.file("ramp.exr");
    let spec = ImageSpec::new(33, 17, 3, PixelFormat::F32).unwrap();
    write_image(&exr, &spec, &f32_ramp(spec.element_count().unwrap())).unwrap();
    assert_whole_image_matches::<f32>(&exr, BaseType::Float32);

    let png = scratch.file("ramp.png");
    let spec = ImageSpec::new(21, 9, 3, PixelFormat::U8).unwrap();
    let bytes: Vec<u8> = (0..spec.element_count().unwrap())
        .map(|index| (index % 251) as u8)
        .collect();
    write_image(&png, &spec, &bytes).unwrap();
    assert_whole_image_matches::<u8>(&png, BaseType::UInt8);

    // The corpus adds real encoders' output.
    if let Some(root) = exr_corpus() {
        let desk = root.join("ScanLines/Desk.exr");
        if desk.is_file() {
            assert_whole_image_matches::<f16>(&desk, BaseType::Float16);
        }
    }
    if let Some(root) = oiio_corpus() {
        for name in ["png/oiio-logo-with-alpha.png", "tahoe-gps.jpg"] {
            let path = root.join(name);
            if path.is_file() {
                assert_whole_image_matches::<u8>(&path, BaseType::UInt8);
            }
        }
    }
}

#[test]
fn conversion_on_read_matches_the_raw_api() {
    let scratch = ScratchDir::new("diffconvert");

    // A uint8 file read as f32: the conversion is the library's, and both
    // sides must apply the identical one.
    let png = scratch.file("ramp.png");
    let spec = ImageSpec::new(19, 7, 3, PixelFormat::U8).unwrap();
    let bytes: Vec<u8> = (0..spec.element_count().unwrap())
        .map(|index| (index % 251) as u8)
        .collect();
    write_image(&png, &spec, &bytes).unwrap();
    let safe = safe_read_whole::<f32>(&png, 0, 0);
    let (_, _, _, raw) = sys_read_whole(&png, 0, 0, BaseType::Float32, 4);
    assert_eq!(as_bytes(&safe), &raw[..]);

    // A half file read as u8: narrowing with round-and-clamp.
    let exr = scratch.file("ramp.exr");
    let spec = ImageSpec::new(23, 11, 3, PixelFormat::F16).unwrap();
    let halves: Vec<f16> = (0..spec.element_count().unwrap())
        .map(|index| f16::from_f32((index % 200) as f32 / 200.0))
        .collect();
    write_image(&exr, &spec, &halves).unwrap();
    let safe = safe_read_whole::<u8>(&exr, 0, 0);
    let (_, _, _, raw) = sys_read_whole(&exr, 0, 0, BaseType::UInt8, 1);
    assert_eq!(as_bytes(&safe), &raw[..]);

    if let Some(root) = oiio_corpus() {
        let mips = root.join("miplevels.tx");
        if mips.is_file() {
            let safe = safe_read_whole::<f32>(&mips, 0, 0);
            let (_, _, _, raw) = sys_read_whole(&mips, 0, 0, BaseType::Float32, 4);
            assert_eq!(as_bytes(&safe), &raw[..]);
        }
    }
}

/// Safe chunked scanline reads, concatenated, equal the raw whole-image
/// read — including across a data window at a negative origin.
fn assert_chunked_scanlines_match(path: &Path) {
    let mut input = ImageInput::from_path(path).unwrap();
    let spec = input.image_spec().unwrap();
    let window = spec.data_window().unwrap();
    let (y_begin, y_end) = (window.y().start, window.y().end);

    let mut concatenated: Vec<u8> = Vec::new();
    let mut y = y_begin;
    while y < y_end {
        let chunk_end = (y + 64).min(y_end);
        let roi = window.with_y(y..chunk_end).unwrap();
        let mut chunk = vec![f16::ZERO; roi.element_count().unwrap()];
        input.read_region_into(roi, &mut chunk).unwrap();
        concatenated.extend_from_slice(as_bytes(&chunk));
        y = chunk_end;
    }
    input.close().unwrap();

    let (_, _, _, raw) = sys_read_whole(path, 0, 0, BaseType::Float16, 2);
    assert_eq!(
        concatenated,
        raw,
        "chunked safe reads disagree with the raw whole image for {}",
        path.display()
    );
}

#[test]
fn chunked_scanline_reads_match_the_raw_whole_image() {
    let scratch = ScratchDir::new("diffchunks");
    let exr = scratch.file("tall.exr");
    let spec = ImageSpec::new(31, 150, 3, PixelFormat::F16).unwrap();
    let halves: Vec<f16> = (0..spec.element_count().unwrap())
        .map(|index| f16::from_f32((index % 977) as f32 / 977.0))
        .collect();
    write_image(&exr, &spec, &halves).unwrap();
    assert_chunked_scanlines_match(&exr);

    if let Some(root) = oiio_corpus() {
        // Data window at -250,-250 inside a 1000x1000 display window: the
        // negative-origin geometry every overscan render has.
        let overscan = root.join("grid-overscan.exr");
        if overscan.is_file() {
            assert_chunked_scanlines_match(&overscan);
        }
    }
    if let Some(root) = exr_corpus() {
        let desk = root.join("ScanLines/Desk.exr");
        if desk.is_file() {
            assert_chunked_scanlines_match(&desk);
        }
    }
}

#[test]
fn tile_blocks_match_the_raw_tile_reads() {
    let Some(root) = exr_corpus() else {
        eprintln!("skipping: set OIIO_BIND_TEST_EXR_IMAGES for the tiled corpus case");
        return;
    };
    let golden = root.join("Tiles/GoldenGate.exr");
    if !golden.is_file() {
        eprintln!("skipping: no Tiles/GoldenGate.exr in the corpus");
        return;
    }

    // 1262x860 with 128px tiles: 1262 and 860 are not tile multiples, so
    // the second block ends on the ragged right/bottom edge.
    let name = golden.to_str().unwrap();
    let mut input = ImageInput::from_path(&golden).unwrap();
    let spec = input.image_spec().unwrap();
    let window = spec.data_window().unwrap();
    let channels = spec.channel_count() as usize;

    for (x0, x1, y0, y1) in [(0, 256, 0, 256), (1152, 1262, 768, 860)] {
        let roi = window.with_x(x0..x1).unwrap().with_y(y0..y1).unwrap();
        let mut safe = vec![f16::ZERO; roi.element_count().unwrap()];
        input.read_region_into(roi, &mut safe).unwrap();

        let mut raw = vec![0_u8; ((x1 - x0) as usize) * ((y1 - y0) as usize) * channels * 2];
        let mut sys_input = sysio::imageinput_open_without_config(name).unwrap();
        let format = typedesc_from_basetype_arraylen(BaseType::Float16, 0);
        // SAFETY: the buffer measures exactly the requested block.
        let read = unsafe {
            sysio::imageinput_read_tiles_span(
                sys_input.pin_mut(),
                0,
                0,
                x0,
                x1,
                y0,
                y1,
                0,
                1,
                0,
                channels as i32,
                format,
                &mut raw,
            )
        };
        assert!(
            read,
            "raw tile read failed for block {x0}..{x1} x {y0}..{y1}"
        );
        assert!(sysio::imageinput_close(sys_input.pin_mut()));
        assert_eq!(
            as_bytes(&safe),
            &raw[..],
            "block {x0}..{x1} x {y0}..{y1} disagrees"
        );
    }
    input.close().unwrap();
}

#[test]
fn every_mip_level_matches_the_raw_api() {
    let Some(root) = oiio_corpus() else {
        eprintln!("skipping: set OIIO_BIND_TEST_IMAGES for the mip corpus case");
        return;
    };
    let mips = root.join("miplevels.tx");
    if !mips.is_file() {
        eprintln!("skipping: no miplevels.tx in the corpus");
        return;
    }

    // Eleven levels, 1024^2 down to 1x1; the small ones are smaller than
    // their own 64px tiles, which is the geometry corner worth the sweep.
    for level in 0..11 {
        let safe = safe_read_whole::<u8>(&mips, 0, level);
        let (width, height, _, raw) = sys_read_whole(&mips, 0, level as i32, BaseType::UInt8, 1);
        assert_eq!(
            as_bytes(&safe),
            &raw[..],
            "mip level {level} ({width}x{height}) disagrees"
        );
    }
}
