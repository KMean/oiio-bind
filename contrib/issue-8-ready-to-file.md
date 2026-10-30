# Issue 8 — ready to file

Verified against current `main` (1ff2c7bc2), v3.2.2.0 and v3.1.19.0, and
reproduced on a locally built 3.2.2.0 release, on 2026-10-30.

Attach `contrib/colorconvert_leftover_repro.cpp`.

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
case rather than an exotic one.

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

Build and run the attached `colorconvert_leftover_repro.cpp`. Public
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

The same thing through the shipped `oiiotool` 3.2.2.0, with no C++ at all:

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
