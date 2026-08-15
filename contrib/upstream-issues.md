# Draft reports for OpenImageIO

Nineteen issues from building Rust bindings against OpenImageIO 3.1/3.2, plus
one open observation not yet settled enough to report.

Issues 1 and 2 have self-contained reproductions, so a maintainer running one
is never shown a second unrelated API behaviour:

- `contrib/span_tiled_read_repro.cpp` — issue 1
- `contrib/span_scanline_origin_repro.cpp` — issue 2

Both use only public OpenImageIO API — no Rust and no binding code — and exit
non-zero when two overloads of the same call disagree, given the same
`ImageSpec` and the same buffer. The tiled repro checks only return values, so
for issue 1 it understates the problem — issue 1 carries a separate table of
measured pixel values; the scanline repro also compares read-back pixels.

Issues 4–9, 11 and 12 were found by reading the source while binding it, and
are stated as code review rather than as runnable reproductions. Where a
reproduction is cheap it is noted in the issue.

**Issues or pull requests.** The repository's `.github/ISSUE_TEMPLATE/bug_report.md`
ends with: "IF YOU ALREADY HAVE A CODE FIX: There is no need to file a
separate issue, please just go straight to making a pull request." Issues 3,
4, 5, 7, 8, 9 and 11 each name a one- or two-line fix, so they belong as pull
requests, not issues. Issue 2 is genuinely a question about intended
behaviour, so it is an issue. Issue 6 could be either.

`CONTRIBUTING.md` asks for the version, platform, compiler, and a repro others
can run. The `bug:` title prefix comes from the bug-report template's
`title: "bug:"` prefill rather than from `CONTRIBUTING.md`, whose prefix list
covers commits and pull requests — where it endorses a parenthesised
subcategory, e.g. `fix(IBA):`, and names `IBA` for `ImageBufAlgo` explicitly.

Issues 4–7 and 11 are out-of-bounds reads and writes. `SECURITY.md` invites
judgement here, and each of those is an API-misuse hazard reachable only from
a caller's own arguments rather than from untrusted file data, so a normal
issue or pull request is the right channel.

Shared environment block, applying to every issue below:

Unabridged `oiiotool --buildinfo`, as the bug template asks:

```
OIIO 3.2.0.2dev | Windows/x86_64
    Build compiler: MSVS 1951 | C++17/199711
    HW features enabled at build: sse2
    No CUDA support (disabled / unavailable at build time)
Dependencies: BZip2 1.0.8, DCMTK NONE, FFmpeg NONE, fmt 12.1.0, Freetype 2.13.3, GIF 5.2.2, Imath 3.2.2, JXL NONE, libdeflate 1.25, Libheif NONE, libjpeg-turbo 3.1.3, LibRaw 0.22.0, libuhdr NONE, OpenColorIO 2.5.1, OpenCV NONE, OpenEXR 3.4.7, OpenJPEG 2.5.4, openjph 0.26.3, PNG 1.6.55, Ptex NONE, Ptex NONE, Robinmap 1.4.1, TBB NONE, TIFF 4.7.1, WebP 1.6.0, ZLIB 1.3.1
```

Also reproduced identically on the released 3.1.12.0 (vcpkg, same machine and
compiler), so this is not new in 3.2. Where an issue below cites line numbers,
they are given for `main` and for 3.1.16.0, the latest 3.1 release; the code in
question is unchanged across the whole 3.1 series.

---

## Issue 1 — filed as #5400; the correcting follow-up was posted 2026-08-14 and the issue retitled

**Title:** `bug: ImageInput::read_image() with an image_span transposes x and y for tiled images`

**Describe the bug**

The `image_span` overload of `ImageInput::read_tiles` forwards its arguments to
the pointer overload with the x and y ranges swapped, so every tiled read
through an `image_span` requests the wrong rectangle. `read_image` with an
`image_span` drives that call one tile row at a time, so it inherits the fault.

`src/libOpenImageIO/imageinput.cpp:719` on `main`, `:702` in 3.1.16.0, `:694`
in 3.1.12.0 — the text is identical in all of them:

```c++
// Default implementation (for now): call the old pointer+stride
return read_tiles(subimage, miplevel, ybegin, yend, xbegin, xend, zbegin,
                  zend, chbegin, chend, format, data.data(),
                  data.xstride());
```

against a pointer overload declared
`read_tiles(subimage, miplevel, xbegin, xend, ybegin, yend, ...)`.

`ImageOutput::write_tiles` does the same job in the same shape, under the same
comment, and forwards its ranges in order — so the read side looks like a slip
rather than a convention.

Depending on the geometry this surfaces in three different ways, none of them
diagnosed:

1. **Silent wrong data.** When the transposed rectangle still satisfies
   `ImageSpec::valid_tile_range`, the read returns `true` with an empty
   `geterror()` and the buffer holds the wrong pixels: each row of tiles
   receives the transposed rectangle's data written at a row pitch recomputed
   for the swapped dimensions — the forward also drops `ystride`/`zstride`,
   passing only `data.xstride()` — so the result is scrambled, not even a
   clean transpose. With square tiles this happens for every square image, and
   for every image whose dimensions are an exact multiple of the tile size.
   (With non-square tiles the swapped ranges usually fail validation instead —
   e.g. a 32×32 image with 16×8 tiles falls into the silent-`false` case.)
2. **Silent success over half-uninitialised memory.** `valid_tile_range` checks
   divisibility by the tile size and the two `== width` / `== height` escape
   hatches, but never checks that the range lies inside the image. A 32×16
   image is asked for `y` in `[0,32)`, a 64×32 image for `y` in `[0,64)`. The
   request is accepted, no data comes back for the region that does not exist,
   and the call still returns `true` — leaving exactly half the destination
   buffer untouched.
3. **Silent failure.** When the transposed rectangle fails `valid_tile_range`,
   the pointer overload returns `false` without recording an error, so
   `geterror()` is empty. The buffer may already be partly written by earlier
   tile rows.

There is no buffer overrun in any case: the transposed rectangle has the same
value count as the intended one, so the damage is misplacement and
non-placement rather than a write past the end.

I expected the `image_span` overload to read the same images the pointer
overload reads, and on failure to record an error explaining why.

**OpenImageIO version and dependencies**

(paste the environment block above)

**To Reproduce**

Build and run the attached `span_tiled_read_repro.cpp`. It writes tiled OpenEXR
files with `write_image`, then reads each back three ways. Verbatim output on
3.2.0.2dev:

```
OpenImageIO 3.2.0.2dev

32x32, 16x16 tiles (exact multiple)
  read_image(image_span, explicit strides) : ok
  read_image(image_span, default strides)  : ok
  read_image(pointer)                      : ok

16x16, 16x16 tiles (exact multiple)
  read_image(image_span, explicit strides) : ok
  read_image(image_span, default strides)  : ok
  read_image(pointer)                      : ok

40x32, 16x16 tiles (PARTIAL edge tiles)
  read_image(image_span, explicit strides) : FAILED
  read_image(image_span, default strides)  : FAILED
  read_image(pointer)                      : ok
  >>> MISMATCH: the overloads disagree on the same file

32x24, 16x16 tiles (PARTIAL edge tiles)
  read_image(image_span, explicit strides) : FAILED
  read_image(image_span, default strides)  : FAILED
  read_image(pointer)                      : ok
  >>> MISMATCH: the overloads disagree on the same file

40x24, 16x16 tiles (PARTIAL edge tiles)
  read_image(image_span, explicit strides) : FAILED
  read_image(image_span, default strides)  : FAILED
  read_image(pointer)                      : ok
  >>> MISMATCH: the overloads disagree on the same file

3 mismatch(es)
```

Note this reproduction only checks the return value, so the cases it prints as
`ok` are **not** clean — they return `true` with transposed or partly
uninitialised data. The table below measures pixel values instead.

Both `image_span` forms are covered: the `image_span<std::byte>` plus
`TypeDesc::FLOAT` overload with strides spelled out, and the typed
`image_span<float>` overload with the strides left to `image_span`'s defaults.
They behave identically, so the result does not depend on the caller's stride
arithmetic.

**Evidence**

Writing float tiled EXRs whose pixel values are separable in x and y
(`ch0 = x + y/1000`, `ch1 = y + x/1000`, `ch2 = x*1000 + y`), reading each back
through both overloads, and pre-filling the destination with a sentinel to tell
"written wrongly" from "never written". All 16×16 tiles, 3 channels, on
3.2.0.2dev. The pointer read matches the generator exactly in every case, so
the files on disk are correct:

| dims  | span returns | geterror | values differing from pointer read | left uninitialised |
|-------|--------------|----------|------------------------------------|--------------------|
| 16×16 | true         | (empty)  | 0 / 768                            | 0                  |
| 32×32 | true         | (empty)  | 2976 / 3072                        | 0                  |
| 32×16 | true         | (empty)  | 1488 / 1536                        | 768                |
| 64×32 | true         | (empty)  | 6048 / 6144                        | 3072               |
| 40×32 | **false**    | (empty)  | 3840 / 3840                        | 3840               |
| 32×24 | **false**    | (empty)  | 2256 / 2304                        | 1152               |
| 40×40 | true         | (empty)  | 4680 / 4800                        | 0                  |
| 24×24 | true         | (empty)  | 1656 / 1728                        | 0                  |
| 17×17 | true         | (empty)  | 816 / 867                          | 0                  |

Every tile row the span path actually executed matches the transposed-request
prediction — swapped ranges, with `ystride` recomputed from the swapped
width — on 100% of in-bounds values (verified 3072/3072 for 32×32). The few
values that agree with the correct image number three channels times one
image side: 32×32 agrees on 96 = 3 × 32 (pixels (0..15, 0) and (16..31, 31)),
40×40 on 120 = 3 × 40, 17×17 on 51 = 3 × 17. They are not the main diagonal —
the buffer is not a neat transposition, because the recomputed row pitch
scrambles the layout further.

Only a single square tile (16×16 with 16×16 tiles) is correct, because there
the swapped arguments are equal.

**The fix**

Forward the ranges in declaration order, and pass all three strides as the
write side does:

```c++
return read_tiles(subimage, miplevel, xbegin, xend, ybegin, yend, zbegin,
                  zend, chbegin, chend, format, data.data(),
                  data.xstride(), data.ystride(), data.zstride());
```

Separately, `ImageSpec::valid_tile_range` accepting a range outside the image
is what turns cases 1 and 2 into silent successes, and the unannotated
`return false` under the guard at `imageinput.cpp:737` on `main` (`:720` in
3.1.16.0) is what makes case 3 silent. Both are worth addressing on their own.

Introduced in PR #4748, which added the `image_span` methods, and unchanged
since — 3.1.9.0, 3.1.12.0, 3.1.14.1, 3.1.16.0 and current `main` all carry the
same text. No format plugin overrides these methods, so the base implementation
is always the one that runs.

---

## Issue 2

**Title:** `bug: ImageOutput::write_scanlines() with an image_span mishandles a non-zero data window origin`

**Describe the bug**

With a data window whose origin is `y = 5` and a height of 4, writing rows
`5..9` through
`ImageOutput::write_scanlines(ybegin, yend, format, image_span)` fails with
`write_scanlines: Invalid scanline range 5-9`, although that range is exactly
the data window. The pointer overload
`write_scanlines(ybegin, yend, z, format, void*, ...)` accepts the same range
with the same specification and buffer.

Passing `0..4` instead is accepted. That is not a second, relative convention
— it is the same absolute convention with a zero-based bounds test in front of
it. `ybegin`/`yend` are forwarded verbatim to the absolute-coordinate pointer
overload, so the accepted call is interpreted downstream as absolute rows
`0..3`, which lie outside the data window entirely.

The documentation is not ambiguous about which convention is intended.
`imageoutput.rst` states that the pixel indices passed to the write functions
are coordinates relative to the full image rather than to the crop window, and
its own example loops `for (int y = yorigin; y < yorigin+croplength; ++y)`.
The overload's doxygen repeats the pointer overload's wording. So the bounds
test is simply comparing against the wrong bound.

The accepted call is worse than a wrong write. Because the buffer is handed
through unchanged, the OpenEXR writer sets its slice base to the buffer start
and then fills data-window rows 5..8 by addressing `base + y * ystride`,
reading past the end of a buffer that only holds four rows. The differing
pixels reported below are heap garbage, and ASan reports a
heap-buffer-overflow read. That is the part I would flag: a caller who
responds to the rejection by subtracting the origin gets no error, incorrect
output, and an out-of-bounds read.

For context, PR #5004 has already corrected one round of incorrect
`image_span` size checks in `imageoutput.cpp`; this looks like a second
instance in the same family, in the range check rather than the size check.
The overload itself arrived in PR #4727.

**OpenImageIO version and dependencies**

(paste the environment block above)

**To Reproduce**

The attached `span_scanline_origin_repro.cpp`. Verbatim output on 3.2.0.2dev:

```
OpenImageIO 3.2.0.2dev

data window origin y=5, height 4

  write_scanlines(image_span), rows 5..9 : FAILED, error: write_scanlines: Invalid scanline range 5-9
  write_scanlines(image_span), rows 0..4  : ok
  write_scanlines(pointer),   rows 5..9 : ok

  >>> MISMATCH: the two overloads disagree about the same range

  read back: origins 5 and 5 (same), pixels DIFFER
  >>> the accepted 0-based call wrote different data, so this is not simply a different
      coordinate convention

2 mismatch(es)
```

Because the accepted call reads out of bounds, "pixels DIFFER" is what happens
in practice rather than something the standard guarantees; the out-of-bounds
read is the reliable symptom.

---

## Issue 3 — better as a pull request

**Title:** `fix(oiioversion): parenthesise OIIO_VERSION_GREATER_EQUAL and OIIO_VERSION_LESS`

`src/include/OpenImageIO/oiioversion.h.in` — the template from which the
installed `oiioversion.h` is generated, and so the file a pull request must
edit — defines the version tests without wrapping the expression:

```c
#define OIIO_VERSION_GREATER_EQUAL(major,minor,patch) \
                        OIIO_VERSION >= OIIO_MAKE_VERSION(major,minor,patch)
```

So the natural way to require a minimum version silently does nothing:

```c
#if !OIIO_VERSION_GREATER_EQUAL(3, 1, 4)
#    error "needs OpenImageIO 3.1.4 or newer"
#endif
```

expands to `!OIIO_VERSION >= OIIO_MAKE_VERSION(3, 1, 4)`, and `!` binds to
`OIIO_VERSION` alone, giving `0 >= 30104` — false for every version, so the
`#error` is unreachable. We wrote that guard believing it worked; it never
fired, and a genuinely too-old OpenImageIO instead failed later with a
confusing C++ overload error.

Wrapping the expression in parentheses fixes it and cannot break any existing
correct use:

```c
#define OIIO_VERSION_GREATER_EQUAL(major,minor,patch) \
                        (OIIO_VERSION >= OIIO_MAKE_VERSION(major,minor,patch))
```

There is exactly one sibling, `OIIO_VERSION_LESS`, and it is identically
affected, so the change is two lines. `OIIO_MAKE_VERSION` is already
parenthesised and needs nothing.

Unchanged in 3.1.9.0, 3.1.12.0 and on current `main`.

For context, #2261 asked for easier compile-time version tests and PR #2641
answered it with `OIIO_MAKE_VERSION` — parenthesised, and not affected. The
two comparison macros were added later by PR #2831, which has no description
and drew no review comments, so parenthesisation was never discussed; the
idiom preferred in #2261's discussion was the raw `#if OIIO_VERSION <= 20008`,
which is presumably why this went unnoticed.

## Issue 4 — `ImageBufAlgo::max` widens the channel range where `min` narrows it

**Title:** `ImageBufAlgo::max(dst, A, B)` reads and writes out of bounds when
the destination has fewer channels than the inputs, or the inputs differ in
channel count

`src/libOpenImageIO/imagebufalgo_pixelmath.cpp:169`, in the image-against-image
branch of `max`:

```c++
roi.chend = std::max(roi.chend, std::max(A.nchannels(), B.nchannels()));
```

`min`, the immediately preceding function, has at line 72:

```c++
roi.chend = std::min(roi.chend, std::min(A.nchannels(), B.nchannels()));
```

and `absdiff`, the same pattern a third time, has `std::min` at line 323. Only
`max` widens.

The block immediately below the dispatch is the same in all three (`absdiff`
differs only in using `OIIO_DASSERT` where the other two use `OIIO_ASSERT`),
and in `max` it is unreachable. Its guard, in full, is:

```c++
if (roi.chend < origroi.chend && A.nchannels() != B.nchannels()) {
```

The first conjunct alone can never hold after a `std::max` against `origroi`'s
own bound, so the second never gets a say. That block's
`OIIO_ASSERT(roi.chend <= dst.nchannels())` records the invariant the widening
breaks.

Two consequences. `IBAprep` on this path clamps `roi.chend` to the *largest* of
`dst`/`A`/`B` (`imagebufalgo.cpp:284`) and, when `dst` was already allocated,
to `dst`'s own channel count (`imagebufalgo.cpp:109` and `:112`) — it
deliberately does not narrow to the shorter input, which is exactly why `min`
needs its own `std::min`:

1. `max` never narrows `roi.chend` to the channel count common to both inputs
   the way `min` does, so with `A.nchannels() != B.nchannels()` the kernel
   evaluates `a[c]` or `b[c]` for `c` beyond the shorter input.
   `ImageBuf::ConstIterator::operator[]` constructs a `ConstDataArrayProxy`
   over the pixel's base pointer, whose `operator[]` does no bounds check, so
   this reads adjacent pixel memory.
2. With `dst` pre-allocated narrower than the inputs — `max(dst=3ch, A=4ch,
   B=4ch)`, where the inputs do *not* differ — `IBAprep` brings `roi.chend`
   down to 3 and the `std::max` puts it back to 4, so the kernel *writes*
   `r[3]` past the end of each destination pixel.

A narrower ROI supplied by the caller does not prevent either, because the
widening ignores the incoming value whenever it is smaller.

The constant-operand branch of `max` passes `IBAprep_CLAMP_MUTUAL_NCHANNELS`
and is unaffected; this is specifically the image-against-image branch. The
existing tests in `imagebufalgo_test.cpp` compare 4-channel against 4-channel
only, which is presumably why this has never fired in CI.

Unchanged from 3.1.9 through 3.1.16 and on current `main`; the whole file is
byte-identical between v3.1.9.0 and `main`.

The fix is the one-line one, matching `min` and `absdiff`:

```c++
roi.chend = std::min(roi.chend, std::min(A.nchannels(), B.nchannels()));
```

which also makes the surplus-channel block below it reachable, as it is in
`min`.

Found while binding `max` for Rust. Until it is fixed, the binding refuses
unequal channel counts and destinations narrower than the inputs, since it
cannot otherwise keep its safety promise.

## Issue 5 — `isConstantColor` writes past its reference buffer

**Title:** `ImageBufAlgo::isConstantColor` writes and reads past its reference
vector when `roi.chbegin > 0`

`src/libOpenImageIO/imagebufalgo_compare.cpp`, in `isConstantColor_`:

```c++
std::vector<T> constval(roi.nchannels());
ImageBuf::ConstIterator<T, T> s(src, roi);
for (int c = roi.chbegin; c < roi.chend; ++c)
    constval[c] = s[c];
```

The vector holds `roi.chend - roi.chbegin` entries but is indexed by absolute
channel number. With a 4-channel image and `chbegin = 1, chend = 4` it has
three entries and `constval[3]` is written past the end. The public wrapper
clamps `chend` to the image's channel count but never touches `chbegin`, so
nothing upstream prevents it.

Whether that write is visible depends on allocator slack, so a release build
may show nothing; ASan or MSVC's debug STL reports it immediately.

There are three matching out-of-range *reads*, not one: the two-pixel early-out
at `:410`, and both parallel kernels at `:425` and `:439`. `parallel_image`
copies `chbegin`/`chend` verbatim into each sub-ROI, so all three are reachable
with the same ROI.

Sizing the vector `roi.chend` entries fixes all four sites at once. Indexing it
`constval[c - roi.chbegin]` also works but must be applied at every one of the
four, or the comparisons silently read the wrong channel.

`chbegin > 0` is intended usage rather than an undocumented corner:
`imagebufalgo.h` documents comparing channels `[roi.chbegin..roi.chend-1]`, the
very next declaration (`isConstantChannel`) says its own `chbegin`/`chend` are
ignored — a deliberate contrast — and the colour write-back later in this same
function already zero-fills `color[0..roi.chbegin)`, which only makes sense if
`chbegin > 0` was meant to work.

`nonzero_region` reaches this too: it trims by calling `isConstantColor` on
strips, and `roi_intersection` (`src/include/OpenImageIO/imageio.h`) preserves
the caller's `chbegin`. `oiiotool` is not affected — its only `nonzero_region`
call uses a default ROI — so this needs a caller-supplied `chbegin > 0`.

No C++ is needed to reach it; the Python bindings pass the ROI straight
through:

```python
ImageBufAlgo.isConstantColor(buf, 0.0, ROI(0, w, 0, h, 0, 1, 1, 4))
```

`src/libOpenImageIO/imagebufalgo_compare.cpp` is byte-identical between
v3.1.9.0 and current `main`, and the same code is at the same lines in
3.1.12.0.

The two-pixel early-out came in with PR #3383; it is not the origin of the
indexing, but it is where the second read site appeared.

## Issue 6 — `histogram` does not clamp its channel range

**Title:** `ImageBufAlgo::histogram` reads out of bounds with `ignore_empty`
and an ROI constructed without an explicit channel range

`ImageBufAlgo::histogram` validates the channel count, the channel, the bin
count and the range, but a *defined* `roi` is passed to `histogram_impl`
verbatim — only an undefined one is replaced by `get_roi(src.spec())`. The
kernel then does:

```c++
if (ignore_empty) {
    bool allblack = true;
    for (int c = roi.chbegin; c < roi.chend; ++c)
        allblack &= (a[c] == 0.0f);
    if (allblack)
        continue;
}
```

`ROI`'s four-argument constructor defaults `chend` to 10000, so the natural
`ROI(x0, x1, y0, y1)` makes this read 10000 channels out of every pixel. `a[c]`
is `ImageBuf::ConstIterator::operator[]`, which builds a `ConstDataArrayProxy`
over the pixel and forwards to *its* `operator[]`; neither bounds-checks.

Note this is not the default-constructed `ROI()`, which is undefined and is
precisely the case the wrapper replaces — it is any ROI built with explicit
bounds but no channel range.

Every other function in this file that iterates `[roi.chbegin, roi.chend)`
clamps `roi.chend` first — `computePixelStats`, `isConstantColor`,
`isMonochrome`, `color_count` and `color_range_check` to `src.nchannels()`,
`compare` to `max(A.nchannels(), B.nchannels())`. `histogram` appears to be the
one that was missed.

`IBAprep`, which does this clamping for every function with a destination
image, is not called here because `histogram` has no destination — so the
clamp has to be by hand.

The Python bindings forward the ROI verbatim and `ROI(int,int,int,int)`
inherits the same `chend = 10000`, so this reproduces in one line under ASan:

```python
ImageBufAlgo.histogram(buf, 0, 256, 0.0, 1.0, True, ROI(0, w, 0, h))
```

`src/libOpenImageIO/imagebufalgo_compare.cpp` is byte-identical between
v3.1.9.0, 3.1.12.0 and current `main`.

## Issue 7 — `computePixelHashSHA1` indexes its block results by the wrong origin

**Title:** `ImageBufAlgo::computePixelHashSHA1` writes past `results` when
`blocksize > 0` and the ROI does not start at the image's first row

```c++
int nblocks = (roi.height() + blocksize - 1) / blocksize;
OIIO_ASSERT(nblocks > 1);
std::vector<std::string> results(nblocks);
parallel_for_chunked(roi.ybegin, roi.yend, blocksize,
                     [&](int64_t ybegin, int64_t yend) {
    int64_t b   = (ybegin - src.ybegin()) / blocksize;  // block number
    ...
    results[b]  = simplePixelHashSHA1(src, "", broi);
}, nthreads);
```

`results` is sized from the ROI's height, but the block index is computed from
the *image's* first row. The chunked loop walks `[roi.ybegin, roi.yend)`, so
chunk *k* starts at `roi.ybegin + k * blocksize` and gets index
`k + (roi.ybegin - src.ybegin()) / blocksize`. The indices are therefore right
only while `roi.ybegin - src.ybegin()` is smaller than one block; from
`roi.ybegin - src.ybegin() >= blocksize` onward every index is shifted.

With `src.ybegin() == 0`, `roi.ybegin == 10`, `roi.yend == 30` and
`blocksize == 4`: `nblocks == 5`, while `b` takes the values 2, 3, 4, 5, 6.
`results[5]` and `results[6]` are past the end, and each writes a `std::string`.

The index should be `(ybegin - roi.ybegin) / blocksize`.

This is a different defect from #5324: that one was an `ImageBuf` race fixed in
#5325, and its reproduction hashed whole images, where
`roi.ybegin == src.ybegin()` and these indices coincide. This survives #5325
and needs an ROI that starts at least one block below the image's first row.

Note also that `blocksize > 0` is not on its own enough to take the blocked
path — it also falls back to `simplePixelHashSHA1` whenever
`blocksize >= roi.height()`.

Found while binding these for Rust; the binding does not expose `blocksize`.

## Issue 8 — the colour engine corrupts channels past the fourth

**Title:** `colorconvert` and friends write `0.5 + 10 * src` into channels 4 and
above instead of copying them

In `src/libOpenImageIO/color_ocio.cpp`, at the end of the per-scanline body in
`colorconvert_impl`:

```c++
if (channelsToCopy < roi.chend && (&R != &A)) {
    // If there are "leftover" channels, just copy them
    // unaltered from the source.
    a.rerange(roi.xbegin, roi.xend, j, j + 1, k, k + 1);
    r.rerange(roi.xbegin, roi.xend, j, j + 1, k, k + 1);
    for (; !r.done(); ++r, ++a)
        for (int c = channelsToCopy; c < roi.chend; ++c)
            r[c] = 0.5 + 10 * a[c];
}
```

The comment says the leftover channels are copied unaltered. The loop scales
and offsets them instead.

This is not a guess about intent. The block entered the file in PR #2987,
"Clarify behavior of color conversion on image with > 4 channels" — and the
line was `0.5 + 10 * a[c]` in that PR's diff as merged. The same pull
request's commit message states that additional channels "will be copied
unaltered from source to destination (not set to black)", and it added to
`imagebufalgo.h` the promise "Any additional channels will be simply copied
unaltered." — a sentence that now appears six times in the public header,
once for each affected operation.
`git log -L` shows the line has never been touched since. It has shipped in
every release from v2.3.7.2 onwards — roughly five years.

Every operation built on this engine is affected — `colorconvert`,
`colormatrixtransform`, `ociolook`, `ociodisplay`, `ociofiletransform`,
`ocionamedtransform` — for any image with more than four channels, which in
practice means most multi-AOV EXRs.

Three conditions are needed together, which is presumably why it has lasted:
more than four channels in the ROI, a destination distinct from the source, and
a processor that is not a no-op. An identity conversion returns through
`ImageBufAlgo::copy` before reaching the engine, and images of four channels or
fewer skip the block because `channelsToCopy == roi.chend` makes its guard
false. The RGBA fast path is gated on both buffers having exactly four
channels, so it never carries an image that would trigger this — it does not
mask the bug so much as never meet it.

Measured on 3.1.9 with a float destination: converting a six-channel image
whose fifth and sixth channels hold 0.125 and 0.875 gives 1.75 and 9.25,
exactly `0.5 + 10 * x`. Both are exactly representable in float and half; an
integer destination would quantise or clamp them. The line is identical on
3.1.12.0 and on current `main`.

The fix is what the comment already says:

```c++
r[c] = a[c];
```

Found while binding the colour operations for Rust. Until it is fixed there,
the binding copies those channels from the source itself after each colour
call, and has a regression test that fails without that repair.

## Issue 9 — `ociolook` dereferences the ColorConfig before checking it

**Title:** `ImageBufAlgo::ociolook` null-dereferences when `fromspace` or
`tospace` is empty and no ColorConfig is supplied

```c++
if (from.empty() || from == "current") {
    auto linearspace = colorconfig->resolve("scene_linear");
    ...
}
...
{
    if (!colorconfig)
        colorconfig = &ColorConfig::default_colorconfig();
```

The null check is thirteen lines below the first dereference, and there is a
second, identical dereference in the `to` block between them. `colorconfig`
defaults to `nullptr`. The trigger is `from.empty() || from == "current"`, so
the literal string `"current"` reaches it too.

An empty `fromspace` is accepted and, per the implementation — it reads
`oiio:Colorspace` from the source's own spec — and per what `--ociolook from=`
promises in the `oiiotool` documentation ("it will try to deduce it from the
image's metadata"), means deduce-from-metadata. (`ociolook`'s own doxygen says an empty string means `scene_linear`,
which is a separate inconsistency and not what this report is about.) So this
is a perfectly ordinary call, not an exotic one. `fromspace` and `tospace` are
required parameters, so a caller must pass `""` explicitly; only `colorconfig`
defaults.

`ColorConfig::resolve` is a non-static member that immediately reads `m_impl`
through `getImpl()`, with no null test on either, so this is an immediate crash
rather than a bad answer.

No C++ is needed to reach it. OpenImageIO's own Python bindings pass a literal
`NULL` for the config, so this crashes the shipped bindings:

```python
ImageBufAlgo.ociolook(dst, src, "look", "", "")
```

`ociodisplay` does the same job in the correct order, which is what the fix
should look like: hoist the

```c++
if (!colorconfig)
    colorconfig = &ColorConfig::default_colorconfig();
```

above the two `from`/`to` resolution blocks.

`ImageBufAlgo::colorconvert` avoids the problem a second way, by defaulting
with a literal — `get_string_attribute("oiio:Colorspace", "scene_linear")` —
so it never touches `colorconfig` before its own null check. Either shape would
do; `ociolook` is simply the one that was missed.

Unchanged on 3.1.9, 3.1.12.0 and current `main`.

Found while binding these for Rust. The binding always passes a real
ColorConfig, so it cannot reach this.

---

## Issue 10 — `copy_image(image_span, image_span)` loops on the wrong variable and writes off the end of the destination

`src/libOpenImageIO/imageio.cpp:1234` (`copy_image`, defined at `:1164`), in the
"General case -- have to do item by item copy" block:

```cpp
for (uint32_t c = 0; x < src.nchannels(); ++c) {
    memcpy(dpel + c * dst.chanstride(),
           spel + c * src.chanstride(), chunksize);
}
```

The channel loop tests `x`, the enclosing pixel index, rather than `c`. `x` is
not modified in the body, so for `x == 0` the loop never terminates: it memcpys
to `dpel + c * dst.chanstride()` for c = 0, 1, 2, ... until it walks out of the
address space. The stride multiplier is caller-supplied, so there is no bound on
how far past the destination it writes.

The branch is entered whenever the destination's channels are not adjacent
within a pixel. The three predicates guarding it collapse to exactly one,
because `is_contiguous` and `is_contiguous_scanline` are both defined in terms
of `is_contiguous_pixel`: `dst.chanstride() != chansize`.

`image_span::getptr` does bounds-check, but the general case calls it once per
scanline and then does raw pointer arithmetic into `memcpy`, so the check is
bypassed.

Reproduced against the shipped OpenImageIO 3.1.12.0 on Windows, with the
destination placed at the base of a 1 MiB `VirtualAlloc` region filled with a
canary so the damage could be measured rather than only observed as a fault:

| case | `chanstride` | channels | branch | result |
| --- | --- | --- | --- | --- |
| control | 4 (adjacent) | 3 | per-pixel memcpy | returns, 0 bytes past the extent |
| 1 | 8 | 3 | general | faulted, 26,392 bytes past a 272-byte extent |
| 2 | 8 | 1 | general | faulted, 46,261 bytes past a 144-byte extent |

Where the destination sits inside a larger mapping, more can be corrupted with
no fault at all.

Present in `v3.1.4.0-beta` (`:1190`), `v3.1.9.0` (`:1200`), `v3.1.12.0`
(`:1201`), `v3.1.16.0` (`:1218`), `v3.2.0.0-dev` (`:1199`), `v3.2.0.2-dev`, and
`main`.

`src/libOpenImageIO/image_span_test.cpp:165` `test_image_span_copy_image` builds
its destination with all-`AutoStride` in all three of its cases, so the
destination is always contiguous-pixel and the general branch has never been
executed by the test suite.

The fix is `c < src.nchannels()`, plus a non-contiguous destination case in
`test_image_span_copy_image`.

The only in-tree caller, `pvt::contiguize` at `imageio.cpp:827`, always builds a
contiguous destination, so this is reached through the public API rather than
through OpenImageIO's own paths — but `copy_image` is `OIIO_API` and the header
recommends it over the pointer form. This binding uses the pointer overload and
is not affected.

---

## Issue 11 — `DeepData::insert_samples` and `erase_samples` do not range-check the pixel index

**Title:** `DeepData::insert_samples`/`erase_samples` read and write past the
per-pixel vectors when `pixel` is out of range

Every other pixel-indexed `DeepData` method checks its `pixel` argument and
answers an out-of-range one with zero, null, or a return — in
`src/libOpenImageIO/deepdata.cpp` at 3.1.16.0: `capacity` (`:496`),
`set_capacity` (`:507`), `samples` (`:540`), `set_samples` (`:551`) and
`data_ptr` (`:639`) all begin with

```c++
if (pixel < 0 || pixel >= m_npixels)
```

The two exceptions are `insert_samples` (`:592`) and `erase_samples` (`:618`),
which index the bookkeeping vectors directly. In `insert_samples`:

```c++
int oldsamps = samples(pixel);                        // guarded, returns 0
if (oldsamps + n > int(m_impl->m_capacity[pixel]))    // unguarded read
    set_capacity(pixel, oldsamps + n);                // guarded, returns
...
m_impl->m_nsamples[pixel] += n;                       // unguarded write
```

and in `erase_samples`:

```c++
n = std::min(n, int(m_impl->m_nsamples[pixel]));      // unguarded read
...
m_impl->m_nsamples[pixel] -= n;                       // unguarded write
```

`m_nsamples` and `m_capacity` hold one `unsigned` per pixel, so any `pixel`
outside `[0, m_npixels)` — negative included — reads and then writes heap at
`vector data + pixel * 4`. When the data is allocated AND `samplepos` is
negative, `insert_samples` also reaches `data_offset(pixel, 0, samplepos)`:
the guarding `if (samplepos < oldsamps)` uses `oldsamps = samples(pixel)`,
which is 0 for an out-of-range pixel, so only `samplepos < 0` enters the
branch. There `data_offset` reads `m_cumcapacity[pixel]` out of range and
feeds the result into a `std::copy_backward` over `m_data`. For
`samplepos >= 0` (as in the reproducer below) that branch is skipped and only
the final `m_impl->m_nsamples[pixel] += n` write lands out of bounds.

Both methods are public API taking `int64_t pixel`, so the caller's own
arithmetic slip lands here directly. `ImageBuf::deep_insert_samples` and
`deep_erase_samples` widen the reach: they compute the pixel index with
`pixelindex(x, y, z)` *without* `check_range` (`imagebuf.cpp:2939` and
`:2950` on `main`, `:2923` and `:2934` in 3.1.16.0), so a coordinate outside
the data window becomes exactly
such an out-of-range — often negative — pixel. The other coordinate-taking
deep accessors (`deep_samples`, `deep_value`, `set_deep_samples`) pass
`check_range = true` or land in a guarded `DeepData` method, so the two
editors are the odd ones out on that level too.

`DeepData`'s own internal callers are safe by accident rather than by
contract: `split` and the merge paths loop `s < samples(pixel)`, which is 0
out of range, so the unguarded pair is never reached with a bad pixel from
inside the library.

The Python bindings expose all four (`py_deepdata.cpp:115` and
`py_imagebuf.cpp:529` on `main`; `:120` and `:510` in 3.1.16.0), so no C++
is needed:

```python
dd = oiio.DeepData()
dd.init(16, 2, (oiio.FLOAT, oiio.FLOAT), ("Z", "A"))
dd.insert_samples(1_000_000, 0, 4)   # heap read and write past m_nsamples
```

The fix is the guard the five siblings already open with, at the top of each
of the two methods:

```c++
if (pixel < 0 || pixel >= m_npixels)
    return;
```

plus `check_range = true` in `ImageBuf::deep_insert_samples`/
`deep_erase_samples` with the early return the other coordinate-taking
accessors have.

Both function bodies are identical at 3.1.9.0 and 3.1.12.0 (`:591`/`:617`),
3.1.16.0 and current `main` (`:592`/`:618`).

Found while binding `DeepData` for Rust. The binding's shims bound `pixel`
themselves, and its `DeepImage` checks coordinates before computing an index,
so it cannot reach either method out of range.

---

## Issue 12 — `getstats`, `mergestats` and `reset_stats` race with concurrent cache use

**Title:** `ImageCache statistics calls are not synchronized against concurrent
lookups, contradicting the documented thread-safety`

`imagecache.rst:29` states: "The ImageCache is completely thread-safe; if
multiple threads are ..." — statistics gathering does not meet that claim.
Line numbers below are identical on `main` and in 3.1.16.0
(`src/libtexture/imagecache.cpp` unless noted).

Three separate races, all reachable by calling a statistics function on one
thread while another thread reads pixels through the same cache:

1. **A use-after-free read in the file walk.** `ImageCacheImpl::getstats`
   (`:2126`) iterates `m_files` and, per file, reads `file->timesopened()`,
   `tilesread()`, `bytesread()`, `iotime()` and `file->subimageinfo(s)` with
   no per-file lock. `find_file` inserts a new `ImageCacheFile` into
   `m_files` *before* opening it ("No such entry in the file cache. Add it,
   but don't open yet", `:1547`), and `ImageCacheFile::open` (`:722`) —
   holding only that file's `m_input_mutex`, which `getstats` never takes —
   clears and repeatedly resizes `m_subimages`. `subimageinfo()`
   (`imagecache_pvt.h:425`) indexes that vector behind an `OIIO_DASSERT` that
   release builds compile out, and the walk's loop bound comes from
   `subimages()` (`:174`), a bare unsynchronized `m_subimages.size()` read,
   so the statistics walk can read the vector mid-reallocation: a dangling
   data pointer.

2. **Unsynchronized counter reads in the merge.** `mergestats` (`:2027`)
   reads every per-thread `ImageCacheStatistics` under
   `m_perthread_info_mutex` — but the owning threads update those fields
   (e.g. `thread_info->m_stats.bytes_read += b` at `:1138`, `:1323`,
   `:1364`) without holding it, and the struct (`imagecache_pvt.h:63`) is
   plain non-atomic scalars. The per-file counters named above
   (`m_tilesread`, `m_bytesread`, `m_timesopened`, `m_iotime`;
   `imagecache_pvt.h:519-520`, `:523-524`) are plain
   `size_t`/`imagesize_t`/`double` too — only the two redundant-tile
   counters at `:521-522` are atomic. Formally a data race, i.e. undefined
   behaviour under the C++ memory model, whatever it happens to produce
   today.

3. **Concurrent writes from the reset.** `reset_stats` (`:2452`) writes
   `init()` into every registered thread's live statistics block and zeroes
   the per-file counters while their owners may be mid-update — a
   write-write race on the same plain fields.

`TextureSystem` statistics inherit all three: `TextureSystemImpl::getstats`
(`texturesys.cpp:779` in 3.1.16.0) calls `m_imagecache->mergestats` (`:785`)
and, with its default `icstats = true`, appends `m_imagecache->getstats`,
while lookups update per-thread counters like `++stats.texture_queries`
(`:1589`) with no lock.

The cheap reproduction for leg 1 under TSan or ASan: thread A loops
`ImageCache::get_image_info`/`get_pixels` over files not yet opened, thread B
loops `getstats(1)`.

Possible directions, from least to most invasive: document statistics calls
as needing external synchronization (an exception to the blanket
thread-safety claim); take each file's `m_input_mutex` in the walk and make
the counters atomics; or snapshot per-thread stats under a lock the writers
also take. Found while binding the cache for Rust; the binding now requires
an exclusive borrow for `stats()`/`reset_stats()`, so its callers cannot
overlap them with reads.

---

## Issue 13 — deep channel-subset reads pair each kept channel's data with the wrong channel's name

`OpenEXRInput::read_native_deep_scanlines` and `read_native_deep_tiles` accept
a channel subset `[chbegin, chend)`, and both initialise the caller's
`DeepData` like this (`exrinput.cpp:1637` and `:1717` on `main`, `:1615` and
`:1695` in 3.1.16.0):

```cpp
std::vector<TypeDesc> channeltypes;
m_spec.get_channelformats(channeltypes);
deepdata.init(npixels, nchans,
              cspan<TypeDesc>(&channeltypes[chbegin], nchans),
              m_spec.channelnames);
```

The channel *types* are offset by `chbegin`; the channel *names* are not.
`DeepData::init` then assigns positionally — `m_impl->m_channelnames[c] =
channelnames[c]` (`deepdata.cpp:385` on both `main` and 3.1.16.0) — so kept
channel `c` receives the data and type of file channel `chbegin + c` under
the name of file channel `c`.

For any `chbegin > 0` every returned channel is mislabelled. The damage is
not limited to the label: `init`'s "channel name hunt" (`deepdata.cpp:378`)
derives `z_channel`, `zback_channel`, `alpha_channel` and each channel's
associated alpha from those names, so `DeepData::z_channel()`,
`opaque_z()` and the alpha-aware operations (`sort`, `merge_overlaps`,
`occlusion_cull`) all act on the wrong channels for a subset read. Reading
`{Z, ZBack}` out of an `{R, G, B, A, Z, ZBack}` file (`chbegin = 4`) returns
the depth data named "R" and "G", with no Z channel found at all.

Reachable from OpenImageIO's own Python bindings:
`ImageInput.read_native_deep_scanlines(subimage, miplevel, ybegin, yend, z,
chbegin, chend)` with `chbegin > 0`. Whole-image deep reads
(`read_native_deep_image`, and any call with `chbegin == 0`) are unaffected.

The one-line fix is to offset the names the same way as the types:

```cpp
deepdata.init(npixels, nchans,
              cspan<TypeDesc>(&channeltypes[chbegin], nchans),
              cspan<std::string>(&m_spec.channelnames[chbegin], nchans));
```

(with the same change in the tiled variant). Found while binding deep region
reads for Rust; the binding sidesteps it by always reading every channel.

---

## Issue 14 — `ImageCache` ignores the constant-color metadata of textures made through the `make_texture` API

`ImageBufAlgo::make_texture` computes and writes `oiio:ConstantColor` and
`oiio:AverageColor` (`maketexture.cpp:1955` and `:1969` on `main`, `:1951`
and `:1965` in 3.1.16.0), but `ImageCache` only honours them when the file's
`Software` tag starts with "OpenImageIO" or "maketx"
(`imagecache.cpp:489-491`, same lines on `main` and 3.1.16.0):

```cpp
string_view software = spec.get_string_attribute("Software");
bool from_maketx     = Strutil::istarts_with(software, "OpenImageIO")
                   || Strutil::istarts_with(software, "maketx");
```

The `maketx` and `oiiotool` executables stamp a `Software` tag; the library
call sets none (the command line never reaches the output header except by
appending to `Exif:ImageHistory`, `maketexture.cpp:1632-1636` on `main`; its
only other use is the update-mode skip comparison at `:1188-1195`). So the same
constant texture behaves differently by provenance: made with `oiiotool
-otex`, `get_image_info("constantcolor")` answers; made with
`ImageBufAlgo::make_texture` from C++ or Python, the file carries the same
`oiio:ConstantColor` attribute and the query fails — and with it the
constant-color fast paths the cache builds on `is_constant_image`.

`average_color` partially masks the problem by falling back to sampling the
1×1 mip level; `constantcolor` and `constantalpha` have no fallback.

Reproduction: `ImageBufAlgo::make_texture(ImageBufAlgo::MakeTxTexture,
constant_buf, "t.tx", config)` with `maketx:constant_color_detect` set in the
config, then `ImageCache::get_image_info(ustring("t.tx"), 0, 0,
ustring("constantcolor"), TypeDesc("float[4]"), data)` — fails; the same
file with a `Software` tag beginning "maketx" or "OpenImageIO" — succeeds
(`testtex t.tx` shows both results in its default `gettextureinfo` pass; a
byte-patch of only the Software value flips the answer on an otherwise
identical file). The making half is reachable from Python's
`ImageBufAlgo.make_texture`; the query half is not — Python's `ImageCache`
leaves `get_image_info` commented out (`py_imagecache.cpp:113` on `main`).

Two candidate fixes: trust the `oiio:`-namespaced attributes on their own
name, or have the library `make_texture` stamp the `Software` tag the way
its CLI front-ends do. The second is the one consistent with upstream's own
intent: `pvt::check_texture_metadata_sanity` (`formatspec.cpp:1347` in
3.1.16.0, called from the TIFF and EXR readers and `imagecache.cpp:1051`)
deliberately squashes these attributes when `Software` does not qualify,
its comment citing files re-saved by other software — so the gate is
anti-tamper design, and trusting the attributes would have to touch that
too. (That squash, incidentally, erases the misspelled names
`oiio::ConstantColor`/`oiio::AverageColor` — double colon,
`formatspec.cpp:1364-1365` — so today only `oiio:SHA-1` is actually
squashed; arguably a second bug.) Found while binding the cache queries for
Rust; the binding works around it by stamping `Software` itself when the
caller sets none.

Related version note, not filed as an issue because the fix already shipped
in 3.1.16.0 (#5236): through 3.1.15, Targa thumbnail round trips come back
with red and blue exchanged (the writer dumps raw RGB top-down where the
reader decodes stored-BGR; fixed at `targaoutput.cpp:266-282` on `main`),
and a thumbnail 256 or larger on either axis is downsized *to* 256 — one
past what the TGA byte field holds — truncating the stored dimension to
zero (fixed at `:694-706` on `main`, which resizes to 255 and says why).

---

## Issue 15 — the UDIM aggregate in `get_image_info` can return uninitialised stack memory as a successful answer

`ImageCacheImpl::get_image_info`'s UDIM branch (`imagecache.cpp:3092-3135`
on `main` and 3.1.16.0) aggregates a query over the pattern's concrete
tiles: the first tile's answer lands in `char* common = OIIO_ALLOCA(char,
size)`, later tiles are compared against it, and at the end `memcpy(data,
common, size); return true;`.

Tiles whose concrete file cannot be opened are skipped —
`if (concretefile && !concretefile->broken())` at `:3109` has no else — and
skipping does not clear `first`. A UDIM set is inventoried by directory
listing alone (`udim_setup`, `:4142-4186`), so a tile can be present at
inventory time and unreadable at query time (corrupt, or deleted in
between). When *every* populated tile is skipped that way, `first` is still
true at `:3132` and the `memcpy` copies the never-written alloca buffer into
the caller's storage, returning success.

For integer queries that is a garbage answer presented as valid; for
`TypeString` queries the caller receives an uninitialised value it must
treat as a `const char*` — dereferencing it is undefined behaviour, and even
the integer case hands the caller an indeterminate value whose use is
undefined. Reachable from the public C++ API (`ImageCache::get_image_info`)
and any binding that exposes it. OpenImageIO's own Python binding comments
the direct method out (`py_imagecache.cpp:115`), but reaches the same code
indirectly: `ImageCache.get_pixels` sizes its allocation from an internal
`"channels"` query on the pattern (`py_imagecache.cpp:47-49`), so from
Python the garbage becomes an arbitrary allocation size.

The fix is one line: after the loop, `if (first) return false;` before the
`memcpy`.

Two adjacent notes found in the same review: the documentation for
`get_image_info` advertises the query name `"udim"`, but the implementation
compares against the interned `s_UDIM("UDIM")` (`imagecache.cpp:63`,
`:3084`) — the documented lowercase name is never answered and falls into
the aggregate above; and a UDIM query that *succeeds* can still leave the
skipped tiles' open-failure errors queued (`mark_broken` records each via
`imagecache().error`, `imagecache.cpp:1493-1501`; only the failure branch
at `:3116-3121` eats them), which a caller's next `geterror` misattributes.

---

## Issue 16 — `get_thumbnail` opens UDIM patterns as files, permanently poisoning their cache record, and reports brokenness only once

`ImageCacheImpl::get_thumbnail` (`imagecache.cpp:3444-3465`) calls
`file->open()` directly, bypassing `verify_file` — the only place carrying
the UDIM guard ("Can't really open a UDIM-like virtual file",
`:1588-1591`). Asked for a thumbnail of `tex.<UDIM>.exr`, it tries to open
the literal pattern, fails, and `mark_broken` sets the pattern's virtual
record broken permanently. Every later query on that pattern — subimage
counts, formats — then fails at the broken-file check (`:3073`, which
precedes the UDIM branches), and `exists`, answered earlier at `:3042-3052`
with the broken flag folded into its value, flips from true to false,
though the tile set on disk is intact: one read-only-looking probe poisons
the record until the file is invalidated.

Separately, `get_thumbnail` is the one query without a broken-file error:
`ImageCacheFile::open()` returns empty for an already-broken file with no
message (`:727-729`), and `get_thumbnail` maps that to plain `false`
(`"// indicates a broken file"`) where its siblings (`get_image_info`
`:3073-3080`, `get_imagespec`, `get_cache_dimensions`) re-issue "Invalid
image file …". So the first thumbnail query on a corrupt file errors and
every subsequent one is indistinguishable from "this format stores no
thumbnail".

---

## Issue 17 — the base `ImageInput::valid_file` silently closes the reader it is called on

`ImageInput::valid_file(const std::string&)` (`imageinput.cpp:133-149` on
`main`, `:116-131` in 3.1.16.0; byte-identical from v3.1.4.0-beta through
`main`) `const_cast`s `this` and, for plugins that do not override it,
probes by `self->open(filename)` + `self->close()` — or, when the plugin
supports `"ioproxy"`, via the IOProxy overload (`main` `:153-171`, 3.1.16.0
`:136-154`): `self->set_ioproxy(...)`, `self->open("")`, `self->close()`,
`self->ioproxy_clear()`. Called on a reader that is currently open — the
natural reading of a `const` member — it destroys that reader's open state,
format-dependently. Measured on 3.2.0.2dev (environment block above): on an
open ICO reader, `valid_file` returns true and every subsequent
`read_scanline` fails with no error set, `current_subimage()` now -1; on an
open Targa reader the next read crashes with an access violation; on an
open GIF reader `valid_file` returns *false* for a perfectly valid GIF — a
wrong answer, not just lost state — and the process then crashes. Nine
shipped readers have no override on 3.1.16.0 and `main` (targa, gif, ico,
iff, rla, cineon, dicom, r3d, softimage; pnm gained a non-destructive
override in 3.1.14.0 via #5203, ptex in 3.1.16.0 via #5265), so
`in = ImageInput::open("a.tga"); in->valid_file("b.tga"); in->read_...`
crashes. The library itself only ever calls `valid_file` on freshly created
probe instances inside `ImageInput::create` (`imageioplugin.cpp:686-687`,
`:764-766` on `main`). Probing on a scratch instance (or documenting
`valid_file` as unusable on open readers) would close it. Found while
binding the probe for Rust; the binding creates a throwaway instance of the
same plugin.

---

## Issue 18 — `missingcolor` fills repeat their first four values for lookups wider than four channels

Texture and environment lookups of more than four channels are split into
four-channel chunks by recursion (`texturesys.cpp:1531-1551` in 3.1.9.0,
same shape on `main`; `environment.cpp:303-322`). The recursion advances
`result`, the derivative pointers, and `options.firstchannel` per chunk —
but never `options.missingcolor`. `missing_texture` then writes
`result[c] = options.missingcolor[c]` for `c` in `0..nchannels` of *each
chunk* (`texturesys.cpp:1435-1437`), ignoring `firstchannel`.

So an eight-channel lookup with a `missingcolor` of `[c0..c7]` against a
missing or broken file — or a sparse UDIM set's unpopulated tile — returns
`[c0, c1, c2, c3, c0, c1, c2, c3]`. In bounds, reported as success, and
wrong for every channel past the fourth, which is precisely the multi-AOV
case where per-channel missing values (a distinctive sentinel per AOV)
would be used.

The fix is to make `missing_texture` honour the chunk position: either
advance a copy of the pointer alongside the recursion, or index
`missingcolor[options.firstchannel + c]` with the documented requirement
that the array covers the requested channel range.

---

## Issue 19 — a malformed UDIM-like filename aborts the caller: `udim_setup` compiles its tile regex unguarded

`ImageCacheFile::udim_setup` builds the tile-matching pattern from the
caller's filename and compiles it with `std::regex decoder(pat);` — no
`try`/`catch` (`imagecache.cpp:4159` in 3.1.9.0; identical at 3.1.16.0 and
on `main`; zero `catch` anywhere in `imagecache.cpp`).
`Filesystem::filename_to_regex` escapes only `.()[]{}` and the glob
characters (`filesystem.cpp:1123-1142`), so a basename that is invalid
regex after that partial escaping — a leading quantifier is the simplest:
`+<UDIM>.exr` becomes `+([0-9]{4})\.exr` — throws `std::regex_error` out
of the `ImageCacheFile` constructor, through `find_file`, and out of
whichever public API first touched the name: `get_image_info`,
`get_image_handle`, `TextureSystem::get_texture_handle`, a texture lookup.

Callers that cannot unwind — C bindings, `noexcept` boundaries, and every
binding built on cxx — terminate outright; C++ callers get an exception
from functions whose error contract is a `false` return plus `geterror()`.
And catching does not make it survivable: the throwing constructor runs
between `find_file`'s *manual* `m_files.lock_bin(filename)` /
`m_files.unlock_bin(bin)` pair (`imagecache.cpp:1542-1553`), so the
unwinding exception leaks the held file-cache bin lock — a caller that
catches and retries, or merely touches another file hashing to the same
bin, deadlocks. Measured through a catch-wrapped binding: the first query
returns false, the second blocks forever.
Notably, `get_directory_entries` guards its own compilation of the same
pattern (`filesystem.cpp:279-300`) — but its result is discarded and the
pattern recompiled bare at `:4159`.

The fix is a `try`/`catch` around the `decoder` construction (or
validating the pattern once, where `get_directory_entries` already does),
marking the file broken with a clear message instead of throwing; RAII for
the bin lock would close the deadlock half independently. Found while
binding texture handles for Rust; the binding refuses such names before
`find_file` can see them, because a barrier alone would trade the abort
for the deadlock.

---

## Open — `IBAprep` allocates the destination uninitialised, and something on 3.1.14 does not fill it

Not yet reproduced locally. Recorded because a property test found it on CI and
the mechanism is only half established.

`ImageBufAlgo::IBAprep` allocates a destination the caller left empty with

```cpp
dst->reset(spec, (prepflags & IBAprep_FILL_ZERO_ALLOC)
                     ? InitializePixels::Yes
                     : InitializePixels::No);
```

`imagebufalgo.cpp:258`. Nothing in the tree passes `IBAprep_FILL_ZERO_ALLOC`,
so every operation that allocates its own destination starts from uninitialised
heap, and anything it does not go on to write is returned to the caller as
image data with a success return.

The lines immediately after it are meant to close that for the channel axis:
when `IBAprep_CLAMP_MUTUAL_NCHANNELS` clamps the written range to the narrower
source, the range from there to `dst->nchannels()` is zeroed. On 3.1.12 that
holds — `mad` with a one-channel and a two-channel source produces a
two-channel destination and both channels come back written.

On the OpenImageIO 3.1.14 that CI builds against, it does not. `tests/property_test.rs`
reported:

```
mad reported success over the whole image but left 2.28e32 at element 47,
which it never wrote.
  a = 8x3, 1 channel, F16, origin 5,7
  b = 8x7, 2 channels, U16, origin 5,7
  mad(dst, a, Image(b), Constant([0.5])), whole image
```

Element 47 of an 8x7x2 buffer is x=7, y=2, channel 1 — exactly the channel the
clamp excludes and the clear is supposed to cover.

What is established: the allocation is uninitialised, nothing requests
otherwise, and the clearing that compensates is version-dependent enough that
one build of 3.1.14 did not do it. What is not: whether 3.1.14 differs here,
whether the Highway SIMD path (`mad_impl_hwy`, enabled by `OIIO_USE_HWY`) skips
the clear, or whether the clear itself has a condition that this shape misses.

Property testing has now hit the class three times, on different platforms,
different operations and different shapes. Twice through `mad` with two image
sources of unequal channel counts, with the channels beyond the narrower
source left holding heap; then through `copy` on macOS/3.1.14 — a one-channel
2x7 source at the origin copied into a pre-allocated 10x6 five-channel F16
destination at 100000,100000 returned success with `inf` in channel 4 of the
first pixel, memory the copy never wrote.

| build | a | b | destination | unwritten |
| --- | --- | --- | --- | --- |
| Windows, 3.1.14 | 8x3, 1 channel, F16 | 8x7, 2 channels, U16 | 8x7x2 | channel 1 |
| Linux debug, 3.1.14 | 5x10, 6 channels, U8 | 10x5, 3 channels, F16 | 6 channels | channels 3 and up |

Neither reproduces against 3.1.12: every mismatched pair tried there — 1 vs 2,
2 vs 1, 6 vs 3, 3 vs 6, 4 vs 1 — came back with every channel written.

This binding now refuses two image sources that disagree on channel count in
`mad`, so it does not depend on which OpenImageIO is linked. That closes it for
users of the crate but not upstream.

Worth settling before it is reported, because the general shape — an operation
that reports success while handing back heap it never wrote — is not specific
to `mad`, and `IBAprep`'s `InitializePixels::No` is what makes it reachable at
all.
