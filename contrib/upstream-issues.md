# Draft reports for OpenImageIO

Nine findings from building Rust bindings against OpenImageIO 3.1/3.2.

Issues 1 and 2 have self-contained reproductions, so a maintainer running one
is never shown a second unrelated API behaviour:

- `contrib/span_tiled_read_repro.cpp` — issue 1
- `contrib/span_scanline_origin_repro.cpp` — issue 2

Both use only public OpenImageIO API — no Rust and no binding code — and exit
non-zero when two overloads of the same call disagree, given the same
`ImageSpec` and the same buffer.

Issues 4–9 were found by reading the source while binding it, and are stated
as code review rather than as runnable reproductions. Where a reproduction is
cheap it is noted in the issue.

**Issues or pull requests.** The repository's `.github/ISSUE_TEMPLATE/bug_report.md`
ends with: "IF YOU ALREADY HAVE A CODE FIX: There is no need to file a
separate issue, please just go straight to making a pull request." Issues 3,
4, 5, 7, 8 and 9 each name a one- or two-line fix, so they belong as pull
requests, not issues. Issue 2 is genuinely a question about intended
behaviour, so it is an issue. Issue 6 could be either.

`CONTRIBUTING.md` asks for the version, platform, compiler, and a repro others
can run. The `bug:` title prefix comes from the bug-report template's
`title: "bug:"` prefill rather than from `CONTRIBUTING.md`, whose prefix list
covers commits and pull requests — where it endorses a parenthesised
subcategory, e.g. `fix(IBA):`, and names `IBA` for `ImageBufAlgo` explicitly.

Issues 4–7 are out-of-bounds reads and writes. `SECURITY.md` invites judgement
here, and each of those is an API-misuse hazard reachable only from a caller's
own arguments rather than from untrusted file data, so a normal issue or pull
request is the right channel.

Shared environment block, applying to every issue below:

```
OIIO 3.2.0.2dev | Windows/x86_64
    Build compiler: MSVS 1951 | C++17/199711
Dependencies: fmt 12.1.0, Imath 3.2.2, OpenColorIO 2.5.1, OpenEXR 3.4.7,
              TIFF 4.7.1, ZLIB 1.3.1, libjpeg-turbo 3.1.3, PNG 1.6.55
```

Also reproduced identically on the released 3.1.12.0 (vcpkg, same machine and
compiler), so this is not new in 3.2.

---

## Issue 1

**Title:** `bug: ImageInput::read_image() with an image_span fails silently for tiled images with partial edge tiles`

**Describe the bug**

`ImageInput::read_image(subimage, miplevel, chbegin, chend, format, image_span)`
returns `false` whenever a tiled image's width or height is not an exact
multiple of the tile size. No error is recorded, so `geterror()` returns an
empty string. The pointer overload reads the same files correctly.

Since the destination buffer is exactly `width * height * nchannels` values,
there is no buffer size a caller could pass that would work, so tiled images
with partial edge tiles cannot be read through the `image_span` overload at
all. Most real tiled images have partial edge tiles.

I expected the `image_span` overload to read the same images the pointer
overload reads, and on failure to record an error explaining why.

**OpenImageIO version and dependencies**

(paste the environment block above)

**To Reproduce**

Build and run the attached `span_tiled_read_repro.cpp`. It writes tiled OpenEXR files
with `write_image`, then reads each back three ways. Output on 3.2.0.2dev:

```
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

32x24, 16x16 tiles (PARTIAL edge tiles)   -> same as above
40x24, 16x16 tiles (PARTIAL edge tiles)   -> same as above
```

Both `image_span` forms are covered: the `image_span<std::byte>` plus
`TypeDesc::FLOAT` overload with strides spelled out, and the typed
`image_span<float>` overload with OpenImageIO computing every stride itself.
They fail identically, so the result does not depend on the caller's stride
arithmetic. Images whose dimensions are an exact multiple of the tile size are
unaffected.

I have not tried to identify the cause in the reading code, so the summary
above is only what is observable from outside.

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

Passing `0..4` instead is accepted, which suggests the `image_span` overload
takes rows relative to the data window origin. However, reading both files
back shows the same data window origin but **different pixel values**, so the
accepted call does not write the same data to the same rows — it writes
something else. That is the part I would flag: a caller who responds to the
rejection by subtracting the origin gets no error and incorrect output.

I could not tell from the documentation which coordinate convention the
`image_span` overload intends, so I do not know whether the bug is the
rejection, the acceptance, or both.

**OpenImageIO version and dependencies**

(paste the environment block above)

**To Reproduce**

The attached `span_scanline_origin_repro.cpp`. Output on 3.2.0.2dev:

```
data window origin y=5, writing scanlines 5..9
  write_scanlines(image_span), rows 5..9 : FAILED, error: write_scanlines: Invalid scanline range 5-9
  write_scanlines(image_span), rows 0..4 : ok
  write_scanlines(pointer),   rows 5..9 : ok
  read back: origins 5 and 5 (same), pixels DIFFER
```

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

For context, the macros came from #2261 and PR #2641; nothing in either
discussion touches parenthesisation, and the idiom preferred there was the raw
`#if OIIO_VERSION <= 20008`, which is presumably why this went unnoticed.

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

The block immediately below the dispatch is the same in all three, and in `max`
it is unreachable. Its guard, in full, is:

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

**Title:** `ImageBufAlgo::isConstantColor` heap overflow when `roi.chbegin > 0`

`imagebufalgo_compare.cpp`, in `isConstantColor_`:

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
and a default-constructed ROI

`ImageBufAlgo::histogram` validates the channel, the bin count and the range,
but a defined `roi` is passed to `histogram_impl` verbatim — only an *undefined*
one is replaced by `get_roi(src.spec())`. The kernel then does:

```c++
if (ignore_empty) {
    bool allblack = true;
    for (int c = roi.chbegin; c < roi.chend; ++c)
        allblack &= (a[c] == 0.0f);
```

`ROI`'s four-argument constructor defaults `chend` to 10000, so the natural
`ROI(x0, x1, y0, y1)` makes this read 10000 channels out of every pixel.
`ConstDataArrayProxy::operator[]` does no bounds check.

Every other statistic in this file clamps `roi.chend` to `src.nchannels()`;
`histogram` appears to be the one that was missed.

## Issue 7 — `computePixelHashSHA1` indexes its block results by the wrong origin

**Title:** `ImageBufAlgo::computePixelHashSHA1` writes past `results` when
`blocksize > 0` and the ROI does not start at the image's first row

```c++
int nblocks = (roi.height() + blocksize - 1) / blocksize;
std::vector<std::string> results(nblocks);
parallel_for_chunked(roi.ybegin, roi.yend, blocksize,
                     [&](int64_t ybegin, int64_t yend) {
    int64_t b   = (ybegin - src.ybegin()) / blocksize;  // block number
    ...
    results[b]  = simplePixelHashSHA1(src, "", broi);
}, nthreads);
```

`results` is sized from the ROI's height, but the block index is computed from
the *image's* first row. The chunked loop walks `[roi.ybegin, roi.yend)`, so the
two agree only when `roi.ybegin == src.ybegin()`.

With `src.ybegin() == 0`, `roi.ybegin == 10`, `roi.yend == 30` and
`blocksize == 4`: `nblocks == 5`, while `b` takes the values 2, 3, 4, 5, 6.
`results[5]` and `results[6]` are past the end, and each writes a `std::string`.

The index should be `(ybegin - roi.ybegin) / blocksize`.

Found while binding these for Rust. The binding does not expose `blocksize` at
all, partly for this and partly because the two paths give different digests
for identical pixels.

## Issue 8 — the colour engine corrupts channels past the fourth

**Title:** `colorconvert` and friends write `0.5 + 10 * src` into channels 4 and
above instead of copying them

In `color_ocio.cpp`, at the end of `colorconvert_impl`:

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
and offsets them instead. It has the shape of debugging code that was committed
by accident.

Every operation built on this engine is affected — `colorconvert`,
`colormatrixtransform`, `ociolook`, `ociodisplay`, `ociofiletransform`,
`ocionamedtransform` — for any image with more than four channels, which in
practice means most multi-AOV EXRs. It only triggers in the generic template
path with a destination distinct from the source, so the RGBA fast path hides
it, which is presumably why it has lasted.

Measured on 3.1.9: converting a six-channel image whose fifth and sixth
channels hold 0.125 and 0.875 gives 1.75 and 9.25, exactly `0.5 + 10 * x`.
The line is identical on 3.2.0.2dev.

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

The null check is fifteen lines below the first dereference. `colorconfig`
defaults to `nullptr`, and an empty `fromspace` is the documented way to say
"use the source's own colour space", so the combination is neither exotic nor
discouraged — it is the default call.

`ColorConfig::resolve` dereferences `getImpl()`, so this is an immediate crash
rather than a bad answer.

`ociodisplay` does the same job in the correct order, which is what the fix
should look like: hoist the

```c++
if (!colorconfig)
    colorconfig = &ColorConfig::default_colorconfig();
```

above the two `from`/`to` resolution blocks.

Found while binding these for Rust. The binding always passes a real
ColorConfig, so it cannot reach this.
