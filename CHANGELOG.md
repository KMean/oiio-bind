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
- `DeepImage`: read deep files, where each pixel holds a list of samples.
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
