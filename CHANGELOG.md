# Changelog

This fork's changes, newest first. Nothing here has been released to
crates.io yet, so there are no version numbers to hang them on.

## Unreleased

### Added

- `ImageOutput`: create and write files, whole images, scanline ranges or
  blocks of tiles, with subimages, mip levels and multi-part files. A writer
  is always open, so there is no state in which a write can be attempted
  against an unopened file.
- `ImageInput::read_region_into`: read part of an image. A region is a `Roi`,
  which selects channels as well as pixels, so one AOV can be read out of a
  many-channel EXR without decoding the rest.
- `ImageCache`: `ImageHandle` to skip repeated file-name lookups, `Perthread`
  for per-thread state, and `TileGuard` to borrow a single tile, released on
  drop.
- Reading and writing in memory, through `ImageInput::from_memory` and
  `ImageOutput::to_memory`, without touching the filesystem.
- `ImageBuf`, and `oiio::algo` over it: `zero`, `fill`, arithmetic against
  another image or a constant, `abs`, `absdiff`, `copy`, `crop`, `flip`,
  `flop`, `transpose`, `compare`, `resize`, `resample`, `fit`, `over`,
  `premult`, `unpremult`, `channel_sum`, `channels` and `color_convert`.
- `oiio::algo` drawing and generators: `fill_gradient`, `fill_corners`,
  `checker`, `noise`, `render_point`, `render_line`, `render_box`,
  `render_text` and `text_size`. `Noise` names what OpenImageIO's two
  anonymous floats mean for each kind, and the documentation says plainly
  that noise is added to the destination rather than replacing it — every
  kind but `Salt`, which assigns. A checkerboard square of zero, a filled box
  with reversed corners, and text with nothing to draw are all refused; each
  is respectively a division by zero, a silent no-op reported as success, and
  an inverted bounding box that OpenImageIO builds an image from without
  checking. Text rendering itself is untested against glyphs, because the
  OpenImageIO this builds against has no FreeType.
- `oiio::algo` deep compositing: `flatten`, `deepen`, `deep_merge` and
  `deep_holdout`, together with the `ImageBuf` sample access they need —
  `is_deep`, `deep_sample_count`, `deep_value`, `deep_value_uint` and their
  setters. Indices are checked in Rust, because OpenImageIO answers an
  out-of-range channel or sample with a null pointer and then reads zero or
  drops the write without a word. `flatten` refuses a destination with more
  channels than its source, whose per-pixel accumulator it would read past;
  `deepen` requires an empty destination, since it can only install its deep
  specification into one.
- `oiio::algo` filtering: `make_kernel` and `convolve`, `laplacian`,
  `unsharp_mask`, `median_filter`, `dilate` and `erode`, `fft` and `ifft`, and
  `polar_to_complex`/`complex_to_polar`. Several refuse arguments OpenImageIO
  accepts and then answers wrongly: an unknown kernel name (which yields a box
  kernel and a complaint on a buffer nobody inspects), an empty kernel (which
  divides by a zero sum and fills the image with `NaN` while reporting
  success), a filter window of one (which translates the image by a pixel
  rather than leaving it alone), and an `unsharp_mask` destination whose pixel
  type differs from the source's (which OpenImageIO reads the source through,
  without converting). `dilate` and `erode` keep to the source's data window,
  since outside it they leave a float extreme rather than an error; and `ifft`
  insists the source's pixels are in memory, since OpenImageIO casts the pixel
  address to a complex pointer behind an assertion a release build removes.
- `oiio::algo` colour transforms beyond a space change:
  `color_matrix_transform` by a 4x4 matrix, and `ocio_look`, `ocio_display`,
  `ocio_file_transform` and `ocio_named_transform` through OpenColorIO.
  `OcioOptions` carries the settings they share, defaulting as OpenImageIO
  does — which means unpremultiplication is on.
- `oiio::algo` measurements: `pixel_stats` for range, mean, spread and how
  many values were not finite; `histogram`; `constant_color`,
  `is_constant_channel` and `is_monochrome`; `nonzero_region` to shrink-wrap
  the content; and `pixel_hash_sha1`. `constant_color` returns the colour
  itself rather than a bool and an out parameter, since OpenImageIO leaves
  that buffer untouched when the answer is no.
- `oiio::algo` rotation and warping: `rotate_90`, `rotate_180`, `rotate_270`,
  `reorient`, `rotate` by an arbitrary angle, `warp` by a 3x3 matrix, and
  `st_warp` by a map of source coordinates. `rotate` takes radians and turns
  clockwise; its edges are black and not adjustable, so `warp` is where a wrap
  mode is chosen. `reorient` now reports the orientation it did not recognise
  instead of failing silently, which is what OpenImageIO does on its own.
- `oiio::algo` pixel maths: `mad`, `pow`, `clamp`, `min`, `max`,
  `contrast_remap`, `saturate`, `invert`, `paste` and `cut`. `Operand` is
  OpenImageIO's `Image_or_Const`, so `mad`, `min` and `max` take either an
  image or one constant per channel. Note that `paste`'s region selects part
  of the source rather than of the destination, and that an empty slice of
  per-channel constants means zero for most of these and "no bound" for
  `clamp`; both are OpenImageIO's rules, and both are documented per
  operation.
- `DeepImage`: read deep files, where each pixel holds a list of samples, and
  build one from Rust to write. A channel keeps the type its specification
  gave it, so an unsigned channel survives a round trip through
  `set_value_uint` rather than through a float that cannot hold it.
- `TextureSystem`: filtered texture lookups, with wrap, mip and interpolation
  modes and explicit derivatives — the mip selection and filtering a renderer
  would otherwise write itself.
- `make_texture`, which writes the tiled, MIP-mapped files `TextureSystem`
  reads, from a file or from an `ImageBuf`. `TextureConfig` gives the common
  `maketx` settings names and types, and still reaches the rest by attribute.
  The crate no longer needs `maketx` or `oiiotool` to produce a texture, so
  the texture tests no longer need an external corpus to have a real one.
- `ColorConfig`: report which colour spaces and roles the active
  OpenColorIO configuration defines.
- `PixelFormat`, describing what a file stores, including formats the
  contiguous buffer API does not itself read or write.
- `AttributeValue`, and metadata that survives a round trip: types this crate
  does not model keep the bytes OpenImageIO stored, so a `float2`, a
  `timecode` or an ICC profile is not lost on the way out.
- Tests against OpenImageIO's and OpenEXR's own image corpora, opt-in through
  `OIIO_BIND_TEST_IMAGES` and `OIIO_BIND_TEST_EXR_IMAGES`, including
  OpenEXR's `Damaged` directory of reader crash cases.

### Fixed

- Colour operations no longer corrupt channels past the fourth. OpenImageIO's
  colour engine says in a comment that it copies leftover channels "unaltered
  from the source" and then writes `0.5 + 10 * source` into them, so any
  conversion of an image with more than four channels — a multi-AOV EXR, say —
  silently ruined every channel after the first four. This affected
  `color_convert`, which this crate already had, as much as the new
  transforms. Each colour call now restores those channels from the source,
  and a test fails without the repair. Drafted for upstream as issue 8;
  present in 3.1 and still in 3.2.
- `algo::ocio_look` cannot be made to dereference a null pointer.
  OpenImageIO's `ociolook` resolves a colour space through the `ColorConfig`
  fifteen lines before it checks whether one was supplied, so the documented
  way of asking for the source's own space crashes on the default
  configuration. The bindings always pass a real configuration. Drafted for
  upstream as issue 9.
- The measurements cannot be made to read or write out of bounds, nor to
  dereference a deep image's absent pixel pointer. OpenImageIO's
  `isConstantColor` sizes its reference buffer to the region's channel count
  but indexes it by absolute channel number; its `histogram` is alone among
  the statistics in never clamping the channel range, so the 10000 a
  default-constructed region carries is read from every pixel; and its
  `computePixelHashSHA1` sizes its block results from the region but indexes
  them from the image. None of the seven checks for a deep image. The
  bindings refuse or clamp each case. Drafted for upstream as issues 5, 6 and
  7; `pixel_hash_sha1` also does not expose the block size, which is the only
  way to reach the third.
- `algo::max` cannot be made to read or write out of bounds. OpenImageIO's
  image-against-image `max` widens its channel range where `min` narrows it,
  after the range has already been clamped to what the buffers hold, so it
  indexes past the shorter input and past a destination narrower than either.
  Its own assertion — in code the widening makes unreachable — says the range
  was meant to narrow. The binding refuses those shapes rather than passing
  them on. Drafted for upstream as issue 4 in `contrib/upstream-issues.md`;
  present in 3.1 and still in 3.2.
- Three borrowed strings pointed into freed memory. `ImageBuf::name`,
  `ImageBuf::file_format_name` and `DeepData::channelname` each built a
  `rust::Str` from an OpenImageIO `string_view`, which selects the
  `std::string` constructor and leaves the result pointing at a destroyed
  temporary. `file_format_name` returned rubbish; the others survived by
  luck.
- Tiled images whose dimensions are not an exact multiple of the tile size
  could not be read at all. The bounded read went through OpenImageIO's
  `image_span` overload, which returns false for those without recording an
  error. The explicit-stride overload reads them correctly. Reported upstream
  as [OpenImageIO#5400](https://github.com/AcademySoftwareFoundation/OpenImageIO/issues/5400).
- Writing scanlines to an image with a non-zero data window origin failed for
  the same reason, and is worked around the same way.
- The minimum-version guard never fired, on any version. OpenImageIO defines
  `OIIO_VERSION_GREATER_EQUAL` without parenthesising its expression, so
  `#if !OIIO_VERSION_GREATER_EQUAL(3, 1, 4)` binds the `!` to `OIIO_VERSION`
  alone and always evaluates false.

### Changed

- `ImageSpec` is constructible from Rust, carries its `PixelFormat`, and
  exposes metadata; it is no longer only something a reader hands back.
- Windows CI no longer builds the test suite twice, and caches vcpkg's built
  packages between runs. The debug suite moved to Linux, where a second build
  costs seconds rather than minutes.
- `missing_docs` is warned on, and the public API is documented.
