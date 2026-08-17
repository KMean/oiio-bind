# oiio-bind

High-level bindings for
[OpenImageIO](https://github.com/AcademySoftwareFoundation/OpenImageIO)

## Introduction

The `src` directory contains the [oiio](https://crates.io/crates/oiio) crate.
The `oiio` crate provides a high-level safe API over the low-level `oiio-sys` crate.

The `oiio-sys` directory contains the [oiio-sys](https://crates.io/crates/oiio-sys/) crate.
`oiio-sys` uses [cxx](https://cxx.rs) to wrap the C++ OpenImageIO API.

The `oiio-sys` crate should not be used directly.

See [ROADMAP.md](ROADMAP.md) for the compatibility policy and planned safe API,
including first-class `ImageCache` support.


## Usage

Building `oiio-sys` requires an OpenImageIO 3.1.4 or newer 3.1.x development installation,
including the C++ headers and libraries. The bindings currently target the
OpenImageIO 3.1 API.

Add the crate from crates.io:

```toml
[dependencies]
oiio = "0.1"
```

Read an image through a private, thread-safe cache:

```rust,no_run
use oiio::ImageCache;
use std::path::Path;

let cache = ImageCache::new()?;
let path = Path::new("image.exr");
let spec = cache.image_spec(path)?;
let roi = spec.data_window()?;
let mut pixels = vec![0.0_f32; roi.element_count()?];
cache.get_pixels_into(path, roi, &mut pixels)?;
# Ok::<(), oiio::Error>(())
```

Read part of one — a region is a `Roi`, so a channel subset is a narrowed data
window:

```rust,no_run
use oiio::ImageInput;
use std::path::Path;

let mut input = ImageInput::from_path(Path::new("image.exr"))?;
let spec = input.image_spec()?;

// The first three channels of the top 64 scanlines.
let roi = spec.data_window()?.with_y(0..64)?.with_channels(0..3)?;
let mut pixels = vec![0.0_f32; roi.element_count()?];
input.read_region_into(roi, &mut pixels)?;
# Ok::<(), oiio::Error>(())
```

How the region may be shaped follows what OpenImageIO can address: a tiled
image is read tile by tile, so each axis must sit on the tile grid or end at
the data window edge; a scanline image is read a row at a time, so the x range
must cover the full width. For arbitrary regions of a tiled file, the
`ImageCache` assembles them from tiles.

Write one:

```rust,no_run
use oiio::{f16, ImageOutput, ImageSpec, PixelFormat};
use std::path::Path;

let spec = ImageSpec::new(1920, 1080, 4, PixelFormat::F16)?
    .with_channel_names(["R", "G", "B", "A"])?
    .with_attribute("Artist", "oiio-bind");
let pixels = vec![f16::ZERO; spec.element_count()?];

let mut output = ImageOutput::create(Path::new("out.exr"), &spec)?;
output.write_image(&pixels)?;
output.close()?;
# Ok::<(), oiio::Error>(())
```

Neither side needs the filesystem. `ImageOutput::to_memory` encodes into a
buffer you take with `close_into_bytes`, and `ImageInput::from_memory` decodes
bytes you already hold — useful for images arriving over a network or held in a
cache:

```rust,no_run
use oiio::{ImageInput, ImageOutput, ImageSpec, PixelFormat};

let spec = ImageSpec::new(64, 64, 3, PixelFormat::F32)?;
let pixels = vec![0.5_f32; spec.element_count()?];

let mut output = ImageOutput::to_memory("image.exr", &spec)?;
output.write_image(&pixels)?;
let encoded: Vec<u8> = output.close_into_bytes()?;

let mut input = ImageInput::from_memory("image.exr", encoded)?;
let mut decoded = vec![0.0_f32; spec.element_count()?];
input.read_image_into(&mut decoded)?;
# Ok::<(), oiio::Error>(())
```

The file name is never opened in either case; it only selects the format.

A writer is always open: `ImageOutput::create` selects the plugin from the file
name and opens the file in one step, so a write can never be attempted against
an unopened file. Whole images, scanline ranges, and blocks of tiles are all
supported, along with subimages, mip levels, and a non-zero data window origin.
Every write region and buffer length is validated before any call into C++.

The contiguous safe APIs support `u8`, `u16`, [`half::f16`](https://docs.rs/half),
and `f32` pixel components. A sealed `Pixel` trait keeps the Rust element type,
OpenImageIO type descriptor, alignment, and buffer byte size consistent.
`PixelFormat` separately describes what a file stores, including formats such as
`uint32` that the contiguous buffer API does not itself read or write.

Deep images, where each pixel holds a list of samples rather than one value,
have their own type, because a fixed number of values per pixel cannot
describe them. `DeepImage` is both what a read returns and what a write
takes:

```rust,no_run
use oiio::ImageInput;
use std::path::Path;

let mut input = ImageInput::from_path(Path::new("deep.exr"))?;
let deep = input.read_deep_image()?;
let z = deep.z_channel().expect("a deep render carries depth");

for sample in 0..deep.sample_count(100, 50)? {
    println!("sample {sample} at depth {}", deep.value(100, 50, z, sample)?);
}
# Ok::<(), oiio::Error>(())
```

`ImageBuf` holds an image in memory and `oiio::algo` operates on it, mirroring
OpenImageIO's `ImageBufAlgo`:

```rust,no_run
use oiio::{algo, ImageBuf, ImageSpec, PixelFormat};

let spec = ImageSpec::new(1920, 1080, 3, PixelFormat::F32)?;
let mut background = ImageBuf::new(&spec)?;
algo::fill(&mut background, &[0.2, 0.4, 0.6], None)?;

let mut brighter = ImageBuf::new(&spec)?;
algo::mul_constant(&mut brighter, &background, &[2.0, 2.0, 2.0], None)?;

// How far apart are two images?
let difference = algo::compare(&background, &brighter, 0.0, 0.0, None)?;
println!("largest difference: {}", difference.max_error);
# Ok::<(), oiio::Error>(())
```

The destination is `&mut` and the sources are `&`, so Rust rejects aliasing
these operations are not written for. Pass `ImageBuf::empty()` when the
operation should decide the result's shape — `transpose` swaps the dimensions
and `copy` can change the pixel format, neither of which happens if you hand
them a buffer you already sized.

Textures are made and then looked up. `make_texture` writes the tiled,
MIP-mapped file a renderer wants, and `TextureSystem` does the filtered
lookups into it, choosing a mip level and filter width from the derivatives
you supply:

```rust,no_run
use oiio::{
    make_texture, Derivatives, TextureConfig, TextureMode, TextureOptions,
    TextureSystem, WrapMode,
};
use std::path::Path;

make_texture(
    TextureMode::Texture,
    Path::new("source.exr"),
    Path::new("surface.tx"),
    &TextureConfig::new()
        .with_wrap_modes(WrapMode::Periodic, WrapMode::Periodic)
        .with_filter("lanczos3"),
)?;

let textures = TextureSystem::new()?;
let [width, _] = textures.resolution(Path::new("surface.tx"))?;

let mut rgb = [0.0_f32; 3];
textures.texture(
    Path::new("surface.tx"),
    &TextureOptions::default(),
    0.5,
    0.5,
    // One screen pixel covers one texel, so this reads the finest level.
    Derivatives::uniform(1.0 / width as f32),
    &mut rgb,
)?;
# Ok::<(), oiio::Error>(())
```

Note that the output format has the last word on a texture's data format, and
takes it silently: a `.tx` is a TIFF, which promotes a request for `half` to
`float`, and OpenEXR demotes integer formats to `half`.

Metadata is read as `AttributeValue`. Integers, floats, strings and string
arrays are modelled directly; every other OpenImageIO type — `float2`,
`timecode`, chromaticities, an ICC profile — is carried as its type name, its
printed form, and the bytes OpenImageIO stored. Writing uses those bytes, so
an attribute this crate does not understand still survives being read from one
image and written to another rather than being silently dropped.

The `oiio` crate is built using `cargo build`. `oiio-sys` discovers
OpenImageIO in this order:

1. An explicit `OIIO_ROOT`, or both `OIIO_INCLUDE_DIR` and
   `OIIO_LIBRARY_DIR`. `OIIO_DLL_DIR` may specify a separate Windows runtime
   directory.
2. vcpkg when targeting Windows with the MSVC Rust toolchain.
3. `pkg-config` on other targets.

`OIIO_ROOT` must contain `include/OpenImageIO` and `lib`. The separate include
and library overrides are useful when a custom installation does not follow
that layout.

On Windows, the default vcpkg triplet is derived from the Rust target:
`x64-windows`, `x86-windows`, or `arm64-windows`. These are dynamic-library
triplets. Set `VCPKG_ROOT` to the vcpkg checkout and install the matching port,
for example:

```powershell
vcpkg install openimageio:x64-windows
cargo build --all
```

Set `OIIO_VCPKG_TRIPLET` to select another triplet. If it is unset,
`VCPKG_DEFAULT_TRIPLET` is honored before the target-derived default.

On Linux, macOS, and other Unix-like systems, `pkg-config` must be able to
locate `OpenImageIO`. For a non-system installation, set `PKG_CONFIG_PATH` to
the directory containing its `OpenImageIO.pc` file.

### Troubleshooting

If discovery fails, first check that the OpenImageIO installation matches the
Rust target architecture and ABI. On Windows, verify `VCPKG_ROOT` and the
selected triplet. With explicit overrides, `OIIO_INCLUDE_DIR` must contain the
`OpenImageIO` directory and `OIIO_LIBRARY_DIR` must contain the OpenImageIO
libraries.

Dynamic Windows builds also need `OpenImageIO.dll`, `OpenImageIO_Util.dll`,
and their dependency DLLs at runtime. vcpkg discovery stages these for Cargo
commands. Explicit installs stage DLLs from `OIIO_DLL_DIR`, or from
`OIIO_ROOT/bin` when present. Applications launched or distributed outside
Cargo must still place the required DLLs beside the executable or make their
directory available through `PATH`.


## See it working

Two small applications in this repository are built entirely on the crate's
public API and double as living examples.

**`oiiox`**, an `oiiotool`-flavoured command chain, ships inside the crate
as an example — no extra dependencies:

```bash
cargo run --release --example oiiox -- -i input.exr --info --stats --resize 512x512 --colorconvert lin_srgb srgb --o out.png
```

```bash
cargo run --release --example oiiox -- -i a.exr -i b.exr --diff
```

A `#` or `%04d` in a path runs the chain once per frame, over the frames
found on disk — a colour-managed proxy sequence is one line:

```bash
cargo run --release --example oiiox -- -i shot.#.exr --colorconvert lin_srgb srgb --o preview.#.jpg
```

`--help` lists the whole chain language: reading and writing, `--info`,
`--stats`, `--resize`, `--flip`/`--flop`, `--premult`/`--unpremult`,
`--colorconvert`, channel shuffles with `--ch R,G,B,A=1.0`, `--frames`,
and `--diff`, which exits non-zero when images differ — usable directly
in CI.

**`oiio-viewer`**, a small review-tool-shaped example built on `egui`,
lives in [`viewer/`](viewer/) as its own crate so the library keeps its
dependency graph free of windowing:

```bash
cd viewer
```

```bash
cargo run --release -- path/to/shots/
```

Open files, a directory, or a `shot.#.exr` pattern as a sequence and it
plays: Space starts playback at an adjustable rate, a scrubber and the
arrow keys move through frames, the scroll wheel zooms about the cursor
and dragging pans. A directory resolves to one sequence — dominant
extension, then dominant name pattern — so EXR renders play without
their JPEG previews or a sibling pass interleaved. Frames decode on a background thread into a prefetching cache, so
playback does not stall on I/O. An exposure slider works in stops,
channels can be isolated (R/G/B/A/luma), multi-part EXR files get a part
selector, and an inspector panel shows the spec and every metadata
attribute. Display-encoded sources — an ordinary JPEG next to a linear
EXR — are linearised on load so the one display transform is right for
both, and a file that fails to decode shows its error as a placeholder
instead of taking the viewer down. It is an example of the API, not a
product — it favours readable code over playback throughput, and its
README says exactly where that line is drawn. See
[viewer/README.md](viewer/README.md) for the scope, the key table and
the design notes.

## What "safe" means here

The crate's purpose is that ordinary use cannot cause undefined behaviour,
so it is worth being precise about how that is achieved and where it stops.

**Checked before entering C++.** Every pixel transfer validates the region and
the buffer against the image's own dimensions, and computes strides from that
validation rather than trusting the caller. A buffer of the wrong length, a
region outside the data window, a tile block off the tile grid, or dimensions
that multiply past `usize` are all errors returned before any pointer is
handed over. The `Pixel` trait is sealed, so a Rust type cannot be paired with
an unrelated OpenImageIO one.

**Lifetimes, not conventions.** A cached tile is released when its `TileGuard`
drops. An `ImageHandle` borrows its cache and cannot outlive it. A tile's
pixels borrow the guard. An in-memory reader owns the bytes it reads, and its
fields are ordered so the reader closes before the proxy, and the proxy before
the buffer it borrows.

**Threading follows OpenImageIO's documentation, not guesswork.** `ImageCache`
and `ImageHandle` are `Send + Sync`; `Perthread` is deliberately neither,
because OpenImageIO states that one "should NEVER be shared between running
threads". Those are the only `unsafe impl`s in the crate, and each carries its
reasoning in a comment.

**What this does not claim.** OpenImageIO is a large C++ library and this
crate does not audit it: a malformed file that crashes a reader will crash the
process. That risk is measured rather than assumed — the opt-in corpus suite
reads OpenEXR's `Damaged` directory, 68 files from a fuzzing corpus of reader
crash cases, and requires each to produce an error or an image. The `oiio-sys`
crate is a thin, mostly `unsafe` layer and is not intended for direct use.

Three memory-safety bugs have been found and fixed in this fork so far; see
[CHANGELOG.md](CHANGELOG.md).


### Development

Build `oiio` and `oiio-sys` using `cargo`. The workspace `Cargo.lock` is
committed so CI and contributors test the same Rust dependency graph; it does
not constrain applications that depend on these library crates.

```bash
cargo build --workspace --locked
```


### Testing

The test suite in the `tests` directory is used to validate the `oiio` crate.

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
cargo test --workspace --release --locked
```

GitHub Actions runs the optimized suite against OpenImageIO 3.1.14 on Linux,
macOS, and Windows. The Windows job deliberately uses the vcpkg discovery path;
the Unix jobs use `pkg-config`. The debug suite runs on Linux, since debug
assertions and overflow checks only exist in that profile.

### Testing against real images

Those tests all read images this crate wrote, which proves it agrees with
itself. Two further suites read files it did not write, and are opt-in because
the corpora are large and not vendored:

```bash
git clone https://github.com/AcademySoftwareFoundation/OpenImageIO-images
git clone https://github.com/AcademySoftwareFoundation/openexr-images
OIIO_BIND_TEST_IMAGES=/path/to/OpenImageIO-images \
OIIO_BIND_TEST_EXR_IMAGES=/path/to/openexr-images/exr-images \
  cargo test --test corpus_test -- --nocapture
```

Without those variables the suite reports that it was skipped and passes.
With them it reads every image in both corpora — overscan EXRs with negative
data window origins, tiled mip pyramids, multi-part files, real camera
metadata — and checks that the cache and the direct reader agree. It also
reads OpenEXR's `Damaged` directory, a fuzzing corpus of reader crash cases,
and requires that each file produces an error or an image rather than taking
the process with it.


## Links

- [source repository](https://github.com/KMean/oiio-bind)
- [OpenImageIO 3.1 C++ documentation](https://openimageio.readthedocs.io/en/v3.1.12.0/)

- [oiio on crates.io](https://crates.io/crates/oiio) and
  [its API documentation on docs.rs](https://docs.rs/oiio)
- [oiio-sys on crates.io](https://crates.io/crates/oiio-sys)


## Credits

This is a fork of [vfx-rs/oiio-bind](https://github.com/vfx-rs/oiio-bind) by
Scott Wilson and David Aguilar, whose history and Apache-2.0 license it keeps.
