# Ready to file — `ImageBufAlgo::max` channel clamp

Prepared 2026-11-02. Verified against `origin/main`, `v3.2.2.0`, `v3.1.19.0`
and `origin/release` (3.2.1.1), and reproduced on a 3.2.2.0 release build.
No em-dashes below the Title line.

**Title:** `bug: ImageBufAlgo::max reads past the narrower input when A and B differ in channel count`

---

**Describe the bug**

`ImageBufAlgo::max` clamps its channel range with `std::max` where every
sibling operation of the same shape uses `std::min`.

`imagebufalgo_pixelmath.cpp:170` (`origin/main`, `v3.2.2.0` and
`origin/release`; `:169` on `v3.1.19.0`):

```c++
roi.chend = std::max(roi.chend, std::max(A.nchannels(), B.nchannels()));
```

The siblings, all with identical surrounding structure:

| operation | file:line (`origin/main`) | clamp |
|---|---|---|
| `min` | `imagebufalgo_pixelmath.cpp:73` | `std::min(roi.chend, std::min(A.nchannels(), B.nchannels()))` |
| `max` | `imagebufalgo_pixelmath.cpp:170` | **`std::max(roi.chend, std::max(...))`** |
| `absdiff` | `imagebufalgo_pixelmath.cpp:324` | `std::min(roi.chend, std::min(...))` |
| `add` | `imagebufalgo_addsub.cpp:294` | `std::min(roi.chend, std::min(...))` |
| `sub` | `imagebufalgo_addsub.cpp:381` | `std::min(roi.chend, std::min(...))` |

The clamp is needed because `IBAprep` is called without
`IBAprep_CLAMP_MUTUAL_NCHANNELS` (`:167`), and `IBAprep` ends with these
lines (`imagebufalgo.cpp:307-309`):

```c++
if (prepflags & IBAprep_CLAMP_MUTUAL_NCHANNELS)
    roi.chend = std::min(roi.chend, minchans);
roi.chend = std::min(roi.chend, maxchans);
```

Without the flag, only the second line applies, and `maxchans` is the widest
of `dst`, `A` and `B` rather than the narrowest, so it is not a constraint
here. Each of these five operations therefore narrows the range by hand
afterwards. `max` widens it instead.

With `A` at 4 channels and `B` at 3, `roi.chend` becomes 4, and `max_impl`
at `:137` runs

```c++
r[c] = std::max(a[c], b[c]);
```

out to channel 3 over both operands. `ImageBuf::ConstIterator::operator[]`
performs no bounds check against the source's channel count: it indexes a
proxy built over the current pixel's raw pointer (`imagebuf.h:2016`). So
`b[3]` on a 3 channel image reads the first channel of the *following*
pixel, and on the last pixel of the buffer it reads past the allocation.
The call returns `true`.

There is a second consequence on the write side, which I want to describe
carefully because I was not able to observe it.

Because the clamp uses `std::max`, it can also raise `roi.chend` above
`dst.nchannels()`. With `dst` pre-initialized to 3 channels and both inputs
at 4, `IBAprep`'s initialized-destination branch gives `roi.chend = 3`,
`:170` raises it to 4, and the loop writes `r[3]` into a 3 channel
destination. The write path is unchecked in the same way as the read:
`Iterator::set` at `imagebuf.h:1946-1951` is `proxy[i] = val` with no
comparison against the channel count.

Reading the destination back afterwards does not show it. Each pixel's stray
write to offset 3 lands on the next pixel's channel 0, and that pixel's own
write to channel 0 follows immediately and overwrites it, so the buffer ends
up looking correct. On the final pixel there is no following write, and that
one goes past the allocation. I could not demonstrate that from outside the
library, so I am reporting it as something the code permits rather than as
something I reproduced. The read case above is the one I can show.

**The block below the clamp describes the other behaviour**

The block immediately below the clamp (`:175-188`) is byte identical to
`min`'s (`:78-91`), comment included:

```c++
if (roi.chend < origroi.chend && A.nchannels() != B.nchannels()) {
    // Edge case: A and B differed in nchannels, we allocated dst to be
    // the bigger of them, but adjusted roi to be the lesser. Now handle
    // the channels that got left out because they were not common to
    // all the inputs.
    OIIO_ASSERT(roi.chend <= dst.nchannels());
```

"adjusted roi to be the lesser" describes what `min` does at `:73`. It is not
what `max` does at `:170`. The comment was carried over with the rest of the
function and still states the intended behaviour.

That block is also unreachable in `max`. Its guard is
`roi.chend < origroi.chend`, and `std::max` can only leave `roi.chend` equal
or larger, so the copy that fills the non common channels does not run and the
`OIIO_ASSERT(roi.chend <= dst.nchannels())` inside it does not evaluate.

Both the clamp and the kernel arrived with the function in `20fecc1da`, "IBA
min and max (per-pixel operations)" (#2470, 2020-01-24), and neither line has
been edited since. The first release carrying it is `v2.2.6.1`.

**OpenImageIO version and dependencies**

Present and byte identical on `origin/main`, `v3.2.2.0`, `v3.1.19.0` and
`origin/release` (3.2.1.1). Reproduced below on a 3.2.2.0 release build,
Windows, MSVC. Platform independent: no SIMD, threading or format plugin is
involved.

**To Reproduce**

Three `oiiotool` commands. `A` has a fourth channel, `B` does not, so the
fourth channel of the result can only come from `A`, whose value is `-1`.

```bash
oiiotool --pattern constant:color=-1,-1,-1,-1 1x1 4 -d float -o A4.exr
oiiotool --pattern constant:color=5,6,7       1x1 3 -d float -o B3.exr

oiiotool A4.exr B3.exr --max -echo "max: {TOP.AVGCOLOR}"
oiiotool A4.exr B3.exr --min -echo "min: {TOP.AVGCOLOR}"
```

Observed:

```
max: 5,6,7,0
min: -1,-1,-1,-1
```

`min` is correct. `max` returns `0` for a channel whose only possible source
holds `-1`.

**Evidence that the value comes from the next pixel**

Giving `B` a different value in every pixel identifies where the read lands.
`B` is 8x1 with 3 channels, pixel `i` channel `c` holding `10 * i + c`; `A`
is 8x1 with 4 channels, every channel `-1`. Full program below.

```
=== ImageBufAlgo::min  (control, std::min clamp) -> ok=1, dst 4ch
   pixel 0:       -1       -1       -1   ch3=      -1
   ...
   pixel 7:       -1       -1       -1   ch3=      -1

=== ImageBufAlgo::max  (std::max clamp) -> ok=1, dst 4ch
   pixel 0:        0        1        2   ch3=      10
   pixel 1:       10       11       12   ch3=      20
   pixel 2:       20       21       22   ch3=      30
   pixel 3:       30       31       32   ch3=      40
   pixel 4:       40       41       42   ch3=      50
   pixel 5:       50       51       52   ch3=      60
   pixel 6:       60       61       62   ch3=      70
   pixel 7:       70       71       72   ch3=       0
```

Channel 3 at pixel `i` is `10 * (i + 1)`, which is `B`'s channel 0 one pixel
further along, and `0` at the last pixel, where the read leaves the buffer.
`min`, which differs only in the clamp and the comparison, returns `A`'s own
channel 3 at every pixel.

**The fix**

One token, at `imagebufalgo_pixelmath.cpp:170` (`:169` on `v3.1.19.0`):

```c++
roi.chend = std::min(roi.chend, std::min(A.nchannels(), B.nchannels()));
```

That makes the line identical to `min`, `absdiff`, `add` and `sub`, and it
brings `:175-188` back into play, so the non common channels are copied from
whichever input has them. `min` already behaves that way on the same inputs,
which is the closest thing to a reference I can offer.

**Test coverage**

`testsuite/oiiotool/run.py:137-139` is the only `--max` image-against-image
test, and it feeds two 3 channel patterns, so `A.nchannels() == B.nchannels()`
and the clamp does not come into play. The `--min` test three lines above does
the same. I did not find anything else in the testsuite that exercises either
call with differing channel counts.

A regression test would be two images differing in channel count through
`--max`, asserting the extra channel survives, beside the existing case at
`:137`. The same for `--min` would pin the behaviour this report is treating
as correct.

<details>
<summary>Full reproduction program</summary>

```c++
// ImageBufAlgo::max with inputs of differing channel counts.
//
// Build (Linux/macOS):
//   c++ -std=c++17 maxbug.cpp -lOpenImageIO -o maxbug
//
// Build (Windows, MSVC):
//   cl /std:c++17 /EHsc /utf-8 maxbug.cpp /I <prefix>\include ^
//      /link /LIBPATH:<prefix>\lib OpenImageIO.lib OpenImageIO_Util.lib

#include <OpenImageIO/imagebuf.h>
#include <OpenImageIO/imagebufalgo.h>
#include <OpenImageIO/imageio.h>
#include <cstdio>
#include <string>
using namespace OIIO;

constexpr int W = 8;

// B: 3 channels, pixel i channel c = 10*i + c, so any over-read names the
// pixel and channel it actually came from. A constant-filled source would
// return that same constant and hide the defect.
static ImageBuf makeB()
{
    ImageBuf b(ImageSpec(W, 1, 3, TypeDesc::FLOAT));
    for (int x = 0; x < W; ++x) {
        float px[3] = { 10.0f * x, 10.0f * x + 1, 10.0f * x + 2 };
        b.setpixel(x, 0, px, 3);
    }
    return b;
}

static void run(const char* label, bool useMax)
{
    // A: 4 channels, all -1. Channel 3 exists only in A and holds the
    // smallest value present, so max() must return -1 there unless it read
    // from somewhere else.
    ImageBuf a(ImageSpec(W, 1, 4, TypeDesc::FLOAT));
    float fill[4] = { -1, -1, -1, -1 };
    ImageBufAlgo::fill(a, fill);
    ImageBuf b = makeB();

    ImageBuf dst;  // uninitialized, as oiiotool passes it
    bool ok = useMax ? ImageBufAlgo::max(dst, a, b)
                     : ImageBufAlgo::min(dst, a, b);
    std::printf("\n=== %s -> ok=%d, dst %dch\n", label, int(ok),
                dst.spec().nchannels);
    if (!ok) {
        std::printf("    error: %s\n", dst.geterror().c_str());
        return;
    }
    for (int x = 0; x < W; ++x) {
        float px[4] = {};
        dst.getpixel(x, 0, px, 4);
        std::printf("   pixel %d: %8g %8g %8g   ch3=%8g\n", x, px[0], px[1],
                    px[2], px[3]);
    }
}

int main()
{
    std::printf("linked OIIO \"%s\" (%d)\n",
                std::string(get_string_attribute("version")).c_str(),
                openimageio_version());
    run("ImageBufAlgo::min  (control, std::min clamp)", false);
    run("ImageBufAlgo::max  (std::max clamp)", true);
    return 0;
}
```

</details>
