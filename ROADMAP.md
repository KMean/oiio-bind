# Roadmap

The goal is a production-quality, community-maintained Rust interface to
OpenImageIO. The public `oiio` crate should be safe and idiomatic; C++ types,
version differences, and ownership details stay inside `oiio-sys`.

The current compatibility baseline is OpenImageIO 3.1.4 or newer 3.1.x. Its patch releases
are expected to retain ABI compatibility, while support for a new OIIO minor
line is established and tested deliberately.

## 0. Modern OIIO baseline

- [x] Preserve the `vfx-rs/oiio-bind` history and Apache-2.0 license.
- [x] Discover OIIO with Windows/MSVC vcpkg, Unix `pkg-config`, or explicit
  install paths.
- [x] Build and run PNG smoke tests on Windows with OIIO 3.1.
- [x] Migrate `ImageCache` and `ImageBuf` cache ownership to `std::shared_ptr`.
- [x] Make trivial cross-language layouts explicit and test them.
- [x] Add Linux, Windows, and macOS CI for supported OIIO versions.

## 1. Safety foundation

- [x] Replace core input/cache raw pixel-pointer overloads with OIIO 3.1
  span-based shims. Remaining low-level compatibility calls are unsafe.
- [x] Add a sealed Rust pixel-type abstraction for `u8`, `u16`, `half`, and
  `f32`.
- [x] Validate dimensions, channels, strides, multiplication overflow, and buffer
  lengths before entering C++.
- [x] Introduce safe `ImageSpec`, `Roi`, and error types. Metadata coverage is
  still intentionally narrow.
- [ ] Give `ImageInput` and `ImageOutput` explicit fallible `close` operations.
  (`ImageInput` is complete.)

## 2. ImageCache as a first-class API

- [x] Add a private-cache builder with typed configuration attributes.
- [x] Implement `image_spec`, `get_pixels_into`, invalidation, errors, and stats.
- Add cache-owning `ImageHandle` and `TileGuard` types; releasing a tile must be
  automatic on drop.
- Model per-thread cache state as neither `Send` nor `Sync`.
- Test scanline and tiled EXR/PNG access, cache lifetime, and tile release.

## 3. Image I/O

- Complete safe, caller-buffer `ImageInput` reads.
- Complete `ImageOutput` creation, writes, and round-trip tests.
- Cover subimages, mip levels, tiled data, channel subsets, and metadata.

## Later subsystems

`ImageBuf`, `ImageBufAlgo`, `DeepData`, `TextureSystem`, and custom image I/O
plugins should be added in independent, reviewable increments after the core
buffer and ownership model is proven.

The project intentionally does not try to generate bindings for every OIIO C++
template or expose raw C++ pointer semantics through the safe crate.
