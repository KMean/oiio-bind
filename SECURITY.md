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
| [#5400](https://github.com/AcademySoftwareFoundation/OpenImageIO/issues/5400) | `read_image` with an `image_span` transposed x and y on tiled files | released |
| [#5638](https://github.com/AcademySoftwareFoundation/OpenImageIO/issues/5638) | the colour engine wrote `0.5 + 10 * src` into channels past the fourth | fixed, on the 3.2 line |
| [#5645](https://github.com/AcademySoftwareFoundation/OpenImageIO/issues/5645) | `ImageBufAlgo::max` reads past the narrower input | fixed, on the 3.2 line |
| [#5648](https://github.com/AcademySoftwareFoundation/OpenImageIO/issues/5648) | `make_kernel("binomial", ...)` indexes its tap arrays past their allocation for even or fractional extents | fixed, on the 3.2 line |
| [#5649](https://github.com/AcademySoftwareFoundation/OpenImageIO/issues/5649) | a deep `paste` wraps into the next scanline instead of clipping, and drops `chbegin` | fixed, on the 3.2 line |
| [#5660](https://github.com/AcademySoftwareFoundation/OpenImageIO/issues/5660) | a UDIM pattern holding a regex metacharacter threw out of the `ImageCache`, matched nothing, or matched the wrong file | fixed on the development line |
| [#5662](https://github.com/AcademySoftwareFoundation/OpenImageIO/issues/5662) | `get_image_info` returned true after copying an uninitialised buffer, for a UDIM set with no readable tile | fixed on the development line |
| [#5661](https://github.com/AcademySoftwareFoundation/OpenImageIO/issues/5661) | operations take their channel range from the destination and index the source with it unchecked | fix open in [#5664](https://github.com/AcademySoftwareFoundation/OpenImageIO/pull/5664) |

The state column distinguishes three things, because only the first of them
changes what an installed library does. "Released" means a published version
carries the fix. "Fixed, on the 3.2 line" means it is merged and cherry-picked
to the branch the 3.2 patch releases are cut from, but no tag carries it yet:
as of 2026-11-14 the newest releases are 3.2.2.0 of 2026-10-28 and 3.1.19.0 of
2026-10-27, and every fix above except #5400 postdates both. "Fixed on the
development line" means merged to `main` only. Each guard here is retired
against the first release that carries its fix, read from the file at that tag,
so a guard outlives the merge that makes it unnecessary.

A defect that is only reachable by calling OpenImageIO directly, not through
this crate, is an upstream matter rather than a vulnerability here.
