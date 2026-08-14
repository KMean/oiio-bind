# Fuzzing and property testing

Two layers, because one cannot do the other's job.

## Layer 1: property tests, always on

`tests/property_test.rs` runs in every `cargo test`, on stable, with no extra
tooling. It generates *shapes* rather than random bytes — a deep buffer, an
empty one, a data window far from the origin, a display window larger than the
data window, a region reaching outside the image or starting above channel
zero, a destination narrower or wider than the source — and asserts that every
operation answers with `Ok` or `Err` rather than dying, and that an `Ok` over a
whole image has actually written that whole image.

Those shapes are not guesses. Every soundness bug found in this crate has been
one of them, and none has been an unusual pixel *value*.

It earns its cost. On its first runs it found eight defects nothing else had:

- `fft` on an uninitialised source
- `fft` where the data and display windows sit far apart, asking for a
  100 010 × 100 003 transform of a 10 × 3 image
- the region clamp added the day before inverting when a region did not overlap
  its image, which `simplePixelHashSHA1` turned into a negative width and then
  an enormous unsigned one
- a region overlapping neither the source nor an already-allocated destination,
  which leaves OpenImageIO zeroing a buffer it never got
- `resize` reading past the source pixel when the destination is *wider* — the
  mirror of the `warp` bug, and in the same file
- `premult` and `unpremult` on an image with no alpha channel and a non-zero
  origin, which copy to the wrong coordinates and leave most of the result
  untouched while reporting success
- `deep_merge` and `deep_holdout` given a flat image
- the arithmetic operations given deep images in shapes OpenImageIO asserts on

Run more cases than the default when changing anything in `oiio-sys`:

```bash
PROPTEST_CASES=2000 cargo test --release --test property_test
```

Set `OIIO_BIND_TRACE=1` to print each operation before it runs. A crash takes
the process down before proptest can shrink, so the last line printed names the
operation that did it.

## Layer 2: libFuzzer, and why it needs a sanitised OpenImageIO

**A fuzzer against a stock OpenImageIO would have caught perhaps three of the
nine bugs the first review found.** Most did not crash: `ifft` on an overscan
image returned heap contents as pixels, `dilate` left `±FLT_MAX` behind, and
the colour engine wrote `0.5 + 10 × source` into channels it claimed to copy.
A read a few bytes past a heap allocation returns neighbouring memory and
nothing notices.

Seeing those needs AddressSanitizer, and it needs it in the **C++** — the reads
happen inside OpenImageIO, not in the Rust. Instrumenting only the Rust side
finds nothing, which is the trap worth knowing about before spending a day on
it.

That means building OpenImageIO with ASan, on Linux:

```bash
cmake -S <oiio-source> -B build-asan \
      -DCMAKE_BUILD_TYPE=RelWithDebInfo \
      -DCMAKE_CXX_FLAGS="-fsanitize=address -fno-omit-frame-pointer" \
      -DCMAKE_EXE_LINKER_FLAGS="-fsanitize=address" \
      -DCMAKE_INSTALL_PREFIX=<prefix>
cmake --build build-asan --target install -j
```

then pointing the crate at it and running the targets under the same
sanitizer:

```bash
export OIIO_ROOT=<prefix>
cargo +nightly fuzz run algo_shapes -- -max_total_time=600
```

`cargo-fuzz` needs nightly; the property tests above deliberately do not, so
the everyday check stays on stable and in CI.

### What to fuzz

Two targets are worth having, and they find different things:

1. **API shapes** — the same generator space as the property tests, driven by
   `arbitrary` rather than proptest. This is where the bugs found so far live.
2. **File bytes** — `ImageInput::from_memory` on arbitrary input. This exercises
   OpenImageIO's parsers, which are upstream's code, but the binding must turn
   whatever they do into an `Err` rather than a crash. That is not currently
   true: see the UTF-8 error-message defect in `contrib/upstream-issues.md`,
   where 30 of the 184 OpenEXR fuzzer fixtures kill the process.

`tests/corpus_test.rs` already sweeps OpenEXR's `Damaged` directory, which is a
corpus of real fuzzer findings and the right seed corpus for target 2.

## What neither layer covers

Neither finds a *wrong answer* that looks plausible — a colour transform off by
a channel, a filter with the wrong weights. Those need the differential testing
this crate does not yet have: run an operation through `oiiotool` and through
the binding and compare. Worth building before claiming the maths is right, as
opposed to claiming it is safe.
