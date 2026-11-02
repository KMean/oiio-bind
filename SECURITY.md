# Security

<!--
  ONE DECISION IS OUTSTANDING: the reporting channel below. This repository
  currently has issues, discussions and GitHub's private vulnerability
  reporting all disabled, so there is no way for anyone to reach the
  maintainer about a soundness hole. Pick one and replace the marked line:

    1. Enable private vulnerability reporting
       (Settings -> Security -> "Private vulnerability reporting"). Free,
       keeps the report private until it is fixed, and is what GitHub's own
       "Report a vulnerability" button uses. This is the usual choice.
    2. Enable issues, and say that a report should be filed as one. Public
       from the moment it is written, which is a poor fit for a soundness
       hole in a published crate.
    3. Publish an email address. Simple, and permanent: it cannot be taken
       back once it is in a tagged release.
-->

## Reporting a vulnerability

<!-- REPLACE THIS LINE once the channel above is chosen. -->
_No reporting channel is configured yet. See the comment in this file._

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
| [#5645](https://github.com/AcademySoftwareFoundation/OpenImageIO/issues/5645) | `ImageBufAlgo::max` reads past the narrower input | open |

Further findings are drafted in `contrib/upstream-issues.md`. A defect that is
only reachable by calling OpenImageIO directly, not through this crate, is an
upstream matter rather than a vulnerability here.
