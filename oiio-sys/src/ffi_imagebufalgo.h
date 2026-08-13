#pragma once

#include <OpenImageIO/imagebuf.h>
#include <OpenImageIO/imagebufalgo.h>
#include <rust/cxx.h>
#include <cstdint>

namespace oiio {
using ImageBuf = OIIO::ImageBuf;
using ROI      = OIIO::ROI;
using TypeDesc = OIIO::TypeDesc;

struct CompareSummary;

// Every call below writes into `dst` and reports failure through `dst`'s own
// error channel. An undefined `roi` means the whole image, which is what
// OpenImageIO's default argument expresses.

bool
imagebufalgo_zero(ImageBuf& dst, const ROI& roi, int nthreads);

bool
imagebufalgo_fill(ImageBuf& dst, rust::Slice<const float> values, const ROI& roi,
                  int nthreads);

bool
imagebufalgo_add_images(ImageBuf& dst, const ImageBuf& a, const ImageBuf& b,
                        const ROI& roi, int nthreads);

bool
imagebufalgo_add_constant(ImageBuf& dst, const ImageBuf& a,
                          rust::Slice<const float> values, const ROI& roi,
                          int nthreads);

bool
imagebufalgo_sub_images(ImageBuf& dst, const ImageBuf& a, const ImageBuf& b,
                        const ROI& roi, int nthreads);

bool
imagebufalgo_sub_constant(ImageBuf& dst, const ImageBuf& a,
                          rust::Slice<const float> values, const ROI& roi,
                          int nthreads);

bool
imagebufalgo_mul_images(ImageBuf& dst, const ImageBuf& a, const ImageBuf& b,
                        const ROI& roi, int nthreads);

bool
imagebufalgo_mul_constant(ImageBuf& dst, const ImageBuf& a,
                          rust::Slice<const float> values, const ROI& roi,
                          int nthreads);

bool
imagebufalgo_div_images(ImageBuf& dst, const ImageBuf& a, const ImageBuf& b,
                        const ROI& roi, int nthreads);

bool
imagebufalgo_div_constant(ImageBuf& dst, const ImageBuf& a,
                          rust::Slice<const float> values, const ROI& roi,
                          int nthreads);

bool
imagebufalgo_abs(ImageBuf& dst, const ImageBuf& a, const ROI& roi, int nthreads);

bool
imagebufalgo_absdiff_images(ImageBuf& dst, const ImageBuf& a, const ImageBuf& b,
                            const ROI& roi, int nthreads);

bool
imagebufalgo_copy(ImageBuf& dst, const ImageBuf& src, TypeDesc convert,
                  const ROI& roi, int nthreads);

bool
imagebufalgo_crop(ImageBuf& dst, const ImageBuf& src, const ROI& roi,
                  int nthreads);

bool
imagebufalgo_flip(ImageBuf& dst, const ImageBuf& src, const ROI& roi,
                  int nthreads);

bool
imagebufalgo_flop(ImageBuf& dst, const ImageBuf& src, const ROI& roi,
                  int nthreads);

bool
imagebufalgo_transpose(ImageBuf& dst, const ImageBuf& src, const ROI& roi,
                       int nthreads);

CompareSummary
imagebufalgo_compare(const ImageBuf& a, const ImageBuf& b, float failthresh,
                     float warnthresh, const ROI& roi, int nthreads);

}  // namespace oiio
