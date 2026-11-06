#pragma once

#include <OpenImageIO/imageio.h>
#include <OpenImageIO/oiioversion.h>

// Which OpenImageIO defects the linked library has already had fixed.
//
// WHY THIS EXISTS. Most of this crate's refusals are workarounds: OpenImageIO
// answers a call wrongly, so the shim refuses the call rather than passing the
// wrong answer on. Each of those refusals has a lifetime. When upstream fixes
// the defect, the library starts answering correctly and the refusal becomes
// the only thing standing between a caller and a working call, so the crate
// then disagrees with the library it binds. Before this header there was no way
// to express "refuse only below the version that fixed this": the sole
// OIIO_VERSION reference in oiio-sys was a reporting accessor.
//
// RUNTIME, NOT COMPILE TIME. These read OIIO::openimageio_version(), the
// version of the library actually loaded, rather than the OIIO_VERSION the
// headers declare. The two normally agree, because build.rs discovers headers
// and library from one install, but it is the loaded library that performs the
// read a guard is protecting against, so it is the loaded library that decides
// whether the guard is still needed. openimageio_build_version() in
// ffi_imageio.cpp exposes the header value for the cases where that is the
// question instead, such as whether a symbol exists at all.
//
// HOW TO USE. Wire a predicate in AT THE CALL SITE, never inside a shared
// refuse_* helper. Guards that look identical can be waiting on different
// fixes, or on none, and the set of call sites one upstream fix covers changes
// without notice. refuse_wide_destination is the worked example:
// ffi_imagebufalgo.cpp reads it at ten call sites (resize, fit, st_warp, the
// four mad shims, invert, clamp and contrast_remap) and gates none of them.
// When PR #5664 was first opened it clamped two kernels. It now passes
// IBAprep_CLAMP_MUTUAL_NCHANNELS at fourteen IBAprep call sites and rewrites resize_'s
// channel loops, so one fix went from covering none of those ten call sites to
// covering nine of them while keeping its number. A predicate inside the
// helper would have relaxed all ten on the strength of whichever revision of
// that PR it happened to be written against.
//
// ADDING A PREDICATE. Name the upstream issue and PR, and give the threshold as
// the first RELEASE that carries the fix, not the commit that made it. A fix on
// main is often backported, in which case the released version is an ancestor
// of nothing: PR #5437 landed as ee1c64138 on main and shipped as 76fca2633 in
// v3.2.2.0, so asking whether ee1c64138 is an ancestor of v3.2.2.0 answers no
// while the fix is plainly in it. Read the file at the tag instead.
//
// A THRESHOLD CANNOT ALWAYS BE WRITTEN. OIIO_VERSION is
// 10000 * major + 100 * minor + patch (oiioversion.h.in), and
// OIIO_VERSION_TWEAK is defined separately and left out of it. The 3.2 line
// ships real releases in that fourth component: v3.2.1.0, .1, .2 and .3 all
// exist, and CHANGES.md compares 3.2.2.0 against 3.2.1.3. So a fix that first
// ships in a tweak release is invisible to library_version(): 3.2.2.1 and
// 3.2.2.0 both report 30202, and the earlier one does not have the fix. When
// that happens the answer is not a threshold at all but a behaviour probe, the
// way the UDIM guard already asks the linked library what it does. Check which
// component moved before writing a number.

namespace oiio::detail {

// The version of the library this process has loaded, as major * 10000 +
// minor * 100 + patch. Cached: the call reads a global in OpenImageIO, but a
// guard may run per pixel-loop and there is no reason to pay for it twice.
inline int
library_version()
{
    static const int version = OIIO::openimageio_version();
    return version;
}

// A threshold no release can reach, for a defect that is still open upstream or
// whose fix is in an unmerged pull request. A predicate built on this is always
// false, so the guard behind it always fires and behaviour is exactly what it
// was before the predicate existed. Landing the fix is then a one-line edit:
// replace this with the release that carries it.
inline constexpr int NOT_YET_IN_ANY_RELEASE = OIIO_MAKE_VERSION(99, 0, 0);

// ---------------------------------------------------------------------------
// One predicate per defect. Thresholds are first-released-in, not merged-in.
// ---------------------------------------------------------------------------

// A whole family of ImageBufAlgo operations take their channel range from the
// destination and index the source with it unchecked, so a destination wider
// than the source reads past each source pixel. The kernels carry no guard of
// their own, and on the undefined-ROI path IBAprep replaces the region with
// the destination's whole window, which undoes any clamp applied before it.
//
// Reported as
// https://github.com/AcademySoftwareFoundation/OpenImageIO/issues/5661 for
// copy and circular_shift, then widened on PR #5664 with a measured table of
// nine more operations plus mad itself.
//
// STATE at 2026-11-14: PR #5664 is OPEN at e74dc52da, 12 files, +373/-56, with
// every check passing and no human review. It no longer adds a clamp per
// kernel. It passes the pre-existing IBAprep_CLAMP_MUTUAL_NCHANNELS flag, which
// add, sub, mul, div and most of pixelmath already passed, so the defect class
// is better described as the operations that forgot a flag their siblings pass.
//
// Covers, at that revision: copy, circular_shift, clamp, contrast_remap, mad
// and so invert, flatten, flip, flop, the three quarter rotations, resize,
// st_warp and colorconvert. perpixel_op is covered too, by the same clamp
// passed as a string-keyed option rather than the flag, which is why a grep
// for the flag name misses it. scale is clamped to its multi-channel operand by
// hand instead, because the mutual minimum would clamp to its single-channel
// one. fit keeps its own unflagged IBAprep and is covered by delegation: the
// exact path reaches warp_impl, which clamps to the source itself, and the
// resize path now goes through the flagged resize. The general warp_ kernel is
// the one thing in the family the PR leaves alone.
//
// So this predicate stands for the class, not for copy. The ten ungated
// refuse_wide_destination call sites listed above retire on the same fix, and
// each has to be wired individually once measured against a release that
// carries it. Note what this predicate does NOT cover, at the one call site
// that reads it: copy's other refusal, of a source data window that does not
// reach the allocated destination, waits on no upstream fix and is deliberately
// left outside the gate.
//
// Two of the crate's refusals do not retire but become divergences, because the
// fixed library defines what they refuse. They do not behave alike, so do not
// describe them together: scale into a destination wider than its multi-channel
// operand computes the mutual channels and leaves the rest as it found them,
// because it clamps after IBAprep returns. mad with image operands of different
// channel counts, into an empty destination, sizes that destination from the
// widest operand and ZEROES the channels outside the mutual range, because
// IBAprep does the clamping itself and zeroes what the caller will not write.
inline bool
library_clamps_mutual_channel_range()
{
    return library_version() >= NOT_YET_IN_ANY_RELEASE;
}

// `make_kernel("binomial", w, h)` sized its tap arrays from the odd-rounded
// kernel image but filled them from the requested extent, overrunning for an
// even size, and truncated a fractional extent to the wrong binomial row.
//
// Reported as
// https://github.com/AcademySoftwareFoundation/OpenImageIO/issues/5648 and
// fixed by PR #5653. Verified against a local build of that PR on 2026-11-04:
// an even extent becomes deterministic and a fractional one moves from
// [1,3,3,1,0] to the correct [1,4,6,4,1]/16. The patch that merged is
// byte-identical to the one measured, so those numbers still describe it.
//
// STATE at 2026-11-14: MERGED to main 2026-11-10 as 1ca3a9ed2 and cherry-picked
// to dev-3.2 as 3a70c33d2. Absent from every tag, v3.2.2.0 included, read by
// file content rather than by ancestry: the cherry-pick means the main SHA is
// an ancestor of nothing on the release line. So the threshold stays here.
//
// TWO THINGS TRAVEL WITH THIS THRESHOLD. The predicate has two readers, the
// helper at ffi_imagebufalgo.cpp:2016 and an inlined copy in unsharp_mask at
// :2091, and they must retire together. Four tests in
// tests/binomial_kernel_test.rs assert the refusal with no version check and
// fail the moment this clears: an_even_binomial_extent_is_refused,
// a_fractional_binomial_extent_is_refused,
// one_odd_and_one_even_extent_is_still_refused and
// unsharp_mask_refuses_an_even_binomial_width_too. Retiring the refusal is also
// a semantic decision rather than a relaxation, because the fixed library
// rounds the extent up and odd: make_kernel("binomial", 4, 4) returns a 5x5
// kernel rather than an error, which is what every other filter name already
// does. The refusal's claim that the call can return a kernel of NaN stops
// being true as well.
inline bool
library_handles_even_binomial_kernels()
{
    return library_version() >= NOT_YET_IN_ANY_RELEASE;
}

// `ImageCache::get_image_info` on a UDIM pattern whose every tile is unreadable
// copied an uninitialised buffer to the caller and returned true.
//
// Reported as
// https://github.com/AcademySoftwareFoundation/OpenImageIO/issues/5662 and
// fixed by PR #5667, which returns false with "No readable tiles for UDIM
// file" and also adds a lowercase "udim" alias for the query.
//
// STATE at 2026-11-14: MERGED to main the same day as cbdafcd37. NOT on
// dev-3.2, dev-3.1 or release: the fix's own added string reads zero times in
// src/libtexture/imagecache.cpp at all three. So it is on the development line
// only, and the first release carrying it is likelier to be a 3.3 than a 3.2.
//
// A THRESHOLD IS THE WRONG LEVER FOR THIS ONE, and setting it is currently a
// no-op because nothing reads it: ffi_texture.cpp does not include this header.
// The missing-colour bypass lives in prepare_lookup (ffi_texture.cpp:123-202)
// and has two arms of the same shape, the subimages probe at :159-162 and the
// channels probe at :183-186. Both key on the defect, and
// prepare_lookup feeds four entry points (texturesystem_texture at :229,
// texturesystem_texture_by_handle at :261, texturesystem_environment at :301
// and texturesystem_environment_by_handle at :337). On a fixed library the
// all-unreadable case answers false where it used to answer true, so :158's
// `!exists || (answered && subimages <= 0)` comes out false and control reaches
// Prepared::Refused at :170 instead of the AnswerMissing arms at :232, :264,
// :304 and :340. All four would return Err exactly where the fixed library
// fills the missing colour correctly. This is the one divergence where the fix
// makes the crate wrong rather than merely strict, and the shape of the repair
// is a detection path, not a version compare, because the fixed library answers
// false for the tiles-disagree case too.
inline bool
library_fails_udim_info_without_readable_tiles()
{
    return library_version() >= NOT_YET_IN_ANY_RELEASE;
}

// `Filesystem::filename_to_regex` left `+ ^ $ | \` unescaped, so a UDIM pattern
// containing one either threw std::regex_error out of the ImageCache, deadlocking
// the file-cache bin it was holding, or silently matched no tiles, or matched
// the wrong file.
//
// Reported as
// https://github.com/AcademySoftwareFoundation/OpenImageIO/issues/5660 and
// fixed by PR #5665, which backslashes every ECMAScript metacharacter and wraps
// the file-cache bin in a scope guard so the throw cannot leak the lock.
//
// STATE at 2026-11-14: MERGED to main the same day as e26b32344. NOT on
// dev-3.2, dev-3.1 or release: the fix's own added comment reads zero times in
// src/libutil/filesystem.cpp at all three.
//
// The crate's own UDIM guard does not need this predicate, because it probes
// the linked library's filename_to_regex at runtime rather than hardcoding a
// character set, so it retires itself when the fix lands. That is the better
// pattern where it is available, and this header's threshold section says why.
// Note what the probe covers, though: patterns that do not COMPILE. The other
// half of the defect, a pattern that compiles and then silently matches no
// tiles or the wrong file, is invisible to a probe that only asks whether a
// regex throws. Retiring the probe also changes two assertions in
// tests/texture_test.rs rather than one, because resolve_udim's None at :918
// exists only while the probe fires.
inline bool
library_escapes_udim_metacharacters()
{
    return library_version() >= NOT_YET_IN_ANY_RELEASE;
}

// `ImageBuf::nmiplevels()` returned 0 on the ImageInput branch, which is the
// branch every buffer this crate opens takes, because init_spec filled
// m_nmiplevels only on its ImageCache branch.
//
// Fixed by PR #5437 and SHIPPED, which makes this the one worked example of a
// threshold that is a real release rather than the sentinel. It landed on main
// as ee1c64138 and reached the 3.2 line as the backport 76fca2633. Measured by
// tag: `oiio:miplevels` appears 0 times in imagebuf.cpp at every tag up to and
// including v3.2.1.3 and 5 times at v3.2.2.0, so 3.2.2.0 is the first release
// that answers.
inline bool
library_fills_miplevels_without_a_cache()
{
    return library_version() >= OIIO_MAKE_VERSION(3, 2, 2);
}

}  // namespace oiio::detail
