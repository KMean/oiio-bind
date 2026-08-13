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
- [x] Introduce safe `ImageSpec`, `Roi`, and error types. `ImageSpec` is
  constructible from Rust, carries its `PixelFormat`, and exposes every
  metadata attribute as an `AttributeValue`.
- [x] Give `ImageInput` and `ImageOutput` explicit fallible `close` operations.

## 2. ImageCache as a first-class API

- [x] Add a private-cache builder with typed configuration attributes.
- [x] Implement `image_spec`, `get_pixels_into`, invalidation, errors, and stats.
- [x] Add cache-owning `ImageHandle` and `TileGuard` types; releasing a tile is
  automatic on drop. A handle borrows the cache so it cannot outlive it, and a
  tile's pixels borrow the guard so they cannot outlive the tile.
- [x] Model per-thread cache state as neither `Send` nor `Sync`, which is what
  OpenImageIO asks for: "given one of these should NEVER be shared between
  running threads". `ImageHandle` is `Send` and `Sync`, matching OpenImageIO's
  own pairing of a shared handle with per-thread state.
- [x] Test scanline and tiled EXR/PNG access, cache lifetime, and tile release.

## 3. Image I/O

- [x] Complete `ImageOutput` creation, writes, and round-trip tests. Whole
  images, scanline ranges, and tile blocks are covered, as are subimages, mip
  levels, multi-part files, non-zero data window origins, and metadata.
- [x] Complete safe, caller-buffer `ImageInput` reads. Whole images, scanline
  ranges, tile blocks, and channel subsets, at any subimage and mip level.
  A read region is a `Roi`, so a channel subset is just a narrowed data
  window. Writing a channel subset instead means describing those channels in
  the specification, so it needs no separate API.
- [x] Read and write images in memory through an `IOProxy`, so a caller can
  decode bytes it already holds and encode without touching the filesystem.
- [ ] Writing metadata whose OpenImageIO type this crate does not model
  directly, such as matrices and arrays.
- [ ] Deep images, which the contiguous pixel API deliberately refuses.

## Notes on the OpenImageIO 3.1 span API

The bounded pixel calls validate the caller's buffer against the image
specification and then pass explicit strides, rather than using OpenImageIO's
`image_span` overloads. Two behaviours motivate that, both measured with
`contrib/span_repro.cpp` — a standalone C++ program using only public
OpenImageIO API, so neither depends on this crate — and both reproduced
identically on **3.1.12.0** and on **3.2.0.2dev** (`main`, August 2026):

- `ImageInput::read_image` taking an `image_span` returns false, without
  recording an error, whenever a tiled image's width or height is not an exact
  multiple of the tile size. No exactly sized destination buffer can satisfy
  it, so tiled images with partial edge tiles could not be read at all.
  Reproduced at 40x32, 32x24 and 40x24 with 16px tiles; 32x32 and 16x16 read
  fine, and the pointer overload reads every one of them correctly.
- `ImageOutput::write_scanlines` taking an `image_span` rejects scanline ranges
  expressed in image coordinates when the data window origin is non-zero,
  reporting "Invalid scanline range 5-9" for rows 5..9 of a data window whose
  origin is y=5. The pointer overload accepts the same range.

The explicit-stride overloads handle both cases correctly, and the safety
argument is unchanged: `bounded_pixel_layout` still proves that the buffer holds
exactly one contiguous value per channel, pixel, row, and slice before any
pointer reaches C++. Both behaviours have regression tests.

To check whether a newer OpenImageIO has fixed them, build the reproduction
against it and run it; it exits non-zero when the two overloads disagree:

```
cl /std:c++17 /EHsc /utf-8 /MD contrib/span_repro.cpp \
   /I <oiio>/include /I <deps>/include \
   /link /LIBPATH:<oiio>/lib OpenImageIO.lib OpenImageIO_Util.lib
```

If it reports no mismatches, the span overloads can be restored and the
minimum supported version raised to whichever release fixed them.

## Later subsystems

`ImageBuf`, `ImageBufAlgo`, `DeepData`, `TextureSystem`, and custom image I/O
plugins should be added in independent, reviewable increments after the core
buffer and ownership model is proven.

The project intentionally does not try to generate bindings for every OIIO C++
template or expose raw C++ pointer semantics through the safe crate.
