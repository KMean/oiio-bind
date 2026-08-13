#include "ffi_imagebufalgo.h"
#include "oiio-sys/src/imagebufalgo.rs.h"

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
    error = rust::String(message);
}

}  // namespace

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
    return OIIO::ImageBufAlgo::abs(dst, a, roi, nthreads);
}

bool
imagebufalgo_absdiff_images(ImageBuf& dst, const ImageBuf& a, const ImageBuf& b,
                            const ROI& roi, int nthreads)
{
    return OIIO::ImageBufAlgo::absdiff(dst, a, b, roi, nthreads);
}

bool
imagebufalgo_copy(ImageBuf& dst, const ImageBuf& src, TypeDesc convert,
                  const ROI& roi, int nthreads)
{
    return OIIO::ImageBufAlgo::copy(dst, src, convert, roi, nthreads);
}

bool
imagebufalgo_crop(ImageBuf& dst, const ImageBuf& src, const ROI& roi,
                  int nthreads)
{
    return OIIO::ImageBufAlgo::crop(dst, src, roi, nthreads);
}

bool
imagebufalgo_flip(ImageBuf& dst, const ImageBuf& src, const ROI& roi,
                  int nthreads)
{
    return OIIO::ImageBufAlgo::flip(dst, src, roi, nthreads);
}

bool
imagebufalgo_flop(ImageBuf& dst, const ImageBuf& src, const ROI& roi,
                  int nthreads)
{
    return OIIO::ImageBufAlgo::flop(dst, src, roi, nthreads);
}

bool
imagebufalgo_transpose(ImageBuf& dst, const ImageBuf& src, const ROI& roi,
                       int nthreads)
{
    return OIIO::ImageBufAlgo::transpose(dst, src, roi, nthreads);
}

bool
imagebufalgo_colorconvert(ImageBuf& dst, const ImageBuf& src,
                          const rust::Str fromspace, const rust::Str tospace,
                          bool unpremult, const ROI& roi, int nthreads)
{
    return OIIO::ImageBufAlgo::colorconvert(
        dst, src, OIIO::string_view(fromspace.data(), fromspace.size()),
        OIIO::string_view(tospace.data(), tospace.size()), unpremult, "", "",
        nullptr, roi, nthreads);
}

bool
imagebufalgo_resize(ImageBuf& dst, const ImageBuf& src,
                    const rust::Str filtername, float filterwidth,
                    const ROI& roi, int nthreads)
{
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
    return OIIO::ImageBufAlgo::resample(dst, src, interpolate, roi, nthreads);
}

bool
imagebufalgo_over(ImageBuf& dst, const ImageBuf& a, const ImageBuf& b,
                  const ROI& roi, int nthreads)
{
    return OIIO::ImageBufAlgo::over(dst, a, b, roi, nthreads);
}

bool
imagebufalgo_premult(ImageBuf& dst, const ImageBuf& src, const ROI& roi,
                     int nthreads)
{
    return OIIO::ImageBufAlgo::premult(dst, src, roi, nthreads);
}

bool
imagebufalgo_unpremult(ImageBuf& dst, const ImageBuf& src, const ROI& roi,
                       int nthreads)
{
    return OIIO::ImageBufAlgo::unpremult(dst, src, roi, nthreads);
}

bool
imagebufalgo_channel_sum(ImageBuf& dst, const ImageBuf& src,
                         rust::Slice<const float> weights, const ROI& roi,
                         int nthreads)
{
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
                     float warnthresh, const ROI& roi, int nthreads)
{
    const OIIO::ImageBufAlgo::CompareResults results
        = OIIO::ImageBufAlgo::compare(a, b, failthresh, warnthresh, roi,
                                      nthreads);
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
imagebufalgo_rotate90(ImageBuf& dst, const ImageBuf& src, const ROI& roi,
                      int nthreads)
{
    return OIIO::ImageBufAlgo::rotate90(dst, src, roi, nthreads);
}

bool
imagebufalgo_rotate180(ImageBuf& dst, const ImageBuf& src, const ROI& roi,
                       int nthreads)
{
    return OIIO::ImageBufAlgo::rotate180(dst, src, roi, nthreads);
}

bool
imagebufalgo_rotate270(ImageBuf& dst, const ImageBuf& src, const ROI& roi,
                       int nthreads)
{
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
    if (matrix.size() != 9) {
        dst.errorfmt("warp: the transform needs nine values, got {}",
                     matrix.size());
        return false;
    }
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
    // OpenImageIO checks these against stbuf's channel count but never against
    // zero, and a negative index reads out of bounds. The safe wrapper takes
    // them as unsigned, so this is the belt to that braces.
    if (chan_s < 0 || chan_t < 0) {
        dst.errorfmt("st_warp: channel indices must not be negative, got "
                     "{} and {}",
                     chan_s, chan_t);
        return false;
    }
    return OIIO::ImageBufAlgo::st_warp(dst, src, stbuf,
                                       to_string_view(filtername), filterwidth,
                                       chan_s, chan_t, flip_s, flip_t, roi,
                                       nthreads);
}

bool
imagebufalgo_mad_iii(ImageBuf& dst, const ImageBuf& a, const ImageBuf& b,
                     const ImageBuf& c, const ROI& roi, int nthreads)
{
    return OIIO::ImageBufAlgo::mad(dst, a, b, c, roi, nthreads);
}

bool
imagebufalgo_mad_iic(ImageBuf& dst, const ImageBuf& a, const ImageBuf& b,
                     rust::Slice<const float> c, const ROI& roi, int nthreads)
{
    return OIIO::ImageBufAlgo::mad(dst, a, b, to_cspan(c), roi, nthreads);
}

bool
imagebufalgo_mad_ici(ImageBuf& dst, const ImageBuf& a,
                     rust::Slice<const float> b, const ImageBuf& c,
                     const ROI& roi, int nthreads)
{
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
    return OIIO::ImageBufAlgo::invert(dst, a, roi, nthreads);
}

bool
imagebufalgo_pow(ImageBuf& dst, const ImageBuf& a, rust::Slice<const float> b,
                 const ROI& roi, int nthreads)
{
    return OIIO::ImageBufAlgo::pow(dst, a, to_cspan(b), roi, nthreads);
}

bool
imagebufalgo_clamp(ImageBuf& dst, const ImageBuf& src,
                   rust::Slice<const float> min, rust::Slice<const float> max,
                   bool clampalpha01, const ROI& roi, int nthreads)
{
    return OIIO::ImageBufAlgo::clamp(dst, src, to_cspan(min), to_cspan(max),
                                     clampalpha01, roi, nthreads);
}

bool
imagebufalgo_min_images(ImageBuf& dst, const ImageBuf& a, const ImageBuf& b,
                        const ROI& roi, int nthreads)
{
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
    return OIIO::ImageBufAlgo::saturate(dst, src, scale, firstchannel, roi,
                                        nthreads);
}

bool
imagebufalgo_paste(ImageBuf& dst, int xbegin, int ybegin, int zbegin,
                   int chbegin, const ImageBuf& src, const ROI& srcroi,
                   int nthreads)
{
    return OIIO::ImageBufAlgo::paste(dst, xbegin, ybegin, zbegin, chbegin, src,
                                     srcroi, nthreads);
}

bool
imagebufalgo_cut(ImageBuf& dst, const ImageBuf& src, const ROI& roi,
                 int nthreads)
{
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
        error = rust::String("unknown make_texture mode");
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
        error = rust::String("unknown make_texture mode");
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
