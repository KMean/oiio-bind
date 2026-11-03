# Roadmap

The goal is a production-quality, community-maintained Rust interface to
OpenImageIO. The public `oiio` crate should be safe and idiomatic; C++ types,
version differences, and ownership details stay inside `oiio-sys`.

The compatibility baseline is OpenImageIO 3.1.4 or newer, across both
released lines: **3.1.x and 3.2.x**. Support for a new OIIO minor line is
established and tested deliberately rather than assumed, which is why the
runtime check in `tests/imageio_test.rs` carries an explicit upper bound that
3.3 will have to be opted into.

The floor is compile-checked, not only declared: the `oiio-sys` shims — the
crate's entire OIIO-facing surface — compile without error or warning against
the v3.1.4.0-beta headers verbatim (checked 2026-08-14, MSVC, compile-only).

The ceiling is run-checked: the full suite passes against a locally built
**3.2.2.0** release (checked 2026-10-30, MSVC, via `OIIO_ROOT`), as well as
against the 3.1.12 used locally and the 3.1.14 used in CI.

Note that a 3.1-built and a 3.2-built `oiio-sys` are not link-compatible:
OpenImageIO puts the minor version in an inline namespace (`v3_1`, `v3_2`).
3.2 does keep `v3_1` and `v3_0` as legacy namespaces and re-exports a number
of symbols forward from them, so the source-level surface is far more stable
across the two than the namespace change alone suggests.

Behaviour does differ between patch releases, and the tests account for it
rather than pinning one: 3.1.16 fixed the Targa thumbnail writer (#5236) and
3.1.18.1 fixed the `image_span` defects described below. One caution when
writing such checks — `openimageio_version()` is
`10000*major + 100*minor + patch` and carries no TWEAK, so 3.1.18.0 and
3.1.18.1 are indistinguishable at 30118.

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
- [x] Streaming deep images: `write_deep_scanlines`/`write_deep_tiles` and
  `read_deep_scanlines_at`/`read_deep_tiles_at`, each band or block a
  `DeepImage` shaped exactly like its region. Scanline bands are in-order
  and tile ranges tile-aligned, because OpenEXR lays the sample arrays over
  the file by trusting the caller's coordinates. Region reads always
  include every channel — OpenImageIO's deep channel-subset path pairs the
  kept channels' data with the wrong names.
- [x] The `ImageCache` image queries (existence, UDIM, counts, formats,
  texture kind, average and constant colors) and thumbnails end to end:
  cache-side reading, `ImageBuf` carriage, `ImageOutput` writing with the
  Targa silent-failure shapes refused. `make_texture` stamps a `Software`
  tag when the caller sets none, so the cache honours API-made textures'
  constant-color metadata (issue 14).
- [x] The remaining `oiiotool`-core operations: `circular_shift`
  (empty-destination bijection), `color_count` (bounded to 32768 colors —
  OpenImageIO tallies into unbounded stack scratch), and `compare_yee`
  (its own result type, image-coordinate worst pixel, in-window region and
  sane viewing parameters required).
- [x] Texture lookups for production: `TextureOptions::missing_color`
  (lost textures return a color instead of killing the lookup) and the
  UDIM surface — `is_udim`, `resolve_udim`, `inventory_udim` — with
  `TextureHandle` pointers confined to the C++ shims.
- [x] Native-format reads: `ImageInput::read_native_image` returns the
  file's exact bytes, each channel its own format, with
  `ImageSpec::native_pixel_bytes` as the stride — byte-validated against
  the file's own specification before OpenImageIO sees a pointer.
- [x] An independent C++-vs-crate corpus differential:
  `contrib/corpus_hash.cpp` (pure OpenImageIO C++, separately compiled, no
  crate code) and `examples/corpus_hash.rs` hash every subimage of both
  test corpora as `f32` in the same format; the arms agree byte-for-byte
  on all 463 readable subimages and on every error/deep verdict. This is
  the check the in-process differentials cannot make: a bug inside the
  crate's own C++ shims would show up here.
- [x] `TextureHandle`-based lookups, the per-call name-hash skip renderers
  use. The safety design is `ImageCache::handle`'s: the handle borrows the
  system, so invalidation cannot happen while one lives. Lookups share the
  by-name path's validation (bounds, missing color, the UDIM gates), and
  creation runs a real `exists` probe because OpenImageIO's `good()` is
  only the broken flag, which a never-opened missing file passes.
- [x] Demonstration applications on the public API: `examples/oiiox.rs`,
  an `oiiotool`-flavoured command chain, and `viewer/`, a minimal image
  and sequence viewer in its own workspace-excluded crate. Ergonomics
  follow-ups noticed while building them: OpenImageIO prints its own
  duplicate error line to stderr when a file fails to open, even though
  the crate drains the error channel — worth tracing;
  `ImageBuf::subimage_count` answers 0 for a derived in-memory buffer
  where 1 would read more naturally; and a convenience for "the same spec
  at a different size, channel names kept" would simplify pre-allocating
  `resize` destinations.

## Notes on the OpenImageIO span API

The bounded pixel calls validate the caller's buffer against the image
specification and then pass explicit strides, rather than using OpenImageIO's
`image_span` overloads. Two defects motivated that, both measured with
two standalone C++ programs using only public OpenImageIO API, so neither
depended on this crate.

**Both are now fixed upstream**, by Larry Gritz in PR #5531 (commit
`054d048`, merged 2026-10-02), first released in **3.1.18.1** and **3.2.1.1**:

- `ImageInput::read_tiles` taking an `image_span` forwarded the x and y ranges
  to the pointer overload in the wrong order and dropped `ystride` and
  `zstride`, so a tiled read returned a different rectangle of the image.
  Usually it returned true with wrong pixels; for some shapes it returned true
  while leaving half the destination untouched; for others it returned false
  with no error recorded. Reported as issue #5400; the first diagnosis blamed
  partial edge tiles, which was wrong and was publicly retracted.
- `ImageOutput::write_scanlines` taking an `image_span` validated the range
  against `[0, height)` rather than `[y, y + height)`, so it rejected ranges
  expressed in image coordinates when the data window origin was non-zero,
  reporting "Invalid scanline range 5-9" for rows 5..9 of a data window whose
  origin is y=5. This was never filed; the maintainer found it by auditing the
  rest of the `image_span` family after reading #5400, along with three more
  defects in that family that this crate never drafted.

The explicit-stride overloads handle both cases correctly, and the safety
argument is unchanged: `bounded_pixel_layout` still proves that the buffer holds
exactly one contiguous value per channel, pixel, row, and slice before any
pointer reaches C++. Both behaviours have regression tests.

The explicit-stride paths nonetheless stay. They are correct on every
OpenImageIO version, including the many that predate the fix, and switching to
the `image_span` overloads would mean either raising the minimum supported
version to 3.1.18.1 — cutting off every 3.1 release before it — or carrying
both paths behind a version check for no behavioural gain.

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
- [x] Statistics and introspection: `pixel_stats`, `histogram`,
  `constant_color`, `is_constant_channel`, `is_monochrome`, `nonzero_region`
  and `pixel_hash_sha1`. Three of them read or write out of bounds for regions
  the rest of `ImageBufAlgo` accepts. Most also have no deep-image guard, and
  a deep
  buffer has no pixel pointer to walk — the two exceptions are
  `computePixelStats`, which has a per-sample branch, and `nonzero_region`,
  which dispatches to `deep_nonempty_region`. This binding still refuses deep
  images to `pixel_stats`, because per-sample figures under a per-pixel name
  would mislead; `nonzero_region` accepts them.
- [x] Remaining pixel maths: `mad`, `pow`, `clamp`, `min`, `max`,
  `contrast_remap`, `saturate`, `invert`, `paste`, `cut`. `Operand` mirrors
  OpenImageIO's `Image_or_Const` for the three operations that accept either.
  `max` refuses two argument shapes that `min` accepts, because OpenImageIO's
  image-against-image `max` reads and writes out of bounds for them, reported
  upstream as #5645.
- [x] Rotation and warping: `rotate`, the right-angle rotations, `reorient`,
  `warp`, `st_warp`. The right-angle rotations take a region of the *source*,
  as `paste` does; `rotate` takes radians clockwise and is hard-wired to black
  edges, so `warp` is the way to choose a wrap mode. `reorient` returns false
  without recording anything when the `Orientation` attribute is not one of the
  eight EXIF values, so the binding supplies that message itself.
- [x] Filtering: `make_kernel`, `convolve`, `laplacian`, `unsharp_mask`,
  `median_filter`, `dilate`, `erode`, `fft`/`ifft` and the polar pair. Five
  guards were needed: an unknown filter name gives a box kernel and a complaint
  nobody reads; an empty kernel divides by its own zero sum and returns `NaN`
  as success; `unsharp_mask` reads the source through the *destination's* pixel
  type; a window of one translates the image instead of leaving it alone;
  `dilate` and `erode` write a float extreme into pixels with no source under
  them; and `ifft` dereferences a null pixel address behind an assertion a
  release build removes.
- [x] Deep compositing: `flatten`, `deepen`, `deep_merge` and `deep_holdout`,
  and the `ImageBuf` sample access they need — `is_deep`,
  `deep_sample_count`, `deep_value` and their setters, with channel and sample
  indices checked here because OpenImageIO answers an out-of-range one with a
  null pointer and then reads zero or drops the write. `flatten` refuses a
  destination wider than its source, whose accumulator it would read past the
  end of, and `deepen` requires an empty destination, since into any other one
  it silently drops what does not fit.
- [x] Colour transforms beyond a space change: `color_matrix_transform`,
  `ocio_look`, `ocio_display`, `ocio_file_transform` and
  `ocio_named_transform`. All of them, and `color_convert`, share one
  OpenImageIO pixel engine that corrupts channels past the fourth, so each
  restores those from the source; see issue 8. They also always pass a real
  `ColorConfig`, because `ociolook` dereferences it before checking whether
  one was given (issue 9).
- [x] Drawing and generators: `fill_gradient` and `fill_corners`, `checker`,
  `noise`, `render_point`, `render_line`, `render_box`, `render_text` and
  `text_size`. `Noise` is an enum, so the two anonymous floats OpenImageIO
  takes are named per kind; note that noise is *added* to the destination
  rather than replacing it, except `Salt`. Guards: a checkerboard square of
  zero is a division by zero; a filled box with reversed corners draws nothing
  and reports success; and text with no glyphs leaves the measured box
  inverted, which `render_text` then builds an `ImageSpec` from, underflowing
  its width.

  **`render_text` is bound but not exercised against real glyphs here.** The
  vcpkg OpenImageIO this crate builds against reports "not compiled with
  FreeType for font rendering", so the tests assert that the reason is
  reported rather than that letters appear. A build with FreeType would
  exercise the rest.

The `ImageBufAlgo` surface this project set out to bind is now complete.

## Publishing

**Published.** `oiio-sys` 0.1.0 and `oiio` 0.1.1 went to crates.io on
2026-08-15, `oiio-sys` first; the current releases are `oiio` 0.2.1 and
`oiio-sys` 0.2.0. `contrib/publishing.md` has the checklist and what was
verified.

## On binding OpenImageIO safely

Binding this library turned up defects in OpenImageIO itself, many of which
let safe Rust read or write out of bounds, crash, or silently corrupt an
image. Four were reported upstream:

- [#5400](https://github.com/AcademySoftwareFoundation/OpenImageIO/issues/5400),
  `read_image` with an `image_span` transposing x and y on tiled files. Fixed
  in PR #5531, which also fixed four more in the same family once the
  maintainer audited it.
- [#5638](https://github.com/AcademySoftwareFoundation/OpenImageIO/issues/5638),
  the colour engine writing `0.5 + 10 * src` into channels past the fourth
  instead of copying them. Fixed.
- [#5645](https://github.com/AcademySoftwareFoundation/OpenImageIO/issues/5645),
  `ImageBufAlgo::max` widening the channel range where `min` narrows it. Fixed
  in PR #5646.
- [#5648](https://github.com/AcademySoftwareFoundation/OpenImageIO/issues/5648),
  `make_kernel("binomial", ...)` sizing its tap arrays from the requested
  extent and indexing them over the odd-rounded one. Open.

The others are guarded here rather than passed on, which is why a few
operations refuse arguments OpenImageIO accepts:

- `algo::max` refuses unequal channel counts and a narrower destination.
- `algo::constant_color` and `algo::nonzero_region` refuse a region that
  begins above channel zero.
- `algo::flatten` refuses a destination wider than its source.
- `algo::unsharp_mask` refuses a destination of a different pixel type.
- `algo::checker` refuses a square of zero, `render_box` a reversed filled
  box, `render_text` text with no glyphs, and the filter window ops a width of
  one.
- `algo::paste` refuses a first channel on a deep source, which OpenImageIO
  ignores, and sizes an empty deep destination from the destination region, so
  an offset paste lands where it was asked rather than wrapping into the next
  scanline. Where the destination is already allocated it clips the source to
  what fits, which is both the flat path's behaviour and what the call
  documents, and refuses the paste when nothing would land.
- Every measurement, and `ifft`, refuse inputs whose pixels they would
  dereference through a null pointer.

The colour operations do not refuse anything: they repair, copying the
channels past the fourth that OpenImageIO's colour engine would otherwise
scale and offset.

Each of these is a documented restriction with a test, not a silent narrowing.

The project intentionally does not try to generate bindings for every OIIO C++
template or expose raw C++ pointer semantics through the safe crate.
