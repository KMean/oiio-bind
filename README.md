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

Until the next crates.io release, depend on the GitHub repository:

```toml
[dependencies]
oiio = { git = "https://github.com/KMean/oiio-bind", branch = "main" }
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

Metadata is read as `AttributeValue`. Integer, float, and string attributes are
modelled directly and can be written back; any other OpenImageIO type is
preserved for inspection with its type name and OpenImageIO's own rendering of
the value.

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
the Unix jobs use `pkg-config`.


## Links

- [source repository](https://github.com/KMean/oiio-bind)
- [oiio on crates.io](https://crates.io/crates/oiio/latest)
- [oiio-sys on crates.io](https://crates.io/crates/oiio-sys/latest)
- [oiio documentation](https://docs.rs/crate/oiio/latest)
- [oiio-sys documentation](https://docs.rs/crate/oiio-sys/latest)
- [OpenImageIO 3.1 C++ documentation](https://openimageio.readthedocs.io/en/v3.1.12.0/)
