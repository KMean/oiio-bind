# Ready to file — `make_kernel("binomial", ...)` reads past its tap array

Prepared 2026-11-02. Verified against `origin/main`, `v3.2.2.0`, `v3.1.19.0`
and `origin/release`, and reproduced on a 3.2.2.0 release build. No em-dashes
below the Title line.

**Title:** `bug: make_kernel("binomial", ...) reads past its tap array for even or fractional extents`

---

**Describe the bug**

`make_kernel`'s binomial branch sizes its tap arrays from the extent the
caller asked for, and then indexes them over the odd-rounded image size.

`src/libOpenImageIO/imagebufalgo.cpp`, lines as on `origin/main`, `v3.2.2.0`
and `v3.1.19.0` (`origin/release` is the same code at `:884`, `:906`, `:910`
and `:921`):

```c++
    int w = std::max(1, (int)ceilf(width));      // :910
    ...
    w |= 1;                                      // :914   round up to odd
    ...
        float* wfilter = OIIO_ALLOCA(float, width);        // :936
        for (int i = 0; i < width; ++i)
            wfilter[i] = binomial(width - 1, i);
        float* hfilter = (height == width) ? wfilter
                                           : OIIO_ALLOCA(float, height);  // :940
    ...
        for (ImageBuf::Iterator<float> p(dst); !p.done(); ++p)
            p[0] = wfilter[p.x() - spec.x] * hfilter[p.y() - spec.y]      // :951
                   * dfilter[p.z() - spec.z];
```

The image is `w` wide, where `w` is `width` rounded up and then forced odd.
The tap array is `width` long. The loop at `:951` indexes it with
`p.x() - spec.x`, which runs over `[0, w)`.

So for a width of 4, the image is 5 wide and the array holds 4 floats, and
the last column reads `wfilter[4]`, one float past the allocation. For a
fractional width the fill itself overruns: `OIIO_ALLOCA(float, 4.5)` reserves
18 bytes and `for (int i = 0; i < 4.5; ++i)` writes five floats into them.

`height` has the same problem through `hfilter`, and the two share one array
when they are equal, so a square even request gets it on both axes.

**Evidence**

The value read is whatever the stack held, so the kernel differs from run to
run. Six fresh `oiiotool` processes, same command each time:

```
run 1:  Stats Min: 0.000000   Stats Max: 0.140625
run 2:  Stats Min: -0.000000  Stats Max: 1.000000
run 3:  Stats Min: -0.000000  Stats Max: 1.000000
run 4:  Stats Min: 0.000000   Stats Max: 0.140569
run 5:  Stats Min: -0.000294  Stats Max: 1.001567
run 6:  Stats Min: -0.000000  Stats Max: 1.000000
```

A correct 4 by 4 binomial is the outer product of `[1,3,3,1]` with itself
over 64, so its maximum is `9/64 = 0.140625`. Runs 1 and 4 are close to that
and the rest are not. Three of the six are a spike: one cell near 1 and the
remainder near zero, which is a delta rather than a blur.

Two things in that table rule out an innocent explanation.

Run 5 has a **negative** minimum. A binomial kernel is a product of binomial
coefficients, every one of which is positive, so no arithmetic on the
intended inputs can produce a negative value. It can only have come from
outside the array.

Run 4 is the quiet case: `0.140569` where `0.140625` is correct. The stray
tap happened to be small, the normalisation absorbed most of it, and the
result looks right at a glance while being wrong in the fifth digit.

On a different machine the same command has returned a kernel of `NaN`,
which propagates to every pixel of whatever it convolves.

**OpenImageIO version and dependencies**

```
OIIO 3.2.2.0 | Windows/x86_64
    Build compiler: MSVS 1951 | C++17/199711
    HW features enabled at build: sse2,sse3,ssse3,sse41,sse42
    No CUDA support (disabled / unavailable at build time)
Dependencies: BZip2 1.0.8, DCMTK NONE, FFmpeg NONE, fmt 12.1.0, Freetype 2.13.3, GIF 5.2.2, Imath 3.2.2, JXL NONE, libdeflate 1.25, Libheif NONE, libjpeg-turbo 3.1.3, LibRaw NONE, libuhdr NONE, OpenColorIO 2.5.1, OpenCV NONE, OpenEXR 3.4.7, OpenJPEG 2.5.4, openjph 0.26.3, PNG 1.6.55, Ptex NONE, Ptex NONE, Robinmap 1.4.1, TBB NONE, TIFF 4.7.1, WebP 1.6.0, ZLIB 1.3.1, zstd 1.5.7
```

Nothing unusual about the environment and no nonstandard build options. The
cited lines are the same on `origin/main`, `v3.2.2.0` and `v3.1.19.0`, and
the same code at a different offset on `origin/release`.

**On filing this as an issue rather than a pull request**

The template says to go straight to a pull request when the fix is in hand.
I do not have a CLA in place, so I am filing the report with the change
described instead.

**To Reproduce**

`oiiotool` only. Run it several times, because a single run is as likely as
not to look correct:

```bash
for i in 1 2 3 4 5 6; do
    oiiotool --kernel binomial 4x4 --printstats 2>&1 | grep "Stats M"
done
```

Expected: the same numbers every time, with a maximum of `0.140625`.
Observed: the table above.

An odd extent is well behaved, which is the control:

```bash
oiiotool --kernel binomial 3x3 --printstats
#   Stats Min: 0.062500   Stats Max: 0.250000   Stats Avg: 0.111111
```

`unsharp_mask` reaches the same branch, since it builds its blur with
`make_kernel(kernel, 1, width)` above a width of three and
`make_kernel(kernel, width, width)` at or below it, so
`--unsharp:kernel=binomial:width=4` carries the defect into a filtered image.

**The fix**

Size the arrays by the rounded extents rather than the requested ones, and
fill the whole of them:

```c++
    float* wfilter = OIIO_ALLOCA(float, w);
    for (int i = 0; i < w; ++i)
        wfilter[i] = binomial(w - 1, i);
    float* hfilter = (h == w) ? wfilter : OIIO_ALLOCA(float, h);
    if (h != w)
        for (int i = 0; i < h; ++i)
            hfilter[i] = binomial(h - 1, i);
    float* dfilter = OIIO_ALLOCA(float, d);
```

`depth` needs the same treatment for consistency, though `d` is 1 in every
in-tree use.

This changes what an even request returns: a 4 by 4 becomes the 5 tap
binomial `[1,4,6,4,1]` centred in its 5 by 5 image, rather than a 4 tap one
sitting off centre with a zero column. That seems the better answer given
`imagebufalgo.h:1849-1850`, which says the size is "rounded up to odd
resolution so that the center of the kernel can be at the center of the
middle pixel", but it is a judgement rather than a correctness matter, and
the alternative is to keep the current intent by zero filling `[width, w)`.
Either way nobody can be relying on today's behaviour, because today's
behaviour is not the same twice.

**Test coverage**

There is none. `git grep binomial testsuite/` returns nothing, which is why
this has lasted: no test asks for a binomial kernel at any extent, even or
odd. A regression test would be `--kernel binomial 4x4` asserting a maximum
of `0.140625`, or whatever the chosen fix makes it, next to a `3x3` case.

The branch has been in this shape for a long time. `git log -S` finds only
reformatting commits over it (`400e2340c` clang-format in 2018,
`0bc7e2bbe` the `ALLOCA` to `OIIO_ALLOCA` rename in 2019), so I have not
tried to name the commit that introduced it.
