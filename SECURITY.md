# Security

## Reporting a vulnerability

Use the **Report a vulnerability** button on the repository's
[Security tab](https://github.com/KMean/oiio-bind/security/advisories/new).
That opens a private advisory, visible only to you and the maintainer
until there is a fix.

Please report privately rather than opening something public, and allow time
for a fix before disclosing. A report is welcome even if you are not sure the
behaviour is a defect: several of the defects this crate now guards against
were found by someone being unsure.

Useful in a report, roughly in order: the OpenImageIO version
(`oiiotool --buildinfo`), the platform, the smallest program that shows the
behaviour, and whether it reproduces through `oiiotool` as well. That last
one decides where the fix belongs.

## Supported versions

| version | supported |
|---|---|
| `oiio` 0.3.x, `oiio-sys` 0.3.x | yes |
| earlier | no; upgrade |

Each release is cut from `main`, and there are no maintenance branches. A fix
goes into the next release rather than being backported.

## What this crate claims

That safe Rust code using the `oiio` API cannot cause undefined behaviour,
and that a mistake becomes a typed error rather than a crash. Where
OpenImageIO would read or write out of bounds for arguments it accepts, this
crate refuses those arguments instead of passing them on, and says why. There
are roughly a dozen such refusals; each carries a comment naming the upstream
mechanism it is avoiding.

**A way to make safe code misbehave is a bug in this crate, and worth
reporting here**, including:

- undefined behaviour reachable without writing `unsafe`;
- a wrong `unsafe impl Send`, or a borrow that can outlive what it points at;
- a refusal that is wrong in either direction: an argument accepted that
  OpenImageIO mishandles, or one refused that is actually fine;
- a panic where an `Err` was documented.

## What it does not claim

That OpenImageIO itself is sound. It is a large C++ library and this crate
does not audit it, so a malformed file that crashes a reader will crash the
process. That risk is measured rather than assumed: the opt-in corpus suite
reads OpenEXR's `Damaged` directory, 68 files from a fuzzing corpus, and
requires an error rather than a crash from each.

Nor does it claim that `oiio-sys` is safe to use directly. That crate is the
raw FFI surface and its functions are `unsafe` for the usual reasons.

## Defects found in OpenImageIO

Work on this binding has turned up defects in OpenImageIO itself. Where one
is reachable from this crate, it is guarded here and reported upstream:

| upstream | what | state |
|---|---|---|
| [#5400](https://github.com/AcademySoftwareFoundation/OpenImageIO/issues/5400) | `read_image` with an `image_span` transposed x and y on tiled files | fixed |
| [#5638](https://github.com/AcademySoftwareFoundation/OpenImageIO/issues/5638) | the colour engine wrote `0.5 + 10 * src` into channels past the fourth | fixed |
| [#5645](https://github.com/AcademySoftwareFoundation/OpenImageIO/issues/5645) | `ImageBufAlgo::max` reads past the narrower input | fix open in [#5646](https://github.com/AcademySoftwareFoundation/OpenImageIO/pull/5646) |

The rest of what this crate works around is written up in
`contrib/upstream-issues.md`, with the upstream source for each. A defect that
is only reachable by calling OpenImageIO directly, not through this crate, is
an upstream matter rather than a vulnerability here.
