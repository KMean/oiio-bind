#include "ffi_imagebufalgo.h"
#include "oiio-sys/src/imagebufalgo.rs.h"

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

}  // namespace oiio
