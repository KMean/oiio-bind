# copy() unguarded channel read — ready to file

Verified against `main` (0008a6f04), v3.2.2.0 and v3.1.19.0 on 2026-10-31, and
reproduced on locally built 3.2.2.0 and on vcpkg 3.1.12.0.

Paste the body from the `**Describe the bug**` line down. The reproduction is
inlined in a collapsed block at the end, because GitHub does not accept `.cpp`
attachments. The standalone source is
`contrib/copy_unguarded_channels_repro.cpp`, kept identical to the inlined copy.

This one is stronger than most of the drafts in `upstream-issues.md`: the
reproduction is sixty lines, needs no image files, and the project's own
`paste()` is the control, since it guards the identical loop.

---

**Title:** `bug: copy() writes destination channels the source does not have, taking the values from past the source's pixel`

---

**Describe the bug**

`ImageBufAlgo::copy()` into a pre-allocated destination with more channels
than the source returns `true` and fills the extra channels by reading past
the end of the source's pixel. The values that land there are the source's
following pixels, and at the end of the buffer they come from past the
allocation.

`src/libOpenImageIO/imagebufalgo_copy.cpp:141-148` on `main`, the kernel:

```c++
    parallel_image(roi, nthreads, [&](ROI roi) {
        ImageBuf::ConstIterator<S, D> s(src, roi);
        ImageBuf::Iterator<D, D> d(dst, roi);
        for (; !d.done(); ++d, ++s) {
            for (int c = roi.chbegin; c < roi.chend; ++c)
                d[c] = s[c];
        }
    });
```

`roi.chend` here is the destination's channel count, and `s[c]` is never
checked against the source's. `ImageBuf::ConstIterator::operator[]` does no
bounds checking of its own: it indexes a proxy built over the raw pixel
pointer, so `s[c]` for a channel the source does not have reads whatever
follows that pixel in memory.

The same file already shows the intended handling. `paste_`, a hundred lines
above, runs the identical loop with a guard, `:46`:

```c++
                if (c_dst >= 0 && c_dst < dst_nchans)
                    d[c_dst] = c < src_nchans ? s[c] : D(0);
```

**Why the clamp above it does not prevent this.** `copy()` does clamp, at
`:196`:

```c++
    roi.chend = std::min(roi.chend, src.nchannels());
```

but it clamps the ROI the caller passed in. A default-constructed `ROI` has
`chend == 0` and is undefined, so that line is a no-op on it. The
`if (!dst.initialized())` block that would otherwise use the clamped value is
skipped, because the destination is already allocated. `IBAprep` is then
called at `:207`, and its initialized-destination branch replaces the ROI
outright, `imagebufalgo.cpp:121-128`:

```c++
    if (dst->initialized()) {
        // Valid destination image.  Just need to worry about ROI.
        if (roi.defined()) {
            // Shrink-wrap ROI to the destination (including chend)
            roi = roi_intersection(roi, get_roi(dst->spec()));
        } else {
            // No ROI? Set it to all of dst's pixel window.
            roi = get_roi(dst->spec());
        }
```

So `roi.chend` arrives at the kernel as the destination's channel count. The
clamp survives only when the caller passes a defined ROI, in which case
`roi_intersection` takes the minimum and the problem does not arise. Calling
`copy(dst, src)` with no region is what reaches the unguarded loop.

Two smaller things in the same area, mentioned because they are one line each
and sit beside this:

- `copy()` discards `IBAprep`'s return value at `:207`, where `crop()` checks
  it at `:259`.
- `circular_shift_` has the same unguarded loop at `:360`, reached through
  `IBAprep(roi, &dst, &src)` at `:373`. I have not reproduced that one.

**OpenImageIO version and dependencies**

Reproduced on a locally built 3.2.2.0 release:

```
OIIO 3.2.2.0 | Windows/x86_64
    Build compiler: MSVS 1951 | C++17/199711
    HW features enabled at build: sse2,sse3,ssse3,sse41,sse42
    No CUDA support (disabled / unavailable at build time)
Dependencies: BZip2 1.0.8, DCMTK NONE, FFmpeg NONE, fmt 12.1.0, Freetype 2.13.3, GIF 5.2.2, Imath 3.2.2, JXL NONE, libdeflate 1.25, Libheif NONE, libjpeg-turbo 3.1.3, LibRaw NONE, libuhdr NONE, OpenColorIO 2.5.1, OpenCV NONE, OpenEXR 3.4.7, OpenJPEG 2.5.4, openjph 0.26.3, PNG 1.6.55, Ptex NONE, Ptex NONE, Robinmap 1.4.1, TBB NONE, TIFF 4.7.1, WebP 1.6.0, ZLIB 1.3.1, zstd 1.5.7
```

and identically on the released 3.1.12.0 from vcpkg, same machine and
compiler, with byte-for-byte the same output. `imagebufalgo_copy.cpp` carries
the same code at the same line numbers on `main` (0008a6f04) and v3.1.19.0,
so this is not new and not specific to either release line.

**To Reproduce**

Build and run the program at the end of this report. It needs no image files.

```
cl /std:c++17 /EHsc /utf-8 repro.cpp /I <prefix>\include ^
   /link /LIBPATH:<prefix>\lib OpenImageIO.lib OpenImageIO_Util.lib
```

or

```
c++ -std=c++17 repro.cpp -lOpenImageIO -lOpenImageIO_Util -o repro
```

It builds a 4x4 one-channel float source whose pixel `(x, y)` holds
`y * 4 + x`, so 0 through 15 in raster order, and a 4x4 five-channel float
destination pre-filled with `-1`. The distinct source values are the point: a
constant-filled source would make an over-read into the next pixel return the
same constant and hide the bug. It then calls `copy(dst, src)` with no ROI,
and `paste(pasted, 0, 0, 0, 0, src)` on the same shapes as a control. It exits
non-zero when a channel the source does not have comes back as anything other
than the sentinel or zero.

**Evidence**

`copy()` returns `true`, `geterror()` is empty, and each pixel's four extra
channels hold the next four source pixels:

```
Case 1  copy(dst[5ch], src[1ch]), no ROI
  copy returned true, geterror: ""
    (0,0) =        0        1        2        3        4
    (1,0) =        1        2        3        4        5
    (2,0) =        2        3        4        5        6
    (3,0) =        3        4        5        6        7
    ...
    (0,3) =       12       13       14       15        0
    (1,3) =       13       14       15        0        0
    (2,3) =       14       15        0        0 1.57101e-10
    (3,3) =       15        0        0 1.57101e-10 -1.61682e-27
```

Pixel `(0,0)` should be the source's `0` and then four channels the source
does not have. It is `0 1 2 3 4`: the source's value followed by its next four
pixels, read one element at a time past the end of that pixel.

The last rows are the part that leaves the allocation. The source holds 16
floats. At `(3,3)`, `s[1]` through `s[4]` are elements 16 through 19 of a
16-element buffer, and the two values that come back, `1.57101e-10` and
`-1.61682e-27`, are not in the image at all.

`paste()` on the same shapes, through its guarded loop:

```
Case 2  paste(dst[5ch], 0,0,0,0, src[1ch])  (control)
    (0,0) =        0       -1       -1       -1       -1
    (1,0) =        1       -1       -1       -1       -1
    ...
    (3,3) =       15       -1       -1       -1       -1
```

Every channel the source does not have is left as it was found.

Totals from the program's own check: 57 of the 64 extra-channel values came
from past the source's pixel, on both 3.2.2.0 and 3.1.12.0.

**The fix**

Guard the loop the way `paste_` does:

```c++
            for (int c = roi.chbegin; c < roi.chend; ++c)
                d[c] = c < src.nchannels() ? s[c] : D(0);
```

Clamping `roi.chend` after `IBAprep` rather than before would also work, and
would match what the clamp at `:196` was evidently meant to do. I have not
formed a view on which you would prefer, since the choice decides whether the
extra channels end up zeroed or left as the destination had them, and that is
a behavioural question rather than a correctness one.

Found while writing Rust bindings. The binding refuses this shape, so it is
closed for its own users, with a regression test that fails without the
refusal.

I am happy to open a pull request for the one-line change if that is more
useful than an issue, it would need me to sort out a CLA first, so say the
word if you would rather have the patch than the report.

---

<details>
<summary>copy_unguarded_channels_repro.cpp</summary>

```c++
// Reproducer: ImageBufAlgo::copy() writes destination channels the source
// does not have, taking the values from an unchecked read past the source's
// pixel.
//
// ImageBufAlgo::paste(), a hundred lines above copy_() in the same file,
// guards the identical loop with `c < src_nchans ? s[c] : D(0)`. copy_()
// does not. This program runs both over the same shapes so the asymmetry is
// visible side by side.
//
// Build (Windows, MSVC), adjusting the OIIO prefix:
//
//   cl /std:c++17 /EHsc /utf-8 copy_unguarded_channels_repro.cpp ^
//      /I <oiio-prefix>\include ^
//      /link /LIBPATH:<oiio-prefix>\lib OpenImageIO.lib
//
// Build (Linux/macOS):
//
//   c++ -std=c++17 copy_unguarded_channels_repro.cpp -lOpenImageIO -o repro
//
// Exits non-zero if the leftover channels are not copies of the source's
// only channel, i.e. if the bug is present.

#include <OpenImageIO/imagebuf.h>
#include <OpenImageIO/imagebufalgo.h>
#include <OpenImageIO/imageio.h>

#include <cstdio>
#include <string>
#include <vector>

using namespace OIIO;

namespace {

constexpr int WIDTH    = 4;
constexpr int HEIGHT   = 4;
constexpr int DST_CHAN = 5;
constexpr float SENTINEL = -1.0f;

// A one-channel source whose every pixel holds a different, predictable
// value: pixel (x, y) holds y * WIDTH + x, so 0 .. 15 in raster order.
//
// The distinct values are the point. If the source were filled with a single
// constant, an over-read into the next pixel would return that same constant
// and the bug would be invisible.
ImageBuf
make_source()
{
    ImageBuf src(ImageSpec(WIDTH, HEIGHT, 1, TypeDesc::FLOAT));
    for (int y = 0; y < HEIGHT; ++y) {
        for (int x = 0; x < WIDTH; ++x) {
            float v = float(y * WIDTH + x);
            src.setpixel(x, y, &v, 1);
        }
    }
    return src;
}

ImageBuf
make_destination()
{
    ImageBuf dst(ImageSpec(WIDTH, HEIGHT, DST_CHAN, TypeDesc::FLOAT));
    std::vector<float> fill(DST_CHAN, SENTINEL);
    ImageBufAlgo::fill(dst, fill);
    return dst;
}

void
report(const char* what, const ImageBuf& buf)
{
    std::printf("  %s:\n", what);
    for (int y = 0; y < HEIGHT; ++y) {
        for (int x = 0; x < WIDTH; ++x) {
            float px[DST_CHAN] = {};
            buf.getpixel(x, y, px, DST_CHAN);
            std::printf("    (%d,%d) =", x, y);
            for (int c = 0; c < DST_CHAN; ++c)
                std::printf(" %8g", px[c]);
            std::printf("\n");
        }
    }
}

}  // namespace

int
main()
{
    // The headers compiled against, and the library actually linked. A
    // machine with more than one OpenImageIO on it can easily measure the
    // wrong one.
    std::printf("compiled against OIIO %s; linked lib reports \"%s\" (%d)\n\n",
                OIIO_VERSION_STRING,
                std::string(get_string_attribute("version")).c_str(),
                openimageio_version());

    const ImageBuf src = make_source();

    // --- Case 1: copy, 1-channel source into a pre-allocated 5-channel
    // destination, no ROI. ---
    std::printf("Case 1  copy(dst[5ch], src[1ch]), no ROI\n");
    ImageBuf dst = make_destination();
    if (!ImageBufAlgo::copy(dst, src)) {
        std::printf("FATAL: copy failed: %s\n", dst.geterror().c_str());
        return 2;
    }
    std::printf("  copy returned true, geterror: \"%s\"\n",
                dst.geterror().c_str());
    report("result", dst);

    // --- Case 2: paste, the same shapes, for contrast. ---
    std::printf("\nCase 2  paste(dst[5ch], 0,0,0,0, src[1ch])  (control)\n");
    ImageBuf pasted = make_destination();
    if (!ImageBufAlgo::paste(pasted, 0, 0, 0, 0, src)) {
        std::printf("FATAL: paste failed: %s\n", pasted.geterror().c_str());
        return 2;
    }
    report("result", pasted);

    // --- Verdict, read off case 1. ---
    //
    // Channel 0 must be the source. Channels 1..4 are channels the source
    // does not have: whatever copy decides to put there, it cannot be read
    // from the source, so the only defensible values are the sentinel left
    // in place or zero.
    std::printf("\n");
    int bad = 0;
    for (int y = 0; y < HEIGHT; ++y) {
        for (int x = 0; x < WIDTH; ++x) {
            float px[DST_CHAN] = {};
            dst.getpixel(x, y, px, DST_CHAN);

            const float expected0 = float(y * WIDTH + x);
            if (px[0] != expected0) {
                std::printf("channel 0 at (%d,%d) is %g, expected %g\n", x, y,
                            px[0], expected0);
                ++bad;
            }
            for (int c = 1; c < DST_CHAN; ++c) {
                if (px[c] != SENTINEL && px[c] != 0.0f) {
                    std::printf(
                        "(%d,%d) channel %d = %g -- the source has no channel "
                        "%d; this is the value %d element(s) past its pixel\n",
                        x, y, c, px[c], c, c);
                    ++bad;
                }
            }
        }
    }

    if (bad) {
        std::printf(
            "\nFAIL: %d destination value(s) came from past the source's "
            "pixel.\n", bad);
        return 1;
    }
    std::printf("\nPASS: channels the source does not have were left alone.\n");
    return 0;
}
```

</details>
