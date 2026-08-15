//! The crate arm of the corpus differential: reads image files through the
//! safe `oiio` API and prints one line per subimage — path, subimage index,
//! dimensions, and an FNV-1a 64 hash of the pixels as `f32` — in exactly the
//! format of `contrib/corpus_hash.cpp`, the standalone C++ arm that uses no
//! part of this crate. Feed both the same file list on stdin and diff.

use oiio::{Error, ImageInput};
use std::io::BufRead;
use std::path::Path;

fn fnv1a64(data: &[u8]) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    for &byte in data {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

fn main() {
    let stdin = std::io::stdin();
    for line in stdin.lock().lines() {
        let Ok(path) = line else { break };
        if path.is_empty() {
            continue;
        }
        let Ok(mut input) = ImageInput::from_path(Path::new(&path)) else {
            println!("{path}\t-\tERROR");
            continue;
        };
        for subimage in 0_u32.. {
            let spec = match input.image_spec_at(subimage, 0) {
                Ok(spec) => spec,
                Err(Error::InvalidImageLevel { .. }) => break,
                Err(_) => {
                    println!("{path}\t{subimage}\tERROR");
                    break;
                }
            };
            if spec.is_deep() {
                println!("{path}\t{subimage}\tDEEP");
                continue;
            }
            let [width, height, _] = spec.dimensions();
            let channels = spec.channel_count();
            let Ok(values) = spec.element_count() else {
                println!("{path}\t{subimage}\tERROR");
                continue;
            };
            // Mirror of the C++ arm's large-file skip.
            if values as u64 > 1 << 28 {
                println!("{path}\t{subimage}\tSKIPPED-LARGE");
                continue;
            }
            let mut pixels = vec![0.0_f32; values];
            match input.read_image_into_at(subimage, 0, &mut pixels) {
                Ok(()) => {
                    let bytes: &[u8] = bytemuck_bytes(&pixels);
                    let hash = fnv1a64(bytes);
                    println!("{path}\t{subimage}\t{width}x{height}x{channels}\t{hash:016x}");
                }
                Err(_) => println!("{path}\t{subimage}\tERROR"),
            }
        }
        let _ = input.close();
    }
}

/// The float buffer viewed as bytes, dependency-free.
fn bytemuck_bytes(pixels: &[f32]) -> &[u8] {
    // SAFETY: f32 has no padding and any byte pattern is readable; the slice
    // covers exactly the same memory.
    unsafe {
        std::slice::from_raw_parts(pixels.as_ptr().cast::<u8>(), std::mem::size_of_val(pixels))
    }
}
