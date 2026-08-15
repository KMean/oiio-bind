# Follow-up comment for OpenImageIO#5400

Post as a comment on <https://github.com/AcademySoftwareFoundation/OpenImageIO/issues/5400>.
It corrects two things the original report got wrong — it named the wrong
trigger, and it said exact-multiple images were unaffected when they are not.
Consider also editing the issue title to
`bug: ImageInput::read_image() with an image_span transposes x and y for tiled images`.

---

I found the cause, and it is worse than I first reported. Two corrections to my
original description, and an apology for the noise.

**The cause is an argument transposition.** The `image_span` overload of
`read_tiles` forwards to the pointer overload with the x and y ranges swapped —
`src/libOpenImageIO/imageinput.cpp:719` on `main`, `:702` in 3.1.16.0:

```c++
// Default implementation (for now): call the old pointer+stride
return read_tiles(subimage, miplevel, ybegin, yend, xbegin, xend, zbegin,
                  zend, chbegin, chend, format, data.data(),
                  data.xstride());
```

against a pointer overload declared
`read_tiles(subimage, miplevel, xbegin, xend, ybegin, yend, ...)`.
`read_image(image_span)` drives that one tile row at a time, so it inherits it.
`ImageOutput::write_tiles` is the same shape under the same comment and
forwards its ranges in order, which is what made me confident this is a slip
rather than a convention I had misread.

> **STATUS: a revised version of this was POSTED to #5400 on 2026-08-14**
> (comment 5292609883). The posted comment supersedes this draft and is the
> source of truth; it reports single-channel measurements (992 of 1024 wrong
> for 32×32, matching this draft's three-channel geometry exactly) and does
> not carry the two sentences below that the 2026-08-15 re-verification
> corrected — the "fixed points = main diagonal" description and the
> unqualified square-image generalisation were draft-only and never went
> online. This file is kept for the verification addenda.

**Correction 1: partial edge tiles are not the trigger.** Square images with
square tiles and partial edge tiles return `true`. The `(xend-x) == width` and
`(yend-y) == height` escape hatches in `ImageSpec::valid_tile_range` coincide
when width equals height, so the transposed request passes validation on every
tile row — and the caller gets wrong data with no error.

**Correction 2: images that are an exact multiple of the tile size are not
unaffected.** I said they were; they are not. They return `true` and hand back
the transposed request's pixels in a scrambled layout — the forward also
drops `ystride`, so the data is written at tile-height row pitch.
Worse, `valid_tile_range` checks divisibility and those two
escape hatches but never checks that the range lies inside the image, so a
32×16 image is asked for `y` in `[0,32)` and a 64×32 image for `y` in `[0,64)`.
The out-of-range request is accepted, nothing comes back for the region that
does not exist, and the call still returns `true` — leaving half the
destination buffer uninitialised.

Measured on 3.2.0.2dev, all with 16×16 tiles, 3 channels, float. Files hold
values separable in x and y (`ch0 = x + y/1000`, `ch1 = y + x/1000`,
`ch2 = x*1000 + y`); the destination is pre-filled with a sentinel to separate
"written wrongly" from "never written". The pointer read matches the generator
exactly in all nine cases, so the files on disk are correct:

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

Every tile row the span path executed matches the transposed-request
prediction — swapped ranges, with `ystride` recomputed from the swapped
width — on 100% of in-bounds values (verified 3072/3072 for 32×32). The few
values agreeing with the correct image number three channels times one image
side: 32×32 agrees on 96 = 3 × 32 (pixels (0..15, 0) and (16..31, 31)),
40×40 on 120 = 3 × 40, 17×17 on 51 = 3 × 17 — not the main diagonal; the
recomputed row pitch scrambles the layout beyond a neat transposition. Only
a single square tile (16×16 with 16×16 tiles) is correct, because there the
swapped arguments are equal.

So the original repro understates this: it only checks return values, and the
cases it prints as `ok` are returning transposed or partly uninitialised data.
The silent-`false` case I reported is the least dangerous of the three.

There is no buffer overrun anywhere — the transposed rectangle has the same
value count as the intended one, so this is misplacement and non-placement, not
a write past the end.

**Suggested fix**, matching the write side, which also passes all three
strides:

```c++
return read_tiles(subimage, miplevel, xbegin, xend, ybegin, yend, zbegin,
                  zend, chbegin, chend, format, data.data(),
                  data.xstride(), data.ystride(), data.zstride());
```

Two things seem worth a look independently of that: `valid_tile_range`
accepting a range outside the image is what turns a malformed request into a
silent success, and the unannotated `return false` under the guard at
`imageinput.cpp:737` on `main` (`:720` in 3.1.16.0) is what makes the failing
case produce an empty `geterror()`.

Introduced in #4748 and unchanged since — 3.1.9.0, 3.1.12.0, 3.1.14.1,
3.1.16.0 and current `main` all carry the same text. No format plugin overrides
these methods, so the base implementation is always the one that runs. Happy to
open a PR if that is useful.
