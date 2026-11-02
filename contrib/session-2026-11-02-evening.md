# Evening session, 2026-11-02

Work done while Kim was away. Four commits, all pushed. 420 tests pass, up
from 413. Nothing here needs undoing; two things need a decision.

## 1. A real defect found and guarded: `make_kernel("binomial", ...)`

**This is the one that mattered.** `b3e300c`.

OpenImageIO's binomial branch sizes its tap arrays from the extent given,
`OIIO_ALLOCA(float, width)` filled by `i < width`, then indexes them over the
odd-rounded image size, `w = max(1, ceilf(width)) | 1`. An even extent reads
one tap past the allocation; a fractional one writes past it.

Measured through this crate, six fresh processes asking for a 4x4:

| run | centre tap | sum |
|---|---|---|
| 1 | 0 | **NaN** |
| 2 | 5.9e-24 | 1.0 |
| 3 | 0 | **NaN** |
| 4 | 0.1406 | 1.0 |
| 5 | 0 | **NaN** |
| 6 | 0.1406 | 1.0 |

Half the runs return a kernel of NaN, which poisons every pixel of whatever
it convolves, and OpenImageIO reports no error. Two of six are correct.

Both `make_kernel` and `unsharp_mask` now refuse a binomial whose extents are
not odd whole numbers, and say why. Scoped to binomial alone: every other
kernel name goes through a `Filter2D` sized from the rounded image, and a
test pins that those still build at even widths.

**This corrects what I told you this morning.** I ran the same call once, got
a correct kernel with `1.39e-14` in the stray row, and said the finding was
overstated and the numeric effect negligible. One sample of uninitialised
stack is not a measurement of uninitialised stack. The original finding was
closer to right than my correction.

Worth filing upstream. It is draft-quality already: the mechanism is three
lines of `imagebufalgo.cpp`, and the reproduction is one call repeated.

## 2. The other eight audit survivors were already guarded

I checked each against the crate rather than assuming. The binding already
refuses every one that is reachable through it:

| finding | crate's position |
|---|---|
| `warp_` overflows its scratch pixel | `refuse_narrow_destination`, also on `rotate`, `fit`, `zover`, `unsharp_mask` |
| `isConstantColor_` writes past a vector | shim refuses `chbegin != 0`, naming the mechanism |
| `resize_` runs dst's channel count over src | `refuse_wide_destination` |
| `normalize` reads src as dst's type | two guards in the shim |
| `max` channel clamp | refused; filed as #5645 |
| `unsharp_mask` format mismatch | guarded |
| `perpixel_op` | not bound by this crate |
| `channels()` name flag, polar doc swap | cosmetic upstream, no crate impact |

So the audit's list looked alarming and was mostly already handled. The one
live gap was the binomial kernel, now closed.

## 3. `SECURITY.md` — needs one decision from you

`dd0da7f`. A crate whose pitch is turning OpenImageIO's memory-safety defects
into typed errors should say where to report a hole in that claim.

The situation was worse than a missing file: **issues, discussions and
GitHub's private vulnerability reporting are all disabled on the repository**,
so today there is no way for anyone to reach you about a soundness hole.

The policy is written and complete except one line. Three options, in a
comment at the top of the file:

1. Enable private vulnerability reporting (Settings -> Security). Free,
   stays private until fixed, and is the usual choice.
2. Enable issues and say to file one. Public from the moment it is written.
3. Publish an email address. Cannot be undone once it is in a tagged release.

I did not pick, and did not publish your address.

## 4. Windows on the 3.2 line — added, but deliberately non-blocking

`5c2b9b5`. Windows was the only platform with no 3.2 coverage, so 3.2 there
rested on local runs on your machine.

conda-forge ships `libopenimageio` for `win-64` including the same 3.2.1.2
the Unix jobs use, so the new job gets 3.2 on Windows for the price of a
download rather than doubling the two-hour vcpkg build. The existing vcpkg
job is untouched, because exercising the vcpkg discovery path is the point of
it and that is how most Windows users will build.

It closes a second gap sideways: `build.rs` sends Windows straight to vcpkg
and never tries pkg-config, so a conda install is only reachable through
`OIIO_INCLUDE_DIR` and `OIIO_LIBRARY_DIR`. Nothing exercised those overrides
before, which left the documented escape hatch unproven.

**It is `continue-on-error: true`**, because I wrote it without a Windows
runner to try it on and a job that has never run should not redden the
workflow while nobody is watching. Remove that line once it has been seen
green; the comment in the file says so.

## Also checked, no action needed

The `enum_names!` macro's generated API renders correctly on docs.rs:
`name`, `NAMES`, `Display` and `FromStr` all appear, and the `concat!` doc
strings resolved into real prose rather than leaking.

## Still open

- **The binomial defect is unreported upstream.** The best candidate for the
  next filing, and cheaper to prepare than any of the fifteen drafts.
- **Fifteen upstream drafts**, 120 stale citations between them.
- **Anisotropy and conservative filtering on `TextureOptions`** need new C++
  shims. Additive now that the struct is `#[non_exhaustive]`.
- **The semver gate's blind spot**: it does not catch a return type changing
  between concrete types. Recorded in `publishing.md`.
