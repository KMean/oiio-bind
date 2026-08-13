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
- [x] Writing metadata whose OpenImageIO type this crate does not model
  directly. `AttributeValue::Other` carries the value's original bytes, so a
  `float2`, a `timecode`, an ICC profile or a chromaticity survives being read
  from one image and written to another. Only its printed form rounds; the
  bytes do not. String arrays are carried as strings, since OpenImageIO stores
  those as pointers rather than characters.
- [x] Reading deep images. The contiguous pixel API still refuses them, since
  a fixed number of values per pixel cannot describe a list of samples;
  `ImageInput::read_deep_image` returns a `DeepImage` instead.
- [x] Writing deep images. A `DeepImage` is constructible from Rust: declare
  the sample count for a pixel, then set each sample's value. Channels keep
  the type the specification gave them, so an unsigned channel round-trips
  through `set_value_uint` rather than through a float that cannot hold it.
  `ImageOutput::write_deep_image` refuses a writer that was opened for flat
  pixels, and refuses a deep image whose dimensions or channel count disagree
  with the specification.

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

`ImageBufAlgo`, `DeepData`, `TextureSystem`, and custom image I/O plugins
should be added in independent, reviewable increments after the core buffer and
ownership model is proven.

Custom image I/O plugins are the one item here that is not merely unfinished.
Writing a format plugin means OpenImageIO calling back into Rust, and the
commented-out `declare_imageio_format` in `oiio-sys/src/imageio.rs` is where
that would start. Upstream marked their own version of it blocked, and nothing
in this fork has changed that.

- [x] `ImageBuf`: allocate from a specification or attach to a file, read on
  demand, bounded pixel transfer in both directions with conversion, write to
  a file, and deep copy. This is what `ImageBufAlgo` will operate on.
- [x] `ImageBufAlgo`, first slice: `zero`, `fill`, `add`/`sub`/`mul`/`div`
  against another image or a constant per channel, `abs`, `absdiff`, `copy`
  with conversion, `crop`, `flip`, `flop`, `transpose`, and `compare`.
- [x] `ImageBufAlgo` geometry and compositing: `resize`, `resample`, `fit`,
  `over`, `premult`/`unpremult`, `channel_sum`, and `channels` for reordering,
  dropping, duplicating or inventing channels.
- [x] Colour management: `algo::color_convert` between named spaces, and
  `ColorConfig` to ask the active OpenColorIO configuration which spaces and
  roles it actually defines, so a caller can name one that exists rather than
  discovering at conversion time that it does not.
- [x] `TextureSystem`: filtered texture lookups with wrap, mip and
  interpolation modes, and explicit derivatives, so a caller gets the
  filtering a renderer would otherwise write itself.

The `ImageBufAlgo` surface is being completed in slices, each one commit:

- [x] `make_texture`, to write the `.tx` mip pyramids `TextureSystem` reads,
  from a file or from an `ImageBuf`. Configuration is a `TextureConfig` with
  typed settings rather than the raw `maketx:` attribute names, though those
  remain reachable. Note that the output format has the last word on the data
  format and takes it silently: TIFF, which is what a `.tx` is, promotes
  `half` to `float`, and OpenEXR demotes integer formats to `half`.
- [ ] Statistics and introspection: `computePixelStats`, `histogram`,
  `isConstantColor`, `isMonochrome`, `nonzero_region`, `computePixelHashSHA1`.
- [x] Remaining pixel maths: `mad`, `pow`, `clamp`, `min`, `max`,
  `contrast_remap`, `saturate`, `invert`, `paste`, `cut`. `Operand` mirrors
  OpenImageIO's `Image_or_Const` for the three operations that accept either.
  `max` refuses two argument shapes that `min` accepts, because OpenImageIO's
  image-against-image `max` reads and writes out of bounds for them; see
  issue 4 in `contrib/upstream-issues.md`.
- [ ] Rotation and warping: `rotate`, the right-angle rotations, `reorient`,
  `warp`, `st_warp`.
- [ ] Filtering: `convolve`, `make_kernel`, `unsharp_mask`, `median_filter`,
  `laplacian`, `dilate`, `erode`, and the Fourier pair.
- [ ] Deep compositing: `flatten`, `deepen`, `deep_merge`, `deep_holdout`.
- [ ] Colour transforms beyond a space change: `colormatrixtransform`,
  `ociolook`, `ociodisplay`, `ociofiletransform`.
- [ ] Drawing and generators: `render_text`, `render_line`, `render_box`,
  `render_point`, `noise`, `checker`.

The project intentionally does not try to generate bindings for every OIIO C++
template or expose raw C++ pointer semantics through the safe crate.
