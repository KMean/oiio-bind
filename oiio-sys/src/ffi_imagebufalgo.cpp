#include "ffi_imagebufalgo.h"
#include "oiio-sys/src/imagebufalgo.rs.h"

#include <OpenImageIO/color.h>
#include <OpenImageIO/paramlist.h>

#include <algorithm>
#include <sstream>
#include <string>

namespace oiio {
namespace {

// OpenImageIO's arithmetic takes Image_or_Const, which is constructible from
// either an ImageBuf or a span of constants. Each pairing gets its own shim so
// the choice is made in C++ rather than smuggled across the bridge.
inline OIIO::cspan<float>
to_cspan(rust::Slice<const float> values)
{
    return OIIO::cspan<float>(values.data(), std::ptrdiff_t(values.size()));
}

inline OIIO::string_view
to_string_view(const rust::Str text)
{
    return OIIO::string_view(text.data(), text.size());
}

bool
to_texture_mode(int32_t mode, OIIO::ImageBufAlgo::MakeTextureMode& out)
{
    if (mode < 0 || mode >= int32_t(OIIO::ImageBufAlgo::_MakeTxLast))
        return false;
    out = OIIO::ImageBufAlgo::MakeTextureMode(mode);
    return true;
}

// A refused input is explained on the operation's own stream, while a failed
// write lands in the global error channel. Report whichever spoke, and both
// when both did.
void
collect_texture_failure(const std::ostringstream& printed, rust::String& error)
{
    std::string message = OIIO::geterror();
    const std::string logged = printed.str();
    if (!logged.empty()) {
        if (!message.empty())
            message += '\n';
        message += logged;
    }
    error = rust::String::lossy(message);
}

// Every colour operation shares one pixel engine, and that engine corrupts
// channels past the fourth. Where color_ocio.cpp says
//
//     // If there are "leftover" channels, just copy them
//     // unaltered from the source.
//
// the loop underneath it writes `r[c] = 0.5 + 10 * a[c];`. It looks like
// debugging code that was committed; whatever its history, it is the content
// of both 3.1.9 and 3.2.0.2dev, and it means any colour conversion of an
// image with more than four channels — a multi-AOV EXR, say — silently
// scribbles over every channel after the first four.
//
// The four the transform actually reads are correct, so the repair is to copy
// the rest from the source afterwards, which is what the comment promises. The
// copy costs one pass over channels that were about to be wrong anyway.
bool
repair_leftover_channels(ImageBuf& dst, const ImageBuf& src, const ROI& roi,
                         int nthreads)
{
    // The region arrives already clamped to the source by color_region, and
    // beginning at channel zero, so the scribble genuinely starts at 4.
    ROI bounded = roi.defined() ? roi : src.roi();
    bounded.chend = std::min(bounded.chend, src.nchannels());
    if (bounded.chend <= 4)
        return true;

    ROI leftover    = bounded;
    leftover.chbegin = 4;
    leftover.chend   = std::min(bounded.chend, dst.nchannels());
    if (leftover.chend <= leftover.chbegin)
        return true;

    return OIIO::ImageBufAlgo::copy(dst, src, OIIO::TypeDesc(), leftover,
                                    nthreads);
}

// ociolook resolves "scene_linear" through the ColorConfig two statements
// before it checks whether that pointer is null, so the documented way of
// asking for the source's own colour space crashes on the default
// configuration. Every colour shim here passes a real ColorConfig, which side
// steps it entirely.
const OIIO::ColorConfig&
default_color_config()
{
    return OIIO::ColorConfig::default_colorconfig();
}

// None of the statistics guards against a deep image, and a deep ImageBuf's
// iterator has no pixel pointer, so the first read dereferences null. Refuse
// them here.
bool
reject_deep(const ImageBuf& src, const char* operation, rust::String& error)
{
    if (!src.deep())
        return false;
    error = rust::String::lossy(std::string(operation)
                         + ": deep images have no contiguous pixels to measure");
    return true;
}

// A defined region is used verbatim by these calls, and a default-constructed
// ROI carries chend = 10000. Bring it back to what the image actually holds,
// in space as well as in channels: OpenImageIO's iterators serve reads outside
// the data window as black, but three of its fast paths take a raw pointer
// from the region instead — simplePixelHashSHA1 and the float-RGBA colour path
// among them — and read past the allocation.
// Refuse two image sources that disagree on how many channels they have.
//
// `mad` builds its destination from the union of its sources, so the wider
// one decides the channel count, and the trailing channels are the ones only
// the wider source has. `IBAprep` allocates that destination with
// `InitializePixels::No` -- nothing in OpenImageIO passes
// `IBAprep_FILL_ZERO_ALLOC` -- so anything the kernel does not go on to write
// is uninitialised heap returned to the caller with a success return. The
// lines meant to clear those trailing channels hold on 3.1.12, where every
// mismatched pair this was tried against came back fully written, and did not
// on the 3.1.14 builds CI uses: property testing caught a one-against-two and
// a six-against-three there, each with exactly the channels beyond the
// narrower source left holding heap. Rather than depend on which OpenImageIO
// is linked, the shape is refused; `channels` is the operation for lining two
// images up first.
bool
refuse_channel_mismatch(ImageBuf& dst, const ImageBuf& a, const ImageBuf& b,
                        const char* operation)
{
    if (!a.initialized() || !b.initialized())
        return false;
    if (a.nchannels() == b.nchannels())
        return false;
    dst.errorfmt("{}: the sources have {} and {} channels; OpenImageIO sizes "
                 "the destination from the wider one and does not reliably "
                 "write the channels only that one has",
                 operation, a.nchannels(), b.nchannels());
    return true;
}

ROI
bounded_to_source(const ImageBuf& src, const ROI& roi)
{
    if (!roi.defined())
        return src.roi();
    ROI bounded     = roi;
    bounded.chbegin = std::max(bounded.chbegin, 0);
    bounded.chend   = std::min(bounded.chend, src.nchannels());
    ROI clipped     = OIIO::roi_intersection(bounded, src.roi());

    // roi_intersection takes the larger begin and the smaller end, so two
    // regions that do not overlap at all come back INVERTED — end before
    // begin. ROI::width() is a plain subtraction returning int, and callers
    // convert it to an unsigned size: simplePixelHashSHA1 multiplies it by the
    // pixel size into an imagesize_t, where a negative width becomes an
    // enormous one. Collapse an empty intersection to a genuinely empty region
    // instead of an inverted one.
    if (clipped.xend < clipped.xbegin)
        clipped.xend = clipped.xbegin;
    if (clipped.yend < clipped.ybegin)
        clipped.yend = clipped.ybegin;
    if (clipped.zend < clipped.zbegin)
        clipped.zend = clipped.zbegin;
    if (clipped.chend < clipped.chbegin)
        clipped.chend = clipped.chbegin;
    return clipped;
}

// A deep image has no contiguous pixels, so any kernel that walks them
// dereferences a null pointer. IBAprep refuses deep images, but it is only
// ever shown dst, A, B and C: an image passed separately — a convolution
// kernel, an ST map — never reaches it, and fft skips IBAprep altogether.
// Each such argument is checked here instead.
bool
refuse_deep(ImageBuf& dst, const ImageBuf& image, const char* operation,
            const char* which)
{
    if (!image.deep())
        return false;
    dst.errorfmt("{}: the {} is a deep image, and this reads the contiguous "
                 "pixels a deep image does not have",
                 operation, which);
    return true;
}

// copy and paste do support deep images, but only deep into deep. The mixed
// cases fall through to the flat kernel; copy additionally calls IBAprep with
// SUPPORT_DEEP and then discards its answer, so its own rejection never fires.
bool
refuse_mixed_deep(ImageBuf& dst, const ImageBuf& src, const char* operation)
{
    if (!dst.initialized() || dst.deep() == src.deep())
        return false;
    dst.errorfmt("{}: one of these images is deep and the other is not; "
                 "OpenImageIO copies deep to deep or flat to flat, not across",
                 operation);
    return true;
}

// warp_ sizes its per-pixel scratch buffer from the destination and then has
// filtered_sample fill it from the source, so a destination with fewer
// channels is written past the end of a stack allocation. rotate and an exact
// fit both reach the same kernel.
bool
refuse_narrow_destination(ImageBuf& dst, const ImageBuf& src,
                          const char* operation)
{
    if (!dst.initialized() || dst.nchannels() >= src.nchannels())
        return false;
    dst.errorfmt("{}: the destination has {} channels and the source has {}; "
                 "OpenImageIO writes past its own per-pixel buffer when the "
                 "destination is narrower",
                 operation, dst.nchannels(), src.nchannels());
    return true;
}

// A defined region that does not overlap the source names no pixels at all.
// OpenImageIO sizes the destination from it, gets a buffer with no storage,
// and then asserts inside its own zeroing of the channels outside the range —
// which ends the process rather than returning false. Refusing is both safe
// and more use to the caller than an empty result would be.
bool
refuse_empty_region(ImageBuf& dst, const ImageBuf& src, const ROI& roi,
                    const char* operation)
{
    if (!roi.defined())
        return false;

    const auto empty = [](const ROI& r) {
        return r.xend <= r.xbegin || r.yend <= r.ybegin || r.zend <= r.zbegin;
    };

    if (empty(OIIO::roi_intersection(roi, src.roi()))) {
        dst.errorfmt("{}: the region {},{} to {},{} does not overlap the "
                     "source, whose pixels span {},{} to {},{}",
                     operation, roi.xbegin, roi.ybegin, roi.xend, roi.yend,
                     src.roi().xbegin, src.roi().ybegin, src.roi().xend,
                     src.roi().yend);
        return true;
    }

    // The destination matters too when it is already allocated: OpenImageIO
    // intersects the region with it, and an empty result leaves the kernel
    // dividing by a zero-width scanline.
    if (dst.initialized() && empty(OIIO::roi_intersection(roi, dst.roi()))) {
        dst.errorfmt("{}: the region {},{} to {},{} does not overlap the "
                     "destination, whose pixels span {},{} to {},{}",
                     operation, roi.xbegin, roi.ybegin, roi.xend, roi.yend,
                     dst.roi().xbegin, dst.roi().ybegin, dst.roi().xend,
                     dst.roi().yend);
        return true;
    }
    return false;
}

// Several operations support deep images on both sides but assert rather than
// erroring when only one side is deep: the deep path calls copy_deep_pixel,
// whose OIIO_ASSERT(dst.deep() && src.deep()) a release build does not check.
// With no explicit region, IBAprep uses the UNION of the inputs' data windows.
// Two images whose windows sit far apart — one at the origin, one at 100000 —
// union to everything in between, and OpenImageIO then tries to allocate a
// destination that size. The allocation fails, and what follows is a division
// by the resulting zero-width scanline. Same shape as the fft case above.
bool
refuse_distant_pair(ImageBuf& dst, const ImageBuf& a, const ImageBuf& b,
                    const char* operation)
{
    const OIIO::imagesize_t held = a.roi().npixels() + b.roi().npixels();
    const OIIO::imagesize_t spanned
        = OIIO::roi_union(a.roi(), b.roi()).npixels();
    if (spanned <= held * 16 + 1024)
        return false;
    dst.errorfmt("{}: the two images sit too far apart to combine — one spans "
                 "{},{} to {},{} and the other {},{} to {},{}, so the result "
                 "would cover {} pixels for {} of input. Give an explicit "
                 "region, or move them onto the same coordinates",
                 operation, a.roi().xbegin, a.roi().ybegin, a.roi().xend,
                 a.roi().yend, b.roi().xbegin, b.roi().ybegin, b.roi().xend,
                 b.roi().yend, spanned, held);
    return true;
}

// OpenImageIO's arithmetic accepts deep images and then handles them poorly:
// the surplus-channel fixup calls a deep copy over a channel range that copy
// asserts against, and several paths reach copy_deep_pixel with a destination
// that never came out deep. Those assertions are compiled away in a release
// build, so what a caller gets is a dead process.
//
// The crate has operations built for deep images — flatten, deepen,
// deep_merge, deep_holdout — and they are guarded properly. Deep arithmetic is
// not something OpenImageIO supports well enough to pass on, so these refuse
// it outright rather than passing on a landmine.
bool
refuse_deep_mismatch(ImageBuf& dst, const ImageBuf& a, const ImageBuf& b,
                     const char* operation)
{
    if (!a.deep() && !b.deep())
        return false;
    dst.errorfmt("{}: deep images are not supported here; flatten them first, "
                 "or use deep_merge or deep_holdout",
                 operation);
    return true;
}

// The deep compositing operations need every image deep, and OpenImageIO
// asserts rather than erroring when a destination it allocated does not come
// out deep. These say so before it can.
bool
require_deep(ImageBuf& dst, const ImageBuf& image, const char* operation,
             const char* which)
{
    if (image.deep())
        return false;
    dst.errorfmt("{}: the {} is not a deep image", operation, which);
    return true;
}

bool
refuse_flat_destination(ImageBuf& dst, const char* operation)
{
    if (!dst.initialized() || dst.deep())
        return false;
    dst.errorfmt("{}: the destination is a flat image; pass an empty one or a "
                 "deep one",
                 operation);
    return true;
}

// resize_ is the mirror image: it sizes the same per-pixel buffer from the
// destination and then reads the SOURCE pixel to that length, so a WIDER
// destination reads past the source. warp overruns for a narrower one, resize
// for a wider one, and fit reaches warp_ when exact and resize_ otherwise —
// so fit needs both, which comes to requiring the counts to match.
bool
refuse_wide_destination(ImageBuf& dst, const ImageBuf& src,
                        const char* operation)
{
    if (!dst.initialized() || dst.nchannels() <= src.nchannels())
        return false;
    dst.errorfmt("{}: the destination has {} channels and the source has {}; "
                 "OpenImageIO reads past the source pixel when the "
                 "destination is wider",
                 operation, dst.nchannels(), src.nchannels());
    return true;
}

// The colour engine mishandles a region that starts above channel zero in
// three separate ways, all measured: it transforms channels 0..n rather than
// the ones asked for, it leaves the requested upper channels merely copied,
// and the scribble described above begins at min(4, roi.nchannels()) rather
// than at channel 4, so it lands below where the repair reaches. No ordering
// of the repair rescues that, so the region is refused instead — the same
// shape is_constant_color and nonzero_region already refuse.
//
// The region is clamped to the source as well, because the float-RGBA fast
// path memcpys from the source's pixel address across the region's width, and
// IBAprep has only intersected the region with the destination.
bool
color_region(ImageBuf& dst, const ImageBuf& src, const ROI& roi, ROI& bounded,
             const char* operation)
{
    if (refuse_deep(dst, src, operation, "source"))
        return false;
    bounded = bounded_to_source(src, roi);
    if (bounded.chbegin != 0) {
        dst.errorfmt("{}: the region must begin at channel zero; OpenImageIO "
                     "transforms the wrong channels otherwise",
                     operation);
        return false;
    }
    return true;
}

}  // namespace

PixelStatistics
imagebufalgo_pixel_stats(const ImageBuf& src, const ROI& roi, int nthreads)
{
    PixelStatistics result;
    result.ok = false;

    if (src.deep()) {
        result.error = rust::String::lossy(
            "pixel statistics: deep images are measured per sample, which this "
            "call does not report");
        return result;
    }

    // OpenImageIO decides success by asking whether `src` carries an error at
    // all, so anything left over from an earlier call would be attributed to
    // this one. Clear it before measuring.
    src.geterror(true);

    const OIIO::ImageBufAlgo::PixelStats stats
        = OIIO::ImageBufAlgo::computePixelStats(src, bounded_to_source(src, roi),
                                                nthreads);
    if (stats.min.empty()) {
        std::string message = src.geterror(true);
        if (message.empty())
            message = "OpenImageIO reported no statistics and no reason";
        result.error = rust::String::lossy(message);
        return result;
    }

    const auto copy_floats = [](const std::vector<float>& from,
                                rust::Vec<float>& to) {
        to.reserve(from.size());
        for (float value : from)
            to.push_back(value);
    };
    const auto copy_counts = [](const std::vector<OIIO::imagesize_t>& from,
                                rust::Vec<uint64_t>& to) {
        to.reserve(from.size());
        for (OIIO::imagesize_t value : from)
            to.push_back(uint64_t(value));
    };

    copy_floats(stats.min, result.min);
    copy_floats(stats.max, result.max);
    copy_floats(stats.avg, result.average);
    copy_floats(stats.stddev, result.standard_deviation);
    copy_counts(stats.nancount, result.nan_count);
    copy_counts(stats.infcount, result.infinite_count);
    copy_counts(stats.finitecount, result.finite_count);
    result.ok = true;
    return result;
}

rust::Vec<uint64_t>
imagebufalgo_histogram(const ImageBuf& src, int channel, int bins, float min,
                       float max, bool ignore_empty, const ROI& roi,
                       int nthreads, rust::String& error)
{
    rust::Vec<uint64_t> result;
    if (reject_deep(src, "histogram", error))
        return result;

    src.geterror(true);
    const std::vector<OIIO::imagesize_t> counts
        = OIIO::ImageBufAlgo::histogram(src, channel, bins, min, max,
                                        ignore_empty,
                                        bounded_to_source(src, roi), nthreads);
    if (counts.empty()) {
        std::string message = src.geterror(true);
        if (message.empty())
            message = "OpenImageIO produced no histogram and no reason";
        error = rust::String::lossy(message);
        return result;
    }

    result.reserve(counts.size());
    for (OIIO::imagesize_t count : counts)
        result.push_back(uint64_t(count));
    return result;
}

bool
imagebufalgo_is_constant_color(const ImageBuf& src, float threshold,
                               rust::Slice<float> color, const ROI& roi,
                               int nthreads, rust::String& error)
{
    if (reject_deep(src, "is_constant_color", error))
        return false;

    const ROI bounded = bounded_to_source(src, roi);
    if (bounded.chbegin != 0) {
        // imagebufalgo_compare.cpp sizes the reference vector to the region's
        // channel count but indexes it with absolute channel numbers, so a
        // region starting above channel zero writes past its end.
        error = rust::String::lossy(
            "is_constant_color: the region must begin at channel zero; "
            "OpenImageIO writes past its own buffer otherwise");
        return false;
    }

    src.geterror(true);
    const bool constant
        = OIIO::ImageBufAlgo::isConstantColor(src, threshold,
                                              OIIO::span<float>(color.data(),
                                                                std::ptrdiff_t(
                                                                    color.size())),
                                              bounded, nthreads);
    if (!constant) {
        const std::string message = src.geterror(true);
        if (!message.empty())
            error = rust::String::lossy(message);
    }
    return constant;
}

bool
imagebufalgo_is_constant_channel(const ImageBuf& src, int channel, float value,
                                 float threshold, const ROI& roi, int nthreads,
                                 rust::String& error)
{
    if (reject_deep(src, "is_constant_channel", error))
        return false;
    if (channel < 0 || channel >= src.nchannels()) {
        // OpenImageIO returns false here and records nothing, which is
        // indistinguishable from "the channel is not constant".
        error = rust::String::lossy("is_constant_channel: channel "
                             + std::to_string(channel) + " is outside the "
                             + std::to_string(src.nchannels())
                             + " the image has");
        return false;
    }

    src.geterror(true);
    const bool constant
        = OIIO::ImageBufAlgo::isConstantChannel(src, channel, value, threshold,
                                                bounded_to_source(src, roi),
                                                nthreads);
    if (!constant) {
        const std::string message = src.geterror(true);
        if (!message.empty())
            error = rust::String::lossy(message);
    }
    return constant;
}

bool
imagebufalgo_is_monochrome(const ImageBuf& src, float threshold, const ROI& roi,
                           int nthreads, rust::String& error)
{
    if (reject_deep(src, "is_monochrome", error))
        return false;

    src.geterror(true);
    const bool monochrome
        = OIIO::ImageBufAlgo::isMonochrome(src, threshold,
                                           bounded_to_source(src, roi),
                                           nthreads);
    if (!monochrome) {
        const std::string message = src.geterror(true);
        if (!message.empty())
            error = rust::String::lossy(message);
    }
    return monochrome;
}

ROI
imagebufalgo_nonzero_region(const ImageBuf& src, const ROI& roi, int nthreads,
                            rust::String& error)
{
    const ROI bounded = bounded_to_source(src, roi);
    if (bounded.chbegin != 0) {
        // nonzero_region trims by calling isConstantColor, so it inherits that
        // function's out-of-bounds write for a region above channel zero.
        error = rust::String::lossy(
            "nonzero_region: the region must begin at channel zero; "
            "OpenImageIO writes past its own buffer otherwise");
        return ROI();
    }
    src.geterror(true);
    return OIIO::ImageBufAlgo::nonzero_region(src, bounded, nthreads);
}

rust::String
imagebufalgo_pixel_hash_sha1(const ImageBuf& src, const rust::Str extrainfo,
                             const ROI& roi, int nthreads, rust::String& error)
{
    if (reject_deep(src, "pixel_hash_sha1", error))
        return rust::String();
    const ROI hashed = bounded_to_source(src, roi);
    if (!src.initialized() || src.spec().pixel_bytes() == 0
        || hashed.npixels() == 0) {
        // simplePixelHashSHA1 divides by roi.width() * pixel_bytes() without
        // checking it, so an empty region takes the process down with an
        // integer division by zero. That includes a region clamped to nothing
        // because it never overlapped the image.
        error = rust::String::lossy(
            "pixel_hash_sha1: the region holds no pixels to hash");
        return rust::String();
    }

    src.geterror(true);
    // Block size fixed at 0: any other value changes the digest for identical
    // pixels, and combined with a region that does not start at the image's
    // first row it indexes past the end of the block-results vector.
    const std::string digest
        = OIIO::ImageBufAlgo::computePixelHashSHA1(src,
                                                   to_string_view(extrainfo),
                                                   hashed, 0, nthreads);
    if (digest.empty()) {
        std::string message = src.geterror(true);
        if (message.empty())
            message = "OpenImageIO produced no digest and no reason";
        error = rust::String::lossy(message);
    }
    return rust::String::lossy(digest);
}

bool
imagebufalgo_zero(ImageBuf& dst, const ROI& roi, int nthreads)
{
    return OIIO::ImageBufAlgo::zero(dst, roi, nthreads);
}

bool
imagebufalgo_fill(ImageBuf& dst, rust::Slice<const float> values, const ROI& roi,
                  int nthreads)
{
    return OIIO::ImageBufAlgo::fill(dst, to_cspan(values), roi, nthreads);
}

bool
imagebufalgo_add_images(ImageBuf& dst, const ImageBuf& a, const ImageBuf& b,
                        const ROI& roi, int nthreads)
{
    if (refuse_distant_pair(dst, a, b, "add")
        || refuse_deep_mismatch(dst, a, b, "add"))
        return false;
    return OIIO::ImageBufAlgo::add(dst, a, b, roi, nthreads);
}

bool
imagebufalgo_add_constant(ImageBuf& dst, const ImageBuf& a,
                          rust::Slice<const float> values, const ROI& roi,
                          int nthreads)
{
    return OIIO::ImageBufAlgo::add(dst, a, to_cspan(values), roi, nthreads);
}

bool
imagebufalgo_sub_images(ImageBuf& dst, const ImageBuf& a, const ImageBuf& b,
                        const ROI& roi, int nthreads)
{
    if (refuse_distant_pair(dst, a, b, "sub")
        || refuse_deep_mismatch(dst, a, b, "sub"))
        return false;
    return OIIO::ImageBufAlgo::sub(dst, a, b, roi, nthreads);
}

bool
imagebufalgo_sub_constant(ImageBuf& dst, const ImageBuf& a,
                          rust::Slice<const float> values, const ROI& roi,
                          int nthreads)
{
    return OIIO::ImageBufAlgo::sub(dst, a, to_cspan(values), roi, nthreads);
}

bool
imagebufalgo_mul_images(ImageBuf& dst, const ImageBuf& a, const ImageBuf& b,
                        const ROI& roi, int nthreads)
{
    if (refuse_distant_pair(dst, a, b, "mul")
        || refuse_deep_mismatch(dst, a, b, "mul"))
        return false;
    return OIIO::ImageBufAlgo::mul(dst, a, b, roi, nthreads);
}

bool
imagebufalgo_mul_constant(ImageBuf& dst, const ImageBuf& a,
                          rust::Slice<const float> values, const ROI& roi,
                          int nthreads)
{
    return OIIO::ImageBufAlgo::mul(dst, a, to_cspan(values), roi, nthreads);
}

bool
imagebufalgo_div_images(ImageBuf& dst, const ImageBuf& a, const ImageBuf& b,
                        const ROI& roi, int nthreads)
{
    if (refuse_distant_pair(dst, a, b, "div")
        || refuse_deep_mismatch(dst, a, b, "div"))
        return false;
    return OIIO::ImageBufAlgo::div(dst, a, b, roi, nthreads);
}

bool
imagebufalgo_div_constant(ImageBuf& dst, const ImageBuf& a,
                          rust::Slice<const float> values, const ROI& roi,
                          int nthreads)
{
    return OIIO::ImageBufAlgo::div(dst, a, to_cspan(values), roi, nthreads);
}

bool
imagebufalgo_abs(ImageBuf& dst, const ImageBuf& a, const ROI& roi, int nthreads)
{
    if (refuse_empty_region(dst, a, roi, "abs"))
        return false;
    return OIIO::ImageBufAlgo::abs(dst, a, roi, nthreads);
}

bool
imagebufalgo_absdiff_images(ImageBuf& dst, const ImageBuf& a, const ImageBuf& b,
                            const ROI& roi, int nthreads)
{
    if (refuse_distant_pair(dst, a, b, "absdiff")
        || refuse_deep_mismatch(dst, a, b, "absdiff"))
        return false;
    return OIIO::ImageBufAlgo::absdiff(dst, a, b, roi, nthreads);
}

bool
imagebufalgo_copy(ImageBuf& dst, const ImageBuf& src, TypeDesc convert,
                  const ROI& roi, int nthreads)
{
    if (refuse_empty_region(dst, src, roi, "copy"))
        return false;
    if (refuse_mixed_deep(dst, src, "copy"))
        return false;
    return OIIO::ImageBufAlgo::copy(dst, src, convert, roi, nthreads);
}

bool
imagebufalgo_crop(ImageBuf& dst, const ImageBuf& src, const ROI& roi,
                  int nthreads)
{
    if (refuse_empty_region(dst, src, roi, "crop"))
        return false;
    return OIIO::ImageBufAlgo::crop(dst, src, roi, nthreads);
}

bool
imagebufalgo_flip(ImageBuf& dst, const ImageBuf& src, const ROI& roi,
                  int nthreads)
{
    if (refuse_empty_region(dst, src, roi, "flip"))
        return false;
    return OIIO::ImageBufAlgo::flip(dst, src, roi, nthreads);
}

bool
imagebufalgo_flop(ImageBuf& dst, const ImageBuf& src, const ROI& roi,
                  int nthreads)
{
    if (refuse_empty_region(dst, src, roi, "flop"))
        return false;
    return OIIO::ImageBufAlgo::flop(dst, src, roi, nthreads);
}

bool
imagebufalgo_transpose(ImageBuf& dst, const ImageBuf& src, const ROI& roi,
                       int nthreads)
{
    if (refuse_empty_region(dst, src, roi, "transpose"))
        return false;
    if (refuse_deep(dst, src, "transpose", "source"))
        return false;
    return OIIO::ImageBufAlgo::transpose(dst, src, roi, nthreads);
}

bool
imagebufalgo_colorconvert(ImageBuf& dst, const ImageBuf& src,
                          const rust::Str fromspace, const rust::Str tospace,
                          bool unpremult, const ROI& roi, int nthreads)
{
    ROI bounded;
    if (!color_region(dst, src, roi, bounded, "colorconvert"))
        return false;

    const std::string from(to_string_view(fromspace));
    const std::string to(to_string_view(tospace));
    if (!OIIO::ImageBufAlgo::colorconvert(dst, src, from, to, unpremult, "", "",
                                          &default_color_config(), bounded,
                                          nthreads))
        return false;
    return repair_leftover_channels(dst, src, bounded, nthreads);
}

bool
imagebufalgo_colormatrixtransform(ImageBuf& dst, const ImageBuf& src,
                                  rust::Slice<const float> matrix,
                                  bool unpremult, const ROI& roi, int nthreads)
{
    if (matrix.size() != 16) {
        dst.errorfmt("colour matrix transform: the matrix needs sixteen "
                     "values, got {}",
                     matrix.size());
        return false;
    }
    ROI bounded;
    if (!color_region(dst, src, roi, bounded, "colour matrix transform"))
        return false;

    float m[4][4];
    for (std::size_t row = 0; row < 4; ++row)
        for (std::size_t column = 0; column < 4; ++column)
            m[row][column] = matrix[row * 4 + column];

    if (!OIIO::ImageBufAlgo::colormatrixtransform(dst, src, m, unpremult,
                                                  bounded, nthreads))
        return false;
    return repair_leftover_channels(dst, src, bounded, nthreads);
}

bool
imagebufalgo_ociolook(ImageBuf& dst, const ImageBuf& src,
                      const rust::Str looks, const rust::Str fromspace,
                      const rust::Str tospace, bool unpremult, bool inverse,
                      const rust::Str context_key,
                      const rust::Str context_value, const ROI& roi,
                      int nthreads)
{
    const std::string look_list(to_string_view(looks));
    const std::string from(to_string_view(fromspace));
    const std::string to(to_string_view(tospace));
    const std::string key(to_string_view(context_key));
    const std::string value(to_string_view(context_value));

    ROI bounded;
    if (!color_region(dst, src, roi, bounded, "ociolook"))
        return false;
    if (!OIIO::ImageBufAlgo::ociolook(dst, src, look_list, from, to, unpremult,
                                      inverse, key, value,
                                      &default_color_config(), bounded,
                                      nthreads))
        return false;
    return repair_leftover_channels(dst, src, bounded, nthreads);
}

bool
imagebufalgo_ociodisplay(ImageBuf& dst, const ImageBuf& src,
                         const rust::Str display, const rust::Str view,
                         const rust::Str fromspace, const rust::Str looks,
                         bool unpremult, bool inverse,
                         const rust::Str context_key,
                         const rust::Str context_value, const ROI& roi,
                         int nthreads)
{
    const std::string display_name(to_string_view(display));
    const std::string view_name(to_string_view(view));
    const std::string from(to_string_view(fromspace));
    const std::string look_list(to_string_view(looks));
    const std::string key(to_string_view(context_key));
    const std::string value(to_string_view(context_value));

    ROI bounded;
    if (!color_region(dst, src, roi, bounded, "ociodisplay"))
        return false;
    if (!OIIO::ImageBufAlgo::ociodisplay(dst, src, display_name, view_name,
                                         from, look_list, unpremult, inverse,
                                         key, value, &default_color_config(),
                                         bounded, nthreads))
        return false;
    return repair_leftover_channels(dst, src, bounded, nthreads);
}

bool
imagebufalgo_ociofiletransform(ImageBuf& dst, const ImageBuf& src,
                               const rust::Str name, bool unpremult,
                               bool inverse, const ROI& roi, int nthreads)
{
    // OpenImageIO hands this straight to OCIO through c_str(string_view),
    // which reads one byte past a view that is not NUL-terminated. A Rust
    // &str never is, so it becomes a std::string here first.
    const std::string transform(to_string_view(name));
    if (transform.empty()) {
        dst.errorfmt("colour file transform: no transform file was named");
        return false;
    }
    ROI bounded;
    if (!color_region(dst, src, roi, bounded, "ociofiletransform"))
        return false;
    if (!OIIO::ImageBufAlgo::ociofiletransform(dst, src, transform, unpremult,
                                               inverse, &default_color_config(),
                                               bounded, nthreads))
        return false;
    return repair_leftover_channels(dst, src, bounded, nthreads);
}

bool
imagebufalgo_ocionamedtransform(ImageBuf& dst, const ImageBuf& src,
                                const rust::Str name, bool unpremult,
                                bool inverse, const rust::Str context_key,
                                const rust::Str context_value, const ROI& roi,
                                int nthreads)
{
    const std::string transform(to_string_view(name));
    const std::string key(to_string_view(context_key));
    const std::string value(to_string_view(context_value));

    ROI bounded;
    if (!color_region(dst, src, roi, bounded, "ocionamedtransform"))
        return false;
    if (!OIIO::ImageBufAlgo::ocionamedtransform(dst, src, transform, unpremult,
                                                inverse, key, value,
                                                &default_color_config(),
                                                bounded, nthreads))
        return false;
    return repair_leftover_channels(dst, src, bounded, nthreads);
}

bool
imagebufalgo_resize(ImageBuf& dst, const ImageBuf& src,
                    const rust::Str filtername, float filterwidth,
                    const ROI& roi, int nthreads)
{
    if (refuse_empty_region(dst, src, roi, "resize"))
        return false;
    if (refuse_wide_destination(dst, src, "resize"))
        return false;
    OIIO::ParamValueList options;
    if (filtername.size() != 0)
        options["filtername"] = std::string(filtername.data(),
                                            filtername.size());
    if (filterwidth > 0.0f)
        options["filterwidth"] = filterwidth;
    return OIIO::ImageBufAlgo::resize(dst, src, options, roi, nthreads);
}

bool
imagebufalgo_fit(ImageBuf& dst, const ImageBuf& src, const rust::Str filtername,
                 float filterwidth, const rust::Str fillmode, bool exact,
                 const ROI& roi, int nthreads)
{
    if (refuse_empty_region(dst, src, roi, "fit"))
        return false;
    // An exact fit reaches warp_ and an inexact one reaches resize_; the two
    // overrun in opposite directions, so a pre-allocated destination has to
    // match the source's channel count either way.
    if (refuse_narrow_destination(dst, src, "fit")
        || refuse_wide_destination(dst, src, "fit"))
        return false;
    OIIO::ParamValueList options;
    if (filtername.size() != 0)
        options["filtername"] = std::string(filtername.data(),
                                            filtername.size());
    if (filterwidth > 0.0f)
        options["filterwidth"] = filterwidth;
    if (fillmode.size() != 0)
        options["fillmode"] = std::string(fillmode.data(), fillmode.size());
    options["exact"] = int(exact);
    return OIIO::ImageBufAlgo::fit(dst, src, options, roi, nthreads);
}

bool
imagebufalgo_resample(ImageBuf& dst, const ImageBuf& src, bool interpolate,
                      const ROI& roi, int nthreads)
{
    if (refuse_empty_region(dst, src, roi, "resample"))
        return false;
    return OIIO::ImageBufAlgo::resample(dst, src, interpolate, roi, nthreads);
}

bool
imagebufalgo_over(ImageBuf& dst, const ImageBuf& a, const ImageBuf& b,
                  const ROI& roi, int nthreads)
{
    if (refuse_distant_pair(dst, a, b, "over")
        || refuse_deep_mismatch(dst, a, b, "over"))
        return false;
    return OIIO::ImageBufAlgo::over(dst, a, b, roi, nthreads);
}

// premult and unpremult, given a source with no alpha channel, document that
// they "just copy instead of dividing by alpha" — and implement that copy as
//
//     paste(dst, src.spec().x, src.spec().y, src.spec().z, roi.chbegin, src, roi, ...)
//
// but paste ADDS its offset to the source's own coordinates, so the origin is
// counted twice. For a data window at 5,7 the pixels land 5,7 further along
// than they should: part of the destination gets the image, the rest is left
// as allocated. Measured on a 4x1 RGB image at origin 5,7 — every pixel of the
// result came back empty, and the call reported success.
//
// The copy is the intent, so do the copy.
bool
copy_instead_of_alpha_division(ImageBuf& dst, const ImageBuf& src,
                               const ROI& roi, int nthreads)
{
    return OIIO::ImageBufAlgo::copy(dst, src, OIIO::TypeDesc(), roi, nthreads);
}

bool
imagebufalgo_premult(ImageBuf& dst, const ImageBuf& src, const ROI& roi,
                     int nthreads)
{
    if (refuse_empty_region(dst, src, roi, "premult"))
        return false;
    if (src.spec().alpha_channel < 0)
        return copy_instead_of_alpha_division(dst, src, roi, nthreads);
    return OIIO::ImageBufAlgo::premult(dst, src, roi, nthreads);
}

bool
imagebufalgo_unpremult(ImageBuf& dst, const ImageBuf& src, const ROI& roi,
                       int nthreads)
{
    if (refuse_empty_region(dst, src, roi, "unpremult"))
        return false;
    if (src.spec().alpha_channel < 0)
        return copy_instead_of_alpha_division(dst, src, roi, nthreads);
    return OIIO::ImageBufAlgo::unpremult(dst, src, roi, nthreads);
}

bool
imagebufalgo_channel_sum(ImageBuf& dst, const ImageBuf& src,
                         rust::Slice<const float> weights, const ROI& roi,
                         int nthreads)
{
    if (refuse_empty_region(dst, src, roi, "channel_sum"))
        return false;
    if (refuse_deep(dst, src, "channel_sum", "source"))
        return false;
    // OpenImageIO pads the weights to the DESTINATION's channel count, which
    // is one, and then indexes them across the SOURCE's channels. A shorter
    // slice than the source has channels is read past its end.
    if (std::ptrdiff_t(weights.size()) != std::ptrdiff_t(src.nchannels())) {
        dst.errorfmt("channel_sum: expected one weight per source channel, "
                     "got {} for {} channels",
                     weights.size(), src.nchannels());
        return false;
    }
    return OIIO::ImageBufAlgo::channel_sum(dst, src, to_cspan(weights), roi,
                                           nthreads);
}

bool
imagebufalgo_channels(ImageBuf& dst, const ImageBuf& src, int nchannels,
                      rust::Slice<const int> channelorder,
                      rust::Slice<const float> channelvalues,
                      const rust::Vec<rust::String>& newchannelnames,
                      bool shuffle_channel_names, int nthreads)
{
    std::vector<std::string> names;
    names.reserve(newchannelnames.size());
    for (const rust::String& name : newchannelnames)
        names.emplace_back(name.data(), name.size());

    return OIIO::ImageBufAlgo::channels(
        dst, src, nchannels,
        OIIO::cspan<int>(channelorder.data(),
                         std::ptrdiff_t(channelorder.size())),
        to_cspan(channelvalues),
        OIIO::cspan<std::string>(names.data(), std::ptrdiff_t(names.size())),
        shuffle_channel_names, nthreads);
}

CompareSummary
imagebufalgo_compare(const ImageBuf& a, const ImageBuf& b, float failthresh,
                     float warnthresh, const ROI& roi, int nthreads,
                     rust::String& error)
{
    CompareSummary refused {};
    if (a.deep() != b.deep()) {
        // CompareResults is a plain aggregate with no initialisers. When the
        // images disagree about deepness, compare sets only `error` and
        // returns, leaving every measurement indeterminate — so there is
        // nothing here worth handing back.
        error = rust::String::lossy(
            "compare: one image is deep and the other is not");
        return refused;
    }
    if (!a.initialized() || !b.initialized()) {
        error = rust::String::lossy("compare: both images must hold pixels");
        return refused;
    }

    a.geterror(true);
    const OIIO::ImageBufAlgo::CompareResults results
        = OIIO::ImageBufAlgo::compare(a, b, failthresh, warnthresh, roi,
                                      nthreads);
    if (results.error && results.nfail == 0) {
        // `error` means "some values exceeded the fail threshold" on the
        // normal path and "the comparison did not happen" otherwise; nfail
        // tells the two apart.
        std::string message = a.geterror(true);
        if (message.empty())
            message = "OpenImageIO could not compare these images";
        error = rust::String::lossy(message);
        return refused;
    }
    CompareSummary summary;
    summary.mean_error                = results.meanerror;
    summary.root_mean_square_error    = results.rms_error;
    summary.peak_signal_to_noise_ratio = results.PSNR;
    summary.max_error                 = results.maxerror;
    summary.max_x                     = results.maxx;
    summary.max_y                     = results.maxy;
    summary.max_z                     = results.maxz;
    summary.max_channel               = results.maxc;
    summary.warnings                  = uint64_t(results.nwarn);
    summary.failures                  = uint64_t(results.nfail);
    summary.failed                    = results.error;
    return summary;
}

bool
imagebufalgo_fill_vertical(ImageBuf& dst, rust::Slice<const float> top,
                           rust::Slice<const float> bottom, const ROI& roi,
                           int nthreads)
{
    return OIIO::ImageBufAlgo::fill(dst, to_cspan(top), to_cspan(bottom), roi,
                                    nthreads);
}

bool
imagebufalgo_fill_corners(ImageBuf& dst, rust::Slice<const float> topleft,
                          rust::Slice<const float> topright,
                          rust::Slice<const float> bottomleft,
                          rust::Slice<const float> bottomright, const ROI& roi,
                          int nthreads)
{
    return OIIO::ImageBufAlgo::fill(dst, to_cspan(topleft), to_cspan(topright),
                                    to_cspan(bottomleft),
                                    to_cspan(bottomright), roi, nthreads);
}

bool
imagebufalgo_checker(ImageBuf& dst, int width, int height, int depth,
                     rust::Slice<const float> color1,
                     rust::Slice<const float> color2, int xoffset, int yoffset,
                     int zoffset, const ROI& roi, int nthreads)
{
    if (width < 1 || height < 1 || depth < 1) {
        // These divide the coordinate, and nothing upstream checks them.
        dst.errorfmt("checker: every square dimension must be at least 1, got "
                     "{}x{}x{}",
                     width, height, depth);
        return false;
    }
    return OIIO::ImageBufAlgo::checker(dst, width, height, depth,
                                       to_cspan(color1), to_cspan(color2),
                                       xoffset, yoffset, zoffset, roi,
                                       nthreads);
}

bool
imagebufalgo_noise(ImageBuf& dst, const rust::Str noisetype, float a, float b,
                   bool mono, int seed, const ROI& roi, int nthreads)
{
    return OIIO::ImageBufAlgo::noise(dst, to_string_view(noisetype), a, b, mono,
                                     seed, roi, nthreads);
}

bool
imagebufalgo_render_point(ImageBuf& dst, int x, int y,
                          rust::Slice<const float> color, const ROI& roi,
                          int nthreads)
{
    return OIIO::ImageBufAlgo::render_point(dst, x, y, to_cspan(color), roi,
                                           nthreads);
}

bool
imagebufalgo_render_line(ImageBuf& dst, int x1, int y1, int x2, int y2,
                         rust::Slice<const float> color, bool skip_first_point,
                         const ROI& roi, int nthreads)
{
    return OIIO::ImageBufAlgo::render_line(dst, x1, y1, x2, y2, to_cspan(color),
                                           skip_first_point, roi, nthreads);
}

bool
imagebufalgo_render_box(ImageBuf& dst, int x1, int y1, int x2, int y2,
                        rust::Slice<const float> color, bool fill,
                        const ROI& roi, int nthreads)
{
    if (fill && (x2 < x1 || y2 < y1)) {
        // The filled path intersects an empty region and draws nothing, while
        // reporting success. The outline path accepts either corner order, so
        // only this one needs saying.
        dst.errorfmt("render_box: a filled box needs its first corner above "
                     "and left of its second, got {},{} to {},{}",
                     x1, y1, x2, y2);
        return false;
    }
    return OIIO::ImageBufAlgo::render_box(dst, x1, y1, x2, y2, to_cspan(color),
                                          fill, roi, nthreads);
}

namespace {

// Every glyph failing to rasterise leaves the measured box at its initial
// xbegin = ybegin = INT_MAX, xend = yend = INT_MIN. render_text builds an
// ImageSpec from that without clamping, and the width underflows.
bool
text_renders_something(const rust::Str text)
{
    for (std::size_t i = 0; i < text.size(); ++i) {
        const char c = text.data()[i];
        if (c != '\n' && c != '\r' && c != '\0')
            return true;
    }
    return false;
}

}  // namespace

bool
imagebufalgo_render_text(ImageBuf& dst, int x, int y, const rust::Str text,
                         int fontsize, const rust::Str fontname,
                         rust::Slice<const float> color, int alignx, int aligny,
                         int shadow, const ROI& roi, int nthreads)
{
    if (!text_renders_something(text)) {
        dst.errorfmt("render_text: there is nothing to draw; OpenImageIO "
                     "measures an inverted box for text with no glyphs and "
                     "builds an image from it");
        return false;
    }
    if (fontsize < 1) {
        dst.errorfmt("render_text: the font size must be at least 1, got {}",
                     fontsize);
        return false;
    }
    return OIIO::ImageBufAlgo::render_text(
        dst, x, y, to_string_view(text), fontsize, to_string_view(fontname),
        to_cspan(color), OIIO::ImageBufAlgo::TextAlignX(alignx),
        OIIO::ImageBufAlgo::TextAlignY(aligny), shadow, roi, nthreads);
}

ROI
imagebufalgo_text_size(const rust::Str text, int fontsize,
                       const rust::Str fontname)
{
    if (!text_renders_something(text) || fontsize < 1)
        return ROI();
    ROI measured = OIIO::ImageBufAlgo::text_size(to_string_view(text), fontsize,
                                                 to_string_view(fontname));
    if (!measured.defined())
        return measured;
    // text_size measures x and y and leaves the rest of the region at its
    // default, so z and the channels come back as empty 0..0 ranges. Text is
    // two-dimensional and single-channel; render_text applies exactly this
    // fixup to the same measurement before using it.
    measured.zbegin  = 0;
    measured.zend    = 1;
    measured.chbegin = 0;
    measured.chend   = 1;
    return measured;
}

bool
imagebufalgo_flatten(ImageBuf& dst, const ImageBuf& src, const ROI& roi,
                     int nthreads)
{
    if (refuse_empty_region(dst, src, roi, "flatten"))
        return false;
    if (dst.initialized() && dst.nchannels() > src.nchannels()) {
        // The kernel allocates its accumulator with src.nchannels() floats and
        // then writes up to roi.chend, which IBAprep clamps only to the larger
        // of the two buffers. A wider destination reads stack past the end of
        // that allocation.
        dst.errorfmt("flatten: the destination has {} channels and the source "
                     "has {}; OpenImageIO reads past its own accumulator when "
                     "the destination is wider",
                     dst.nchannels(), src.nchannels());
        return false;
    }
    return OIIO::ImageBufAlgo::flatten(dst, src, roi, nthreads);
}

bool
imagebufalgo_deepen(ImageBuf& dst, const ImageBuf& src, float zvalue,
                    const ROI& roi, int nthreads)
{
    if (refuse_empty_region(dst, src, roi, "deepen"))
        return false;
    if (dst.initialized()) {
        // Only an uninitialized destination gets the deep, float specification
        // deepen builds. Into a pre-allocated one, writes to channels it does
        // not have are dropped without a word.
        dst.errorfmt("deepen: the destination must be empty, so it can take "
                     "the deep specification this builds");
        return false;
    }
    return OIIO::ImageBufAlgo::deepen(dst, src, zvalue, roi, nthreads);
}

bool
imagebufalgo_deep_merge(ImageBuf& dst, const ImageBuf& a, const ImageBuf& b,
                        bool occlusion_cull, const ROI& roi, int nthreads)
{
    if (require_deep(dst, a, "deep_merge", "first image")
        || require_deep(dst, b, "deep_merge", "second image")
        || refuse_flat_destination(dst, "deep_merge")
        || refuse_empty_region(dst, a, roi, "deep_merge"))
        return false;
    return OIIO::ImageBufAlgo::deep_merge(dst, a, b, occlusion_cull, roi,
                                          nthreads);
}

bool
imagebufalgo_deep_holdout(ImageBuf& dst, const ImageBuf& src,
                          const ImageBuf& holdout, const ROI& roi, int nthreads)
{
    if (require_deep(dst, src, "deep_holdout", "source")
        || require_deep(dst, holdout, "deep_holdout", "holdout")
        || refuse_flat_destination(dst, "deep_holdout")
        || refuse_empty_region(dst, src, roi, "deep_holdout"))
        return false;
    return OIIO::ImageBufAlgo::deep_holdout(dst, src, holdout, roi, nthreads);
}

bool
imagebufalgo_make_kernel(ImageBuf& dst, const rust::Str name, float width,
                         float height, float depth, bool normalize)
{
    dst = OIIO::ImageBufAlgo::make_kernel(to_string_view(name), width, height,
                                          depth, normalize);
    // An unrecognised filter name still yields a box kernel, with the
    // complaint left on the buffer. Convolving with a silent substitute would
    // be worse than reporting it.
    return !dst.has_error();
}

bool
imagebufalgo_convolve(ImageBuf& dst, const ImageBuf& src,
                      const ImageBuf& kernel, bool normalize, const ROI& roi,
                      int nthreads)
{
    if (refuse_empty_region(dst, src, roi, "convolve"))
        return false;
    if (!kernel.initialized() || kernel.roi().npixels() == 0
        || kernel.nchannels() < 1) {
        // Otherwise the kernel sum stays zero, normalising divides by it, and
        // every pixel comes back NaN with the call reporting success.
        dst.errorfmt("convolve: the kernel is empty");
        return false;
    }
    // The kernel is a third image, so IBAprep never sees it.
    if (refuse_deep(dst, kernel, "convolve", "kernel"))
        return false;
    return OIIO::ImageBufAlgo::convolve(dst, src, kernel, normalize, roi,
                                        nthreads);
}

bool
imagebufalgo_laplacian(ImageBuf& dst, const ImageBuf& src, const ROI& roi,
                       int nthreads)
{
    if (refuse_empty_region(dst, src, roi, "laplacian"))
        return false;
    return OIIO::ImageBufAlgo::laplacian(dst, src, roi, nthreads);
}

bool
imagebufalgo_unsharp_mask(ImageBuf& dst, const ImageBuf& src,
                          const rust::Str kernel, float width, float contrast,
                          float threshold, const ROI& roi, int nthreads)
{
    if (refuse_empty_region(dst, src, roi, "unsharp_mask"))
        return false;
    if (dst.initialized() && dst.spec().format != src.spec().format) {
        // The final pass dispatches on the destination's type and then reads
        // the source through an iterator of that same type. ImageBuf iterators
        // reinterpret the buffer rather than converting, so a mismatch misreads
        // the source, and reads past its end when the destination is wider.
        dst.errorfmt("unsharp_mask: the destination is {} and the source is "
                     "{}; OpenImageIO reads the source as the destination's "
                     "type, so they must match, or pass an empty destination",
                     dst.spec().format, src.spec().format);
        return false;
    }
    return OIIO::ImageBufAlgo::unsharp_mask(dst, src, to_string_view(kernel),
                                            width, contrast, threshold, roi,
                                            nthreads);
}

namespace {

// A width of one gives w_2 = max(1, 0) = 1, so the window is [x-1, x): one
// pixel, one to the left. The image comes back translated rather than
// unchanged, which no caller asking for a 1-pixel filter wants.
bool
check_window(ImageBuf& dst, const char* operation, int width, int height)
{
    if (width < 2) {
        dst.errorfmt("{}: the window must be at least 2 wide, got {}; "
                     "OpenImageIO translates the image by one pixel for 1",
                     operation, width);
        return false;
    }
    if (height > 0 && height < 2) {
        dst.errorfmt("{}: the window must be at least 2 high, got {}; "
                     "pass 0 or -1 to match the width",
                     operation, height);
        return false;
    }
    return true;
}

// dilate and erode leave -FLT_MAX or +FLT_MAX in any destination pixel that
// had no source pixel under it, and report success. Keeping the region inside
// the source's data window means that case cannot arise.
ROI
within_source(const ImageBuf& src, const ROI& roi)
{
    if (!roi.defined())
        return src.roi();
    return OIIO::roi_intersection(roi, src.roi());
}

}  // namespace

bool
imagebufalgo_median_filter(ImageBuf& dst, const ImageBuf& src, int width,
                           int height, const ROI& roi, int nthreads)
{
    if (refuse_empty_region(dst, src, roi, "median_filter"))
        return false;
    if (!check_window(dst, "median_filter", width, height))
        return false;
    return OIIO::ImageBufAlgo::median_filter(dst, src, width, height, roi,
                                             nthreads);
}

bool
imagebufalgo_dilate(ImageBuf& dst, const ImageBuf& src, int width, int height,
                    const ROI& roi, int nthreads)
{
    if (refuse_empty_region(dst, src, roi, "dilate"))
        return false;
    if (!check_window(dst, "dilate", width, height))
        return false;
    return OIIO::ImageBufAlgo::dilate(dst, src, width, height,
                                      within_source(src, roi), nthreads);
}

bool
imagebufalgo_erode(ImageBuf& dst, const ImageBuf& src, int width, int height,
                   const ROI& roi, int nthreads)
{
    if (refuse_empty_region(dst, src, roi, "erode"))
        return false;
    if (!check_window(dst, "erode", width, height))
        return false;
    return OIIO::ImageBufAlgo::erode(dst, src, width, height,
                                     within_source(src, roi), nthreads);
}

bool
imagebufalgo_fft(ImageBuf& dst, const ImageBuf& src, const ROI& roi,
                 int nthreads)
{
    // fft never calls IBAprep; it goes straight to paste, which walks the
    // source's pixels, and then to hfft_, whose assertions a release build
    // compiles away.
    if (refuse_deep(dst, src, "fft", "source"))
        return false;
    if (!src.initialized() || src.roi().npixels() == 0) {
        dst.errorfmt("fft: the source holds no pixels to transform");
        return false;
    }

    // fft transforms the union of the data and display windows, allocating a
    // complex buffer that size. Those two windows are independent — moving a
    // data window without moving the display window leaves them far apart —
    // and the union of two distant windows spans everything between. A 10x3
    // image whose data window sits at 100000,100000 with its display window
    // still at the origin asks for 100010 x 100003 pixels: the allocation
    // fails, and OpenImageIO then zeroes a buffer it never got, which takes
    // the process down.
    //
    // Zero-padding out to a display window a few times the data window is
    // ordinary; a union orders of magnitude larger is two windows that
    // disagree, not padding.
    const ROI transformed = OIIO::roi_union(src.roi(), src.roi_full());
    const OIIO::imagesize_t held      = src.roi().npixels();
    const OIIO::imagesize_t requested = transformed.npixels();
    if (requested > held * 16 + 1024) {
        dst.errorfmt(
            "fft: the source's pixels span {},{} to {},{} but its display "
            "window spans {},{} to {},{}, so the transform would cover {} "
            "pixels for an image holding {}. Crop the source, or give it a "
            "display window near its pixels",
            src.roi().xbegin, src.roi().ybegin, src.roi().xend, src.roi().yend,
            src.roi_full().xbegin, src.roi_full().ybegin, src.roi_full().xend,
            src.roi_full().yend, requested, held);
        return false;
    }
    return OIIO::ImageBufAlgo::fft(dst, src, roi, nthreads);
}

bool
imagebufalgo_ifft(ImageBuf& dst, const ImageBuf& src, const ROI& roi,
                  int nthreads)
{
    if (refuse_deep(dst, src, "ifft", "source"))
        return false;
    if (!src.initialized() || src.roi().npixels() == 0) {
        dst.errorfmt("ifft: the source holds no pixels to transform");
        return false;
    }
    if (!src.localpixels()) {
        // hfft_ casts src.pixeladdr(...) to a complex pointer behind an
        // assertion that a release build compiles out. A cache-backed buffer
        // answers that address with null.
        dst.errorfmt("ifft: the source's pixels are not in memory; read the "
                     "image before transforming it");
        return false;
    }

    // Being in memory is not enough. ifft transforms the union of the source's
    // data and display windows, allocates its working buffer at the origin
    // with that size, and then reads the SOURCE at those same coordinates —
    // hfft_'s own assertion says dst.roi() == src.roi(), and nothing enforces
    // it. So a data window that does not start at the origin, or that is
    // smaller than the display window, walks off the allocation: the first
    // crashes, the second quietly returns heap contents as pixels. An overscan
    // EXR is exactly the second shape, and so is anything algo::crop produced.
    const ROI transformed = OIIO::roi_union(src.roi(), src.roi_full());
    if (src.spec().x != 0 || src.spec().y != 0 || src.spec().z != 0
        || !src.roi().contains(transformed)) {
        dst.errorfmt(
            "ifft: the source's pixels must start at the origin and cover its "
            "display window. This one holds {},{} to {},{} with a display "
            "window of {},{} to {},{}; OpenImageIO would read outside it",
            src.roi().xbegin, src.roi().ybegin, src.roi().xend, src.roi().yend,
            src.roi_full().xbegin, src.roi_full().ybegin, src.roi_full().xend,
            src.roi_full().yend);
        return false;
    }
    return OIIO::ImageBufAlgo::ifft(dst, src, roi, nthreads);
}

bool
imagebufalgo_polar_to_complex(ImageBuf& dst, const ImageBuf& src, const ROI& roi,
                              int nthreads)
{
    if (refuse_empty_region(dst, src, roi, "polar_to_complex"))
        return false;
    return OIIO::ImageBufAlgo::polar_to_complex(dst, src, roi, nthreads);
}

bool
imagebufalgo_complex_to_polar(ImageBuf& dst, const ImageBuf& src, const ROI& roi,
                              int nthreads)
{
    if (refuse_empty_region(dst, src, roi, "complex_to_polar"))
        return false;
    return OIIO::ImageBufAlgo::complex_to_polar(dst, src, roi, nthreads);
}

bool
imagebufalgo_rotate90(ImageBuf& dst, const ImageBuf& src, const ROI& roi,
                      int nthreads)
{
    if (refuse_empty_region(dst, src, roi, "rotate90"))
        return false;
    return OIIO::ImageBufAlgo::rotate90(dst, src, roi, nthreads);
}

bool
imagebufalgo_rotate180(ImageBuf& dst, const ImageBuf& src, const ROI& roi,
                       int nthreads)
{
    if (refuse_empty_region(dst, src, roi, "rotate180"))
        return false;
    return OIIO::ImageBufAlgo::rotate180(dst, src, roi, nthreads);
}

bool
imagebufalgo_rotate270(ImageBuf& dst, const ImageBuf& src, const ROI& roi,
                       int nthreads)
{
    if (refuse_empty_region(dst, src, roi, "rotate270"))
        return false;
    return OIIO::ImageBufAlgo::rotate270(dst, src, roi, nthreads);
}

bool
imagebufalgo_reorient(ImageBuf& dst, const ImageBuf& src, int nthreads)
{
    const bool succeeded = OIIO::ImageBufAlgo::reorient(dst, src, nthreads);
    if (!succeeded && !dst.has_error()) {
        // reorient's switch has no default arm, so an Orientation outside 1..8
        // leaves dst untouched and records nothing. Say what happened rather
        // than returning an empty error.
        dst.errorfmt("reorient: the source's Orientation is {}, which is not "
                     "one of the eight EXIF orientations",
                     src.orientation());
    }
    return succeeded;
}

bool
imagebufalgo_rotate(ImageBuf& dst, const ImageBuf& src, float angle,
                    bool has_center, float center_x, float center_y,
                    const rust::Str filtername, float filterwidth,
                    bool recompute_roi, const ROI& roi, int nthreads)
{
    if (refuse_empty_region(dst, src, roi, "rotate"))
        return false;
    if (refuse_narrow_destination(dst, src, "rotate"))
        return false;
    if (!has_center)
        return OIIO::ImageBufAlgo::rotate(dst, src, angle,
                                          to_string_view(filtername),
                                          filterwidth, recompute_roi, roi,
                                          nthreads);
    return OIIO::ImageBufAlgo::rotate(dst, src, angle, center_x, center_y,
                                      to_string_view(filtername), filterwidth,
                                      recompute_roi, roi, nthreads);
}

bool
imagebufalgo_warp(ImageBuf& dst, const ImageBuf& src,
                  rust::Slice<const float> matrix, const rust::Str filtername,
                  float filterwidth, const rust::Str wrap, bool edgeclamp,
                  bool recompute_roi, const ROI& roi, int nthreads)
{
    if (refuse_empty_region(dst, src, roi, "warp"))
        return false;
    if (matrix.size() != 9) {
        dst.errorfmt("warp: the transform needs nine values, got {}",
                     matrix.size());
        return false;
    }
    if (refuse_narrow_destination(dst, src, "warp"))
        return false;
    float m[3][3];
    for (std::size_t row = 0; row < 3; ++row)
        for (std::size_t column = 0; column < 3; ++column)
            m[row][column] = matrix[row * 3 + column];

    // Only names OpenImageIO recognises are sent: IBA_check_optional's report
    // of an unknown one is discarded upstream, so a typo would be silent.
    OIIO::ParamValueList options;
    if (filtername.size() != 0)
        options["filtername"] = std::string(to_string_view(filtername));
    if (filterwidth > 0.0f)
        options["filterwidth"] = filterwidth;
    if (wrap.size() != 0)
        options["wrap"] = std::string(to_string_view(wrap));
    options["edgeclamp"]     = int(edgeclamp);
    options["recompute_roi"] = int(recompute_roi);

    return OIIO::ImageBufAlgo::warp(dst, src, m, options, roi, nthreads);
}

bool
imagebufalgo_st_warp(ImageBuf& dst, const ImageBuf& src, const ImageBuf& stbuf,
                     const rust::Str filtername, float filterwidth, int chan_s,
                     int chan_t, bool flip_s, bool flip_t, const ROI& roi,
                     int nthreads)
{
    if (refuse_empty_region(dst, src, roi, "st_warp"))
        return false;
    // OpenImageIO checks these against stbuf's channel count but never against
    // zero, and a negative index reads out of bounds. The safe wrapper takes
    // them as unsigned, so this is the belt to that braces.
    if (chan_s < 0 || chan_t < 0) {
        dst.errorfmt("st_warp: channel indices must not be negative, got "
                     "{} and {}",
                     chan_s, chan_t);
        return false;
    }
    // stbuf is a third image, so IBAprep never sees it.
    if (refuse_deep(dst, stbuf, "st_warp", "coordinate map"))
        return false;
    return OIIO::ImageBufAlgo::st_warp(dst, src, stbuf,
                                       to_string_view(filtername), filterwidth,
                                       chan_s, chan_t, flip_s, flip_t, roi,
                                       nthreads);
}

bool
imagebufalgo_mad_iii(ImageBuf& dst, const ImageBuf& a, const ImageBuf& b,
                     const ImageBuf& c, const ROI& roi, int nthreads)
{
    if (refuse_distant_pair(dst, a, b, "mad")
        || refuse_deep_mismatch(dst, a, b, "mad")
        || refuse_channel_mismatch(dst, a, b, "mad")
        // a*b+c reads every image operand to the union channel count, so c has
        // to line up too, not only a and b.
        || refuse_channel_mismatch(dst, a, c, "mad"))
        return false;
    return OIIO::ImageBufAlgo::mad(dst, a, b, c, roi, nthreads);
}

bool
imagebufalgo_mad_iic(ImageBuf& dst, const ImageBuf& a, const ImageBuf& b,
                     rust::Slice<const float> c, const ROI& roi, int nthreads)
{
    if (refuse_distant_pair(dst, a, b, "mad")
        || refuse_deep_mismatch(dst, a, b, "mad")
        || refuse_channel_mismatch(dst, a, b, "mad"))
        return false;
    return OIIO::ImageBufAlgo::mad(dst, a, b, to_cspan(c), roi, nthreads);
}

bool
imagebufalgo_mad_ici(ImageBuf& dst, const ImageBuf& a,
                     rust::Slice<const float> b, const ImageBuf& c,
                     const ROI& roi, int nthreads)
{
    if (refuse_distant_pair(dst, a, c, "mad")
        || refuse_deep_mismatch(dst, a, c, "mad")
        // b is a constant; a and c are both images and must line up.
        || refuse_channel_mismatch(dst, a, c, "mad"))
        return false;
    return OIIO::ImageBufAlgo::mad(dst, a, to_cspan(b), c, roi, nthreads);
}

bool
imagebufalgo_mad_icc(ImageBuf& dst, const ImageBuf& a,
                     rust::Slice<const float> b, rust::Slice<const float> c,
                     const ROI& roi, int nthreads)
{
    return OIIO::ImageBufAlgo::mad(dst, a, to_cspan(b), to_cspan(c), roi,
                                   nthreads);
}

bool
imagebufalgo_invert(ImageBuf& dst, const ImageBuf& a, const ROI& roi,
                    int nthreads)
{
    if (refuse_empty_region(dst, a, roi, "invert"))
        return false;
    return OIIO::ImageBufAlgo::invert(dst, a, roi, nthreads);
}

bool
imagebufalgo_pow(ImageBuf& dst, const ImageBuf& a, rust::Slice<const float> b,
                 const ROI& roi, int nthreads)
{
    if (refuse_empty_region(dst, a, roi, "pow"))
        return false;
    return OIIO::ImageBufAlgo::pow(dst, a, to_cspan(b), roi, nthreads);
}

bool
imagebufalgo_clamp(ImageBuf& dst, const ImageBuf& src,
                   rust::Slice<const float> min, rust::Slice<const float> max,
                   bool clampalpha01, const ROI& roi, int nthreads)
{
    if (refuse_empty_region(dst, src, roi, "clamp"))
        return false;
    return OIIO::ImageBufAlgo::clamp(dst, src, to_cspan(min), to_cspan(max),
                                     clampalpha01, roi, nthreads);
}

bool
imagebufalgo_min_images(ImageBuf& dst, const ImageBuf& a, const ImageBuf& b,
                        const ROI& roi, int nthreads)
{
    if (refuse_distant_pair(dst, a, b, "min")
        || refuse_deep_mismatch(dst, a, b, "min"))
        return false;
    return OIIO::ImageBufAlgo::min(dst, a, b, roi, nthreads);
}

bool
imagebufalgo_min_constant(ImageBuf& dst, const ImageBuf& a,
                          rust::Slice<const float> values, const ROI& roi,
                          int nthreads)
{
    return OIIO::ImageBufAlgo::min(dst, a, to_cspan(values), roi, nthreads);
}

bool
imagebufalgo_max_images(ImageBuf& dst, const ImageBuf& a, const ImageBuf& b,
                        const ROI& roi, int nthreads)
{
    if (refuse_distant_pair(dst, a, b, "max")
        || refuse_deep_mismatch(dst, a, b, "max"))
        return false;
    // OpenImageIO's image/image max is not memory-safe when the channel counts
    // disagree. imagebufalgo_pixelmath.cpp:169 reads
    //
    //     roi.chend = std::max(roi.chend, std::max(A.nchannels(), B.nchannels()));
    //
    // where min at line 72 uses std::min twice. The widening is unconditional,
    // so no ROI a caller supplies can prevent it, and it happens after IBAprep
    // has clamped the range to what the buffers actually hold. The kernel then
    // reads a[c] and b[c] past the shorter input, and writes r[c] past dst if
    // dst has fewer channels than the inputs; ImageBuf's iterators do no
    // bounds checking. max's own OIIO_ASSERT(roi.chend <= dst.nchannels()),
    // in the now-unreachable block below the dispatch, records the intent that
    // was lost. Present in 3.1.9 and still in 3.2.0.2dev.
    //
    // So refuse exactly the shapes that would run off the end, and clamp the
    // channel range so the widening is a no-op. min needs none of this.
    if (a.nchannels() != b.nchannels()) {
        dst.errorfmt(
            "max: the two images must have the same number of channels, got "
            "{} and {} (OpenImageIO's max reads out of bounds otherwise)",
            a.nchannels(), b.nchannels());
        return false;
    }
    if (dst.initialized() && dst.nchannels() < a.nchannels()) {
        dst.errorfmt(
            "max: the destination has {} channels but the sources have {} "
            "(OpenImageIO's max writes out of bounds otherwise)",
            dst.nchannels(), a.nchannels());
        return false;
    }

    ROI bounded = roi;
    if (!bounded.defined())
        bounded = OIIO::roi_union(a.roi(), b.roi());
    bounded.chbegin = std::max(bounded.chbegin, 0);
    bounded.chend   = std::min(bounded.chend, a.nchannels());
    return OIIO::ImageBufAlgo::max(dst, a, b, bounded, nthreads);
}

bool
imagebufalgo_max_constant(ImageBuf& dst, const ImageBuf& a,
                          rust::Slice<const float> values, const ROI& roi,
                          int nthreads)
{
    // The image/constant path is the correct mirror of min's and needs no
    // guard: it goes through IBAprep_CLAMP_MUTUAL_NCHANNELS.
    return OIIO::ImageBufAlgo::max(dst, a, to_cspan(values), roi, nthreads);
}

bool
imagebufalgo_contrast_remap(ImageBuf& dst, const ImageBuf& src,
                            rust::Slice<const float> black,
                            rust::Slice<const float> white,
                            rust::Slice<const float> min,
                            rust::Slice<const float> max,
                            rust::Slice<const float> scontrast,
                            rust::Slice<const float> sthresh, const ROI& roi,
                            int nthreads)
{
    if (refuse_empty_region(dst, src, roi, "contrast_remap"))
        return false;
    return OIIO::ImageBufAlgo::contrast_remap(dst, src, to_cspan(black),
                                              to_cspan(white), to_cspan(min),
                                              to_cspan(max),
                                              to_cspan(scontrast),
                                              to_cspan(sthresh), roi, nthreads);
}

bool
imagebufalgo_saturate(ImageBuf& dst, const ImageBuf& src, float scale,
                      int firstchannel, const ROI& roi, int nthreads)
{
    if (refuse_empty_region(dst, src, roi, "saturate"))
        return false;
    return OIIO::ImageBufAlgo::saturate(dst, src, scale, firstchannel, roi,
                                        nthreads);
}

bool
imagebufalgo_paste(ImageBuf& dst, int xbegin, int ybegin, int zbegin,
                   int chbegin, const ImageBuf& src, const ROI& srcroi,
                   int nthreads)
{
    if (refuse_mixed_deep(dst, src, "paste"))
        return false;

    // chbegin offsets the source's channels into the destination and is not
    // clamped anywhere. Far enough negative and the destination's channel
    // count goes negative too, at which point IBAprep hands it to
    // default_channel_names(), which is noexcept and reserves that many —
    // std::terminate, with nothing printed.
    const ROI source = srcroi.defined() ? srcroi : src.roi();
    if (chbegin + source.chend <= 0) {
        dst.errorfmt("paste: a first channel of {} leaves the source's {} "
                     "channels entirely outside the destination",
                     chbegin, source.nchannels());
        return false;
    }

    if (!OIIO::ImageBufAlgo::paste(dst, xbegin, ybegin, zbegin, chbegin, src,
                                   srcroi, nthreads))
        return false;
    if (!dst.initialized()) {
        // OpenImageIO can leave the destination unallocated and still report
        // success, having recorded its complaint through an assertion rather
        // than an error.
        dst.errorfmt("paste: the destination could not be sized from a first "
                     "channel of {}",
                     chbegin);
        return false;
    }
    return true;
}

bool
imagebufalgo_cut(ImageBuf& dst, const ImageBuf& src, const ROI& roi,
                 int nthreads)
{
    if (refuse_empty_region(dst, src, roi, "cut"))
        return false;
    return OIIO::ImageBufAlgo::cut(dst, src, roi, nthreads);
}

bool
imagebufalgo_make_texture_from_buffer(int32_t mode, const ImageBuf& input,
                                      const rust::Str outputfilename,
                                      const ImageSpec& config,
                                      rust::String& error)
{
    OIIO::ImageBufAlgo::MakeTextureMode texture_mode;
    if (!to_texture_mode(mode, texture_mode)) {
        error = rust::String::lossy("unknown make_texture mode");
        return false;
    }

    std::ostringstream printed;
    const bool succeeded
        = OIIO::ImageBufAlgo::make_texture(texture_mode, input,
                                           std::string(to_string_view(
                                               outputfilename)),
                                           config, &printed);
    if (!succeeded)
        collect_texture_failure(printed, error);
    return succeeded;
}

bool
imagebufalgo_make_texture_from_file(int32_t mode, const rust::Str filename,
                                    const rust::Str outputfilename,
                                    const ImageSpec& config,
                                    rust::String& error)
{
    OIIO::ImageBufAlgo::MakeTextureMode texture_mode;
    if (!to_texture_mode(mode, texture_mode)) {
        error = rust::String::lossy("unknown make_texture mode");
        return false;
    }

    std::ostringstream printed;
    const bool succeeded
        = OIIO::ImageBufAlgo::make_texture(texture_mode,
                                           std::string(
                                               to_string_view(filename)),
                                           std::string(to_string_view(
                                               outputfilename)),
                                           config, &printed);
    if (!succeeded)
        collect_texture_failure(printed, error);
    return succeeded;
}

}  // namespace oiio
