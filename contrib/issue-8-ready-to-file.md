# Issue 8 — ready to file

Verified against current `main` (1ff2c7bc2), v3.2.2.0 and v3.1.19.0, and
reproduced on a locally built 3.2.2.0 release, on 2026-10-30.

GitHub does not accept `.cpp` attachments, so the reproduction is inlined
below in a collapsed block. Paste the whole thing from the `**Title:**`
line down.

**A note on issues versus pull requests.** The bug template says to go
straight to a pull request when you already have the fix, and the fix here is
one line. A pull request to this project needs a signed CLA, which is not in
place. Filing as an issue, with the fix stated and a pull request offered, is
the honest version of that. Delete the closing offer if the CLA is settled and
this goes up as a pull request instead.

---

**Title:** `bug: colorconvert and the other OCIO operations write 0.5 + 10 * src into channels past the fourth instead of copying them`

---

**Describe the bug**

`ImageBufAlgo::colorconvert` and every other operation built on the same
engine corrupt channels past the fourth. The code that is supposed to copy
them states the intent in its own comment and then does something else.

`src/libOpenImageIO/color_ocio.cpp:3089-3097` on `main`, the last statement of
the per-scanline body in `colorconvert_impl`:

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

`channelsToCopy` is `std::min(4, roi.nchannels())` (`:3028`), so with the
default ROI this is every channel from index 4 upward. Those channels come out
as `0.5 + 10 * source` rather than as the source value.

I expected them to be copied unaltered, which is what the comment says, and
what the public header promises six times — once for each affected operation:

- `src/include/OpenImageIO/imagebufalgo.h:2117` — `colorconvert`
- `:2190` — `colormatrixtransform`
- `:2222` — `ociolook`
- `:2271` — `ociodisplay`
- `:2330` — `ociofiletransform`
- `:2366` — `ocionamedtransform`

each reading "Any additional channels will be simply copied unaltered."

All six funnel into `colorconvert_impl` through the processor overload of
`colorconvert`, so all six are affected.

Three conditions have to hold together, which is presumably why this has
lasted:

1. more than four channels in the ROI;
2. the destination is a different `ImageBuf` from the source — the guard tests
   `&R != &A`;
3. the processor is not a no-op. An identity conversion returns earlier
   through `ImageBufAlgo::copy` (`:3194-3197`), and an in-place no-op returns
   at `:3186-3187`.

The RGB/RGBA fast path does not hide it: `colorconvert_impl_float_rgba` is
gated on both buffers having exactly four channels (`:3206-3208`), so it never
carries an image that would reach this block.

In practice condition 1 means most multi-AOV EXRs, so this affects a common
case rather than an exotic one. The Evidence section below shows it on a
stock OpenEXR test image whose depth, disparity, motion-vector and mask
channels are all corrupted by a single `--colorconvert`.

**This is not a guess about intent.** The block entered the file in PR #2987,
"Clarify behavior of color conversion on image with > 4 channels", commit
`1b884e123485eef66b27c110f0374bd91bcafda6` (2021-05-31). The line was
`0.5 + 10 * a[c]` in that diff as merged, and the *same* pull request added
the "copied unaltered" sentence to `imagebufalgo.h`. `git log -L` shows the
line untouched since. It has shipped in every release from v2.3.7.2 onward —
about five years.

There is no test or documentation anywhere in the repository that refers to
this expression: a search for `0.5 + 10` across `src/` and `testsuite/`
returns this one line.

**OpenImageIO version and dependencies**

```
OIIO 3.2.2.0 | Windows/x86_64
    Build compiler: MSVS 1951 | C++17/199711
    HW features enabled at build: sse2,sse3,ssse3,sse41,sse42
    No CUDA support (disabled / unavailable at build time)
Dependencies: BZip2 1.0.8, DCMTK NONE, FFmpeg NONE, fmt 12.1.0, Freetype 2.13.3, GIF 5.2.2, Imath 3.2.2, JXL NONE, libdeflate 1.25, Libheif NONE, libjpeg-turbo 3.1.3, LibRaw NONE, libuhdr NONE, OpenColorIO 2.5.1, OpenCV NONE, OpenEXR 3.4.7, OpenJPEG 2.5.4, openjph 0.26.3, PNG 1.6.55, Ptex NONE, Ptex NONE, Robinmap 1.4.1, TBB NONE, TIFF 4.7.1, WebP 1.6.0, ZLIB 1.3.1, zstd 1.5.7
```

The quoted block is byte-identical in current `main`, v3.2.2.0 and v3.1.19.0.
Line numbers: `main` and v3.2.2.0 `color_ocio.cpp:3089-3097` with the
assignment at `:3096`; v3.1.19.0 `:2557-2565` with the assignment at `:2564`.

**To Reproduce**

Build and run the program at the end of this report. Public
OpenImageIO API only. It fills a six-channel float image whose channels 4 and
5 hold 0.125 and 0.875, converts ACEScg to sRGB - Display into a *distinct*
destination, and prints the result, with three controls.

It prints both the headers it compiled against and the version the linked
library reports, so the measurement cannot be against a different build by
accident.

```
compiled against OIIO 3.2.2.0; linked lib reports "3.2.2.0" (30202)
OCIO config: ocio://default
config has 27 color spaces
processor ACEScg -> sRGB - Display: isNoOp=0 (real transform)
processor ACEScg -> ACEScg: isNoOp=1

input fill:
  6 channels, as filled              c0=0.18      c1=0.18      c2=0.18      c3=1         c4=0.125     c5=0.875

Case 1  6ch, DISTINCT dst, real transform  (expect c4/c5 unchanged)
  result                             c0=0.461355  c1=0.461355  c2=0.461355  c3=1         c4=1.75      c5=9.25
  expected (a copy)                  c4=0.125     c5=0.875
  expected if 0.5+10*x               c4=1.75      c5=9.25
  VERDICT: copy=0  0.5+10*x=1  -> CORRUPTION REPRODUCED

Case 2  4ch control, DISTINCT dst, real transform  (no leftover channels)
  result                             c0=0.461355  c1=0.461355  c2=0.461355  c3=1

Case 3  6ch, DISTINCT dst, IDENTITY transform  (early-outs via copy)
  result                             c0=0.18      c1=0.18      c2=0.18      c3=1         c4=0.125     c5=0.875

Case 4  6ch, IN-PLACE dst==src, real transform  (&R==&A guard)
  result                             c0=0.461355  c1=0.461355  c2=0.461355  c3=1         c4=0.125     c5=0.875
```

0.125 and 0.875 come back as 1.75 and 9.25, which are exactly
`0.5 + 10 * 0.125` and `0.5 + 10 * 0.875`. Both are exactly representable in
float and half, so the equality is exact rather than approximate; an integer
destination would quantise or clamp instead.

The three controls all behave correctly, which confirms the trigger is the
conjunction above and not something else.

**Evidence**

This is not only a synthetic case. Here it is on a real multi-AOV frame from
the OpenEXR test images — `Beachball/singlepart.0001.exr`, 911x876, 20
channels, half, with a non-zero data window origin. No C++ at all:

```
oiiotool singlepart.0001.exr --colorconvert ACEScg "sRGB - Display" \
         --cut 1x1+1100+680 -d float -o conv.exr
oiiotool --dumpdata conv.exr
```

Every one of the sixteen channels past the fourth comes back as
`0.5 + 10 * original`, at pixel (1100, 680):

| channel | original | after `--colorconvert` | `0.5 + 10 * original` |
| --- | ---: | ---: | ---: |
| `Z` | 9.000000 | 90.500000 | 90.500000 |
| `disparityL.x` | 37.031250 | 370.812500 | 370.812500 |
| `disparityL.y` | 0.000040 | 0.500401 | 0.500401 |
| `disparityR.x` | -37.000000 | -369.500000 | -369.500000 |
| `disparityR.y` | -0.000053 | 0.499473 | 0.499473 |
| `forward.left.u` | 43.375000 | 434.250000 | 434.250000 |
| `forward.left.v` | -41.718750 | -416.687500 | -416.687500 |
| `forward.right.u` | 43.375000 | 434.250000 | 434.250000 |
| `forward.right.v` | -41.750000 | -417.000000 | -417.000000 |
| `left.R` | 0.000000 | 0.500000 | 0.500000 |
| `left.G` | 0.000000 | 0.500000 | 0.500000 |
| `left.B` | 0.500000 | 5.500000 | 5.500000 |
| `left.A` | 1.000000 | 10.500000 | 10.500000 |
| `left.Z` | 9.007812 | 90.578125 | 90.578125 |
| `whitebarmask.left.mask` | 0.000000 | 0.500000 | 0.500000 |
| `whitebarmask.right.mask` | 0.700195 | 7.501953 | 7.501953 |

Sixteen of sixteen, and the same at every other pixel I checked. A depth of
9 becomes 90.5, an alpha of 1 becomes 10.5, and the second eye of the stereo
pair — carried in `left.R`/`left.G`/`left.B` — is scaled along with
everything else, because those channels are past the fourth too.

The same thing on a synthetic file, for a minimal case:

```
// six.exr :    2 x    1, 6 channel, float openexr
float 0[1][2][6] =
{
  { /* (0, 0): */ { 0.461354554, 0.461354613, 0.461354613, 1.000000000, 1.750000000, 9.250000000 },
    /* (1, 0): */ { 0.461354554, 0.461354613, 0.461354613, 1.000000000, 1.750000000, 9.250000000 } },
};
```

Channels 4 and 5 were 0.125 and 0.875 before the conversion.

**The fix**

What the comment already says:

```c++
r[c] = a[c];
```

One caveat on scope, in case it affects how you want to fix it:
`channelsToCopy` is a *count* of ROI channels but is compared against the
*absolute* `roi.chend`, and the load and store loops at `:3050-3051` and
`:3087-3088` index from 0 and ignore `roi.chbegin`. With `roi.chbegin > 0` the
boundary therefore moves. Everything above is stated for the default ROI,
which is what the reproduction uses; I have not tried to work out what the
intended behaviour is for a shifted channel range, and have deliberately not
folded it into this report.

Found while writing Rust bindings. The binding currently repairs those
channels from the source after each colour call and has a regression test that
fails without the repair.

I am happy to open a pull request for the one-line change if that is more
useful than an issue — it would need me to sort out a CLA first, so say the
word if you would rather have the patch than the report.

---

<details>
<summary>colorconvert_leftover_repro.cpp</summary>

```c++
// Reproducer: ImageBufAlgo::colorconvert() corrupts channels past the fourth
// when the destination is a different ImageBuf and the transform is not a no-op.
//
// src/libOpenImageIO/color_ocio.cpp, colorconvert_impl():
//     if (channelsToCopy < roi.chend && (&R != &A)) {
//         // If there are "leftover" channels, just copy them
//         // unaltered from the source.
//         ...
//         r[c] = 0.5 + 10 * a[c];     <-- not a copy
//     }
//
// Public API only. Prints channels 4 and 5 for several cases.

#include <OpenImageIO/color.h>
#include <OpenImageIO/imagebuf.h>
#include <OpenImageIO/imagebufalgo.h>
#include <OpenImageIO/imageio.h>

#include <cstdio>
#include <cstdlib>
#include <string>
#include <vector>

using namespace OIIO;

static const int XRES = 4;
static const int YRES = 2;

// Channel fill values. Channels 4 and 5 are the ones under test.
static const float FILL[6] = { 0.18f, 0.18f, 0.18f, 1.0f, 0.125f, 0.875f };

static ImageBuf
make_buf(int nchannels)
{
    ImageSpec spec(XRES, YRES, nchannels, TypeDesc::FLOAT);
    if (nchannels == 6)
        spec.channelnames = { "R", "G", "B", "A", "Z", "mask" };
    ImageBuf buf(spec);
    if (!ImageBufAlgo::fill(buf, cspan<float>(FILL, FILL + nchannels))) {
        std::printf("FATAL: fill failed: %s\n", buf.geterror().c_str());
        std::exit(1);
    }
    return buf;
}

static void
report(const char* label, const ImageBuf& buf, int nchannels)
{
    std::vector<float> px(nchannels, 0.0f);
    buf.getpixel(0, 0, px.data(), nchannels);
    std::printf("  %-34s", label);
    for (int c = 0; c < nchannels; ++c)
        std::printf(" c%d=%-9.6g", c, px[c]);
    std::printf("\n");
}

int
main()
{
    // Headers we compiled against, and the library we actually linked, so the
    // report cannot be accused of measuring the wrong build.
    std::printf("compiled against OIIO %s; linked lib reports \"%s\" (%d)\n",
                OIIO_VERSION_STRING,
                std::string(get_string_attribute("version")).c_str(),
                openimageio_version());

    ColorConfig config;
    if (config.has_error()) {
        std::printf("FATAL: ColorConfig: %s\n", config.geterror().c_str());
        return 1;
    }
    std::printf("OCIO config: %s\n", config.configname().c_str());

    const char* FROM = "ACEScg";
    const char* TO   = "sRGB - Display";

    // Confirm both spaces really exist in whatever config we got.
    std::vector<std::string> names = config.getColorSpaceNames();
    std::printf("config has %d color spaces\n", (int)names.size());
    bool have_from = false, have_to = false;
    for (const std::string& n : names) {
        if (n == FROM)
            have_from = true;
        if (n == TO)
            have_to = true;
    }
    if (!have_from || !have_to) {
        std::printf("FATAL: need both \"%s\" (found=%d) and \"%s\" (found=%d).\n"
                    "Available:\n",
                    FROM, (int)have_from, TO, (int)have_to);
        for (const std::string& n : names)
            std::printf("    %s\n", n.c_str());
        return 1;
    }

    // A real, non-identity transform. Fail loudly if it is a no-op, because a
    // no-op would return early through ImageBufAlgo::copy() and prove nothing.
    ColorProcessorHandle proc = config.createColorProcessor(FROM, TO);
    if (!proc) {
        std::printf("FATAL: createColorProcessor(%s -> %s) failed: %s\n", FROM,
                    TO, config.geterror().c_str());
        return 1;
    }
    if (proc->isNoOp()) {
        std::printf("FATAL: %s -> %s is a no-op; test would be vacuous.\n", FROM,
                    TO);
        return 1;
    }
    std::printf("processor %s -> %s: isNoOp=0 (real transform)\n", FROM, TO);

    // And an identity transform, for the control case.
    ColorProcessorHandle ident = config.createColorProcessor(FROM, FROM);
    if (!ident) {
        std::printf("FATAL: createColorProcessor(%s -> %s) failed: %s\n", FROM,
                    FROM, config.geterror().c_str());
        return 1;
    }
    std::printf("processor %s -> %s: isNoOp=%d\n", FROM, FROM,
                (int)ident->isNoOp());

    std::printf("\ninput fill:\n");
    report("6 channels, as filled", make_buf(6), 6);

    // --- Case 1: the bug. 6 channels, distinct dst, real transform. ---
    std::printf("\nCase 1  6ch, DISTINCT dst, real transform  (expect c4/c5 "
                "unchanged)\n");
    {
        ImageBuf src = make_buf(6);
        ImageBuf dst;
        if (!ImageBufAlgo::colorconvert(dst, src, proc.get(),
                                        /*unpremult=*/false)) {
            std::printf("FATAL: colorconvert failed: %s\n",
                        dst.geterror().c_str());
            return 1;
        }
        report("result", dst, 6);

        std::vector<float> px(6, 0.0f);
        dst.getpixel(0, 0, px.data(), 6);
        std::printf("  %-34s c4=%-9.6g c5=%-9.6g\n", "expected (a copy)",
                    FILL[4], FILL[5]);
        std::printf("  %-34s c4=%-9.6g c5=%-9.6g\n", "expected if 0.5+10*x",
                    0.5f + 10.0f * FILL[4], 0.5f + 10.0f * FILL[5]);
        bool is_copy = px[4] == FILL[4] && px[5] == FILL[5];
        bool is_bug  = px[4] == 0.5f + 10.0f * FILL[4]
                      && px[5] == 0.5f + 10.0f * FILL[5];
        std::printf("  VERDICT: copy=%d  0.5+10*x=%d  -> %s\n", (int)is_copy,
                    (int)is_bug,
                    is_bug ? "CORRUPTION REPRODUCED"
                           : (is_copy ? "correct" : "neither"));
    }

    // --- Case 2: control. 4 channels, distinct dst, real transform. ---
    std::printf("\nCase 2  4ch control, DISTINCT dst, real transform  (no "
                "leftover channels)\n");
    {
        ImageBuf src = make_buf(4);
        ImageBuf dst;
        if (!ImageBufAlgo::colorconvert(dst, src, proc.get(),
                                        /*unpremult=*/false)) {
            std::printf("FATAL: colorconvert failed: %s\n",
                        dst.geterror().c_str());
            return 1;
        }
        report("result", dst, 4);
    }

    // --- Case 3: control. 6 channels, distinct dst, identity transform. ---
    std::printf("\nCase 3  6ch, DISTINCT dst, IDENTITY transform  (early-outs "
                "via copy)\n");
    {
        ImageBuf src = make_buf(6);
        ImageBuf dst;
        if (!ImageBufAlgo::colorconvert(dst, src, ident.get(),
                                        /*unpremult=*/false)) {
            std::printf("FATAL: colorconvert failed: %s\n",
                        dst.geterror().c_str());
            return 1;
        }
        report("result", dst, 6);
    }

    // --- Case 4: control. 6 channels, IN-PLACE, real transform. ---
    std::printf("\nCase 4  6ch, IN-PLACE dst==src, real transform  (&R==&A "
                "guard)\n");
    {
        ImageBuf buf = make_buf(6);
        if (!ImageBufAlgo::colorconvert(buf, buf, proc.get(),
                                        /*unpremult=*/false)) {
            std::printf("FATAL: colorconvert failed: %s\n",
                        buf.geterror().c_str());
            return 1;
        }
        report("result", buf, 6);
    }

    return 0;
}
```

</details>
