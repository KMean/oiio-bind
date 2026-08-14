# Changelog

This fork's changes, newest first.

## 0.1.0 — unreleased

The first release of this fork, and the first release under these names:
neither `oiio` nor `oiio-sys` had been published to crates.io before.

Both crates are versioned 0.1.0 together. `oiio-sys` carried 0.2.0-beta0 from
before the fork, but nothing was ever published under it, so starting both at
0.1.0 says what is true — this is a first release, and the API may still move.

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
- `oiio::algo` quality control and texture preparation from the binding gap
  map: `color_range_check` returning its three counters as a struct (and
  refusing the channel range OpenImageIO would answer with zeroes under a
  success return), `color_map`/`color_map_named` with the knot table
  validated in `u64` — upstream's own room check multiplies in `int`, where
  a large pair overflows into an out-of-bounds read — plus
  `compare_with_relative`, `normalize` (draining the source's error stack,
  where OpenImageIO records the 3/4-channel refusal) and
  `fillholes_pushpull`, whose internal failures upstream discards into a
  silently black success.
- `oiio::algo` channel layout from the binding gap map: `channel_append`
  (which requires an empty destination and refuses deep images, since
  OpenImageIO shapes the union result itself with no `IBAprep` in sight) and
  the `maxchan`/`minchan` reductions, whose channel range is validated
  before OpenImageIO reads `a[chbegin]` unconditionally. `Roi` gained its
  algebra — `union`, `intersection` as an `Option`, and the two containment
  tests — validated rather than inverted on disjoint inputs.
- `oiio::algo` compositing and value hygiene from the binding gap map:
  `repremult` (which refuses a source with no alpha channel rather than
  degrading to OpenImageIO's silently misplaced paste), `zover` depth
  compositing, `scale` for single-channel mask multiplies, `fix_non_finite`
  with a typed `NonFiniteFix` mode returning the repaired-pixel count, and
  the `rangecompress`/`rangeexpand` pair, documented with their real 0.18
  knee — the identity-below-1.0 wisdom is not what OpenImageIO ships.
- `ImageBuf::try_clone`, the copy with the failure reportable: `Clone` still
  works and panics with the reason when the copy's pixels cannot be
  allocated, where OpenImageIO's own copy constructor would have handed back
  a broken copy that crashed on its first read.
- `ImageBuf::pixels_valid`, which says whether the pixels in memory were ever
  filled in. OpenImageIO allocates before it opens the file, so a failed read
  leaves the allocation behind untouched, and nothing exposed the difference.
- `ImageSpec::pixel_count`, and `TextureConfig::MAX_CHANNELS`.
- `tests/soundness_test.rs` and `tests/property_test.rs`: one test per
  reproduced crash, a sweep that hands deep and empty buffers to every
  operation, and a proptest harness over structurally awkward image shapes.
  `contrib/fuzzing.md` explains why the second layer is proptest rather than
  cargo-fuzz.

### Fixed

- A fourth review — six property lenses rather than another per-module pass:
  concurrency claims, unsafe contracts, panic and drop paths, FFI
  conversions, discarded results and documentation contracts, every finding
  adversarially verified against the OpenImageIO source — found fourteen
  more, and none was refuted. The prior rounds hardened the operations; what
  remained was the properties around them.

  Statistics are exclusive now. `ImageCache::stats`, `reset_stats` and
  `TextureSystem::stats` took `&self`, but OpenImageIO gathers statistics
  with no lock: the merge reads per-thread counters that concurrent lookups
  update, and the file walk reads each file's subimage vector while a first
  open on another thread resizes it — the same free-under-read invalidation
  can cause, reached by a diagnostic call. All three take `&mut self` now,
  matching `invalidate`. Recorded for upstream as issue 12 in
  `contrib/upstream-issues.md`, since it contradicts the manual's claim that
  the cache is completely thread-safe.

  Reading a deep file with one high byte in a channel name no longer aborts
  the process. OpenEXR checks channel names only for termination and length
  — never encoding — and the shim built a borrowed `rust::Str` from those
  file bytes, a throwing constructor inside a `noexcept` wrapper. A borrowed
  string has no lossy form, so the name now comes back owned and lossy, like
  the error strings before it; a regression test binary-patches a channel
  name to `0xE9` bytes and reads the file back.

  The deep sample axis is guarded like the pixel axis. A pixel's sample
  storage is allocated on the first value written, sized from the recorded
  counts, by a vector resize OpenImageIO does not guard; the third review
  capped the pixel count, but no pixel cap bounds `i32::MAX` samples of a
  many-channel pixel. Every shim that can reach that allocation — the
  sample-count setters, the value setters, insert, capacity, the deep
  copies, and the pointer utilities — now catches the failure and reports
  it instead of terminating, on both the `DeepImage` and `ImageBuf` paths.

  `ImageBuf::clone` cannot hand back a copy that crashes on its first read.
  OpenImageIO's copy constructor catches its own allocation failure, records
  an error on the copy, and returns it anyway — with the source's
  valid-pixels flag still set, so reading the broken copy reached a division
  by a zero tile width, or a null cache pointer, inside OpenImageIO. The
  copy's error state is checked now: `Clone` panics with the reason, and the
  new `ImageBuf::try_clone` returns it as an `Err` instead.

  `AttributeValue::is_writable` answers `false` for an empty string array,
  whose write the third review had already made an error; the predicate had
  kept saying `true`.

  Five documentation contracts now say what the code does: a cached tile
  holds the format the cache stores — `f32` for everything but `uint8`,
  `uint16` and `half` — not the file's native format; `constant_color`'s
  threshold-zero comparison is stricter than the float comparison for
  formats wider than `f32`, not equivalent to it; `deepen` also grants a
  sample for a lone non-zero `Z` below OpenImageIO's 1e30 cutoff, and
  excludes `Zback` like `Z`; `write_scanlines` no longer claims only tiled
  EXRs advertise `random_access` — DPX, FITS, GIF, RLA and WebP do so
  unconditionally, and honour it; and the `algo` module's region text
  counts its seven source-region operations and no longer pretends the
  parameter name marks them out. `DeepImage::set_sample_count` documents
  the sample resurrection its `ImageBuf` sibling already admitted — shrink
  keeps the room, regrow brings the old values back — pinned by a test.

- The whole family the property test probes with mismatched destinations —
  `warp`, `rotate`, `fit`, `unsharp_mask`, `channel_sum` — now refuses a
  pre-allocated destination wider than its result, not only the narrower one
  that overran a stack buffer, closing the remaining known members of the
  class below by construction rather than waiting for the property test to
  catch them one at a time.

- `copy` refuses a pre-allocated destination it cannot cover — a channel
  count that disagrees with the source's, or a data window the source does
  not reach. Property testing on the 3.1.14 CI builds caught the second
  operation in the `IBAprep` uninitialised-destination class `mad` was the
  first of: a one-channel source copied into a disjoint five-channel
  destination returned success with `inf` in a channel the copy never wrote.
  The class is recorded as the open observation in
  `contrib/upstream-issues.md`, now with three sightings.

- A third review — nine reviewers fanned across every module plus a
  cross-cutting sweep, each finding verified against the OpenImageIO source —
  found eight more. What it caught, again, were guards applied to one family
  and not a sibling. All are closed, and each behavioural fix has a
  regression test that fails without it.

  The deep sample editors are bounded now. `DeepData::insert_samples` and
  `erase_samples` are the only two `DeepData` operations OpenImageIO does not
  range-check itself — every sibling answers an out-of-range pixel with zero
  or null — and both index their per-pixel bookkeeping vectors directly, so a
  pixel outside the image was a heap read and a heap write. The shims refuse
  it, which keeps the pair callable like their guarded siblings. Recorded for
  upstream as issue 11 in `contrib/upstream-issues.md`.

  A deep image too large for the machine is an error rather than
  `std::terminate`. `DeepData::init` resizes three per-pixel vectors with no
  try/catch of its own, inside shims cxx wraps `noexcept`, on both paths that
  reach it — `ImageBuf::new` on a deep specification, and `DeepImage::new`.
  `DeepImage::new` also now applies the `i32::MAX` pixel cap `ImageBuf::new`
  already had, since `DeepData` indexes pixels with an int and a larger count
  truncated to a negative one.

  `mad` checks the channel count of all three operands. `a*b+c` reads every
  image operand to the union channel count, and the guard the second review
  added lined up only `a` and `b`: the third image operand went unchecked,
  and the variant whose `b` is a constant checked nothing at all.

  An empty string array attribute is an error rather than a silent drop.
  `"string[0]"` parses as a scalar string, so the shim declined to store it —
  correctly — and the write arm discarded the refusal, leaving the attribute
  missing from the file with nothing said: the same discarded-bool shape the
  second review closed on the `Other` arm.

- A second review, against the surface the first one did not cover, found
  forty more. All of them are closed, and each has a regression test that
  fails without its fix.

  Reading a corrupt file could abort the process. OpenImageIO quotes the
  file's own bytes back at you when a header will not parse, and every error
  shim built its string with cxx's throwing `rust::String` constructor while
  being declared `noexcept`, so an attribute name that was not valid UTF-8
  was `std::terminate` rather than an error. Thirty of the OpenEXR project's
  fuzzer fixtures did exactly that. Every one of the thirty-four sites builds
  its string with `rust::String::lossy` now.

- The image cache no longer hands out memory it has freed. `TileGuard` records
  its region and pixel format when the tile is borrowed rather than asking the
  cache afterwards, because OpenImageIO derives a tile's region from the
  *file's* current spec, which invalidation frees, and `pixels()` took the
  length of its slice from it. Reads through an `ImageHandle` validate their
  channel range: OpenImageIO does not clamp it, so asking for eight channels
  of a three channel image read 8/3 past every tile row and reported success.
  A caller-supplied `Perthread` is routed through `get_perthread_info` before
  use, which is what the header requires and where the cache acts on the purge
  flag an invalidation sets, and one belonging to a different cache is refused
  outright — a shared lifetime is not a shared identity.

- `ImageBuf` refuses what it cannot do rather than crashing or guessing.
  A deep buffer reports local storage but has no flat pixels, and its
  iterator leaves the proxy pointer null, so the contiguous pixel API read and
  wrote through null; reading a deep EXR back was enough to reach it. A region
  whose channels start at or past the end left `IBAprep`'s intersection
  inverted and reached `memcpy` with a negative length. A buffer whose read
  failed handed back the uninitialised allocation and called it success.
  `ImageBuf::new` on a specification too large for the machine died inside the
  zero-fill instead of returning the allocation failure.

- The same inverted channel range reached six `algo` operations directly.
  `zero`, `fill`, `copy`, `crop`, `premult` and `unpremult` all crashed on
  a region starting past the destination's last channel.

- Texture lookups are bounded by what the texture has. `subimageinfo` indexes
  a vector behind an assert that release builds compile out, and while the
  samplers clamp the channel count they compute texel addresses from the raw
  first channel, so a subimage or first channel the file does not have walked
  off a cached tile. Asking for more channels than exist still works and still
  fills with the fill value, but the shim supplies the padding rather than
  letting OpenImageIO recurse with an unbounded first channel.
  `TextureConfig::with_channel_count` is clamped: it reaches an `alloca` sized
  from the argument, so a large count overflowed the stack.

- `write_scanlines` requires rows in order for formats that say they need
  them. OpenEXR's scanline writer biases the caller's pointer backwards by the
  requested row and then writes at its own cursor, so starting half way down a
  1024 row image read 8 MB before the buffer.

- An attribute whose type is an array with no concrete length is refused.
  `TypeDesc::fromstring` presets the length to -1 and `size()` clamps it to
  one, so `"float[]"` measured four bytes and passed every guard; nothing
  clamps it on the way back out, where `size_t(-1)` walked off the end of the
  stored value. Building one took safe Rust and no `unsafe`.

- Results that were wrong rather than fatal are errors now: a region outside
  the data window read as zeros and was indistinguishable from a black one,
  a write outside it went nowhere and reported success, `TileGuard::format`
  named the file's format where `pixels()` requires the cache's, an `Other`
  attribute that could not be carried vanished silently, `has_color_space`
  rejected names a conversion accepts, and `deepdata_get_pointers` returned
  the caller's buffer untouched.


- Safe Rust can no longer crash the process or read memory it does not own.
  A pre-publication review found, and reproduced, several inputs that did:
  a deep `ImageBuf` handed to any of seven entry points that walk contiguous
  pixels (`fft`, `paste`, `copy`, `transpose`, `channel_sum`, and `convolve`'s
  kernel and `st_warp`'s coordinate map — the last two are third images, which
  OpenImageIO's own `IBAprep` never sees, and `fft` does not call it at all);
  `ifft` on an ordinary overscan image, whose data window is smaller than its
  display window, which returned heap contents as pixels; `pixel_hash_sha1` on
  an `ImageBuf::empty()`, which divided by zero; `paste` with a channel offset
  negative enough to reach `std::terminate`; `channel_sum` with fewer weights
  than the source has channels, which read past the caller's own slice;
  `warp`, `rotate` and an exact `fit` into a destination narrower than the
  source, which wrote past a stack buffer; a region larger than the source
  handed to any of the three OpenImageIO fast paths that take a raw pointer
  from it; and `set_deep_sample_count` with a coordinate outside the image,
  which resized a different pixel and reported success. Each is now refused or
  clamped, and `tests/soundness_test.rs` covers every one.
- `algo::compare` returns `Result`. `CompareResults` is a plain aggregate, so
  when the comparison cannot be made — one image deep and the other flat —
  OpenImageIO sets only its error flag and every measurement is left
  whatever the stack held.
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
  but indexes it by absolute channel number; its `histogram` never clamps the
  channel range, so a region naming more channels than the image has is read
  from every pixel; and its `computePixelHashSHA1` sizes its block results
  from the region but indexes them from the image. Most of them also have no
  deep-image guard — `computePixelStats` and `nonzero_region` are the two that
  handle deep images properly. The bindings refuse or clamp each case.
  Drafted for upstream as issues 5, 6 and 7; `pixel_hash_sha1` also does not
  expose the block size, which is the only way to reach the third.
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

- Several signatures moved to `&mut self`, and one option is gone, because the
  old shapes could not be made sound. `ImageCache::invalidate`,
  `invalidate_all`, and `TextureSystem`'s invalidation and attribute setters
  are exclusive: invalidation is not made thread-safe by OpenImageIO, and it
  frees state that a live `TileGuard`, `ImageHandle` or `Perthread` still
  points into, so both hazards are compile errors now rather than reads of
  freed memory. The fourth review found statistics gathering on the same
  footing, so `ImageCache::stats`, `ImageCache::reset_stats` and
  `TextureSystem::stats` are exclusive too. `ImageCacheBuilder::shared` is removed: two Rust
  values over OpenImageIO's process-wide cache would let `&mut` on one alias
  `&` on the other, and every lifetime claim in that module is written against
  a single value. The third review removed `TextureSystem::shared` for the
  same reason — it was the one process-wide singleton still offered as
  multiple Rust values, so invalidation through one could free the subimage
  spec a lookup through another was reading, exactly the race the exclusive
  receivers exist to prevent. A private system shared through `Arc` has no
  such ambiguity.
- `TileGuard::roi` returns `Roi` rather than `Result<Roi>`, since it can no
  longer fail, and reports the region recorded when the tile was borrowed.
- `ImageOutput::spec` reports the specification the file was opened with
  rather than the caller's. `check_open` rewrites it — the origin is zeroed
  for formats that cannot carry one, the display window is filled in, a zero
  depth is raised to one — and a caller had no way to see any of it.
- The `imagebufalgo_*` declarations in `oiio-sys` are `unsafe fn`. In a cxx
  bridge, `unsafe extern "C++"` only asserts that the signatures are right, so
  they were callable from safe Rust with a hand-built `ROI`; the contract is
  stated once at the top of that module. The third review found one straggler
  in the raw-buffer read family — `imageinput_read_native_tiles`, which sizes
  the read from caller-supplied ranges — and it is `unsafe fn` now like its
  siblings, and `ImageSpec::to_sys`'s documentation now says what the code
  deliberately does: an attribute that cannot be carried faithfully fails the
  write rather than vanishing from it.

- `ImageSpec` is constructible from Rust, carries its `PixelFormat`, and
  exposes metadata; it is no longer only something a reader hands back.
- Windows CI no longer builds the test suite twice, and caches vcpkg's built
  packages between runs. The debug suite moved to Linux, where a second build
  costs seconds rather than minutes.
- `missing_docs` is warned on, and the public API is documented.
