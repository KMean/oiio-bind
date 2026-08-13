# Draft reports for OpenImageIO

Three findings from building Rust bindings against OpenImageIO 3.1/3.2.

Each has its own self-contained reproduction, so a maintainer running one is
never shown a second unrelated API behaviour:

- `contrib/span_tiled_read_repro.cpp` — issue 1
- `contrib/span_scanline_origin_repro.cpp` — issue 2

Both use only public OpenImageIO API — no Rust and no binding code — and exit
non-zero when two overloads of the same call disagree about the same file.

File at <https://github.com/AcademySoftwareFoundation/OpenImageIO/issues>
using the "Bug report" template. Their `CONTRIBUTING.md` asks for the version,
platform, compiler, and a repro others can run; the title convention is a
`bug:` prefix. Issues 1 and 2 are separate reports because they are different
calls with different fixes. Issue 3 is a one-line change and is friendlier as
a pull request than an issue.

Shared environment block for both issues:

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

**Title:** `fix: parenthesise OIIO_VERSION_GREATER_EQUAL and friends`

`oiioversion.h` defines the version tests without wrapping the expression:

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
`#error` is unreachable. We shipped that guard believing it worked; it never
fired, and a genuinely too-old OpenImageIO instead failed later with a
confusing C++ overload error.

Wrapping the expression in parentheses fixes it and cannot break any existing
correct use:

```c
#define OIIO_VERSION_GREATER_EQUAL(major,minor,patch) \
                        (OIIO_VERSION >= OIIO_MAKE_VERSION(major,minor,patch))
```

`OIIO_VERSION_LESS` and any sibling macros want the same treatment.

## Issue 4 — `ImageBufAlgo::max` widens the channel range where `min` narrows it

**Title:** `ImageBufAlgo::max(dst, A, B)` reads and writes out of bounds when
channel counts differ

`imagebufalgo_pixelmath.cpp`, in the image-against-image branch of `max`:

```c++
roi.chend = std::max(roi.chend, std::max(A.nchannels(), B.nchannels()));
```

`min`, twenty lines of otherwise identical code earlier in the same file, has:

```c++
roi.chend = std::min(roi.chend, std::min(A.nchannels(), B.nchannels()));
```

The block immediately below the dispatch is the same in both, and in `max` it
is unreachable: its guard is `roi.chend < origroi.chend`, which cannot hold
after a `std::max` against `origroi`'s own bound. That block's
`OIIO_ASSERT(roi.chend <= dst.nchannels())` records the invariant the widening
breaks.

Two consequences, both after `IBAprep` has already clamped `roi` to what the
buffers hold:

1. With `A.nchannels() != B.nchannels()`, the kernel evaluates `a[c]` or `b[c]`
   for `c` beyond the shorter input. `ImageBuf::ConstIterator::operator[]`
   constructs a proxy over the pixel's base pointer and indexes it with no
   bounds check, so this reads adjacent pixel memory.
2. With `dst` pre-allocated narrower than the inputs — `max(dst=3ch, A=4ch,
   B=4ch)` — `IBAprep` intersects `roi.chend` down to 3 and the widening puts
   it back to 4, so the kernel *writes* `r[3]` past the end of each destination
   pixel.

No ROI the caller supplies can prevent either, because the widening ignores the
incoming value whenever it is smaller.

Reproduced on 3.1.9 and on 3.2.0.2dev (`main`, August 2026); the line is
unchanged between them.

The fix is the one-word one, matching `min`:

```c++
roi.chend = std::min(roi.chend, std::min(A.nchannels(), B.nchannels()));
```

which also makes the surplus-channel block below it reachable, as it is in
`min`.

Found while binding `max` for Rust. Until it is fixed, the binding refuses
unequal channel counts and destinations narrower than the inputs, since it
cannot otherwise keep its safety promise.
