#pragma once

#include <OpenImageIO/imagebuf.h>
#include <OpenImageIO/imagebufalgo.h>
#include <OpenImageIO/imageio.h>
#include <rust/cxx.h>
#include <cstdint>

namespace oiio {
using ImageBuf  = OIIO::ImageBuf;
using ImageSpec = OIIO::ImageSpec;
using ROI       = OIIO::ROI;
using TypeDesc  = OIIO::TypeDesc;

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

// Colour space conversion using the default configuration, which is whatever
// $OCIO names or OpenImageIO's built-in one.
bool
imagebufalgo_colorconvert(ImageBuf& dst, const ImageBuf& src,
                          const rust::Str fromspace, const rust::Str tospace,
                          bool unpremult, const ROI& roi, int nthreads);

// Resizing takes its filter through OpenImageIO's keyword arguments; the
// options list is assembled here rather than exposed across the bridge. An
// empty filter name asks OpenImageIO to choose one.
bool
imagebufalgo_resize(ImageBuf& dst, const ImageBuf& src,
                    const rust::Str filtername, float filterwidth,
                    const ROI& roi, int nthreads);

bool
imagebufalgo_fit(ImageBuf& dst, const ImageBuf& src, const rust::Str filtername,
                 float filterwidth, const rust::Str fillmode, bool exact,
                 const ROI& roi, int nthreads);

bool
imagebufalgo_resample(ImageBuf& dst, const ImageBuf& src, bool interpolate,
                      const ROI& roi, int nthreads);

bool
imagebufalgo_over(ImageBuf& dst, const ImageBuf& a, const ImageBuf& b,
                  const ROI& roi, int nthreads);

bool
imagebufalgo_premult(ImageBuf& dst, const ImageBuf& src, const ROI& roi,
                     int nthreads);

bool
imagebufalgo_unpremult(ImageBuf& dst, const ImageBuf& src, const ROI& roi,
                       int nthreads);

bool
imagebufalgo_channel_sum(ImageBuf& dst, const ImageBuf& src,
                         rust::Slice<const float> weights, const ROI& roi,
                         int nthreads);

// Channel shuffling. Each output channel takes either an input channel, when
// the matching `channelorder` entry is non-negative, or the constant in
// `channelvalues` at that position.
bool
imagebufalgo_channels(ImageBuf& dst, const ImageBuf& src, int nchannels,
                      rust::Slice<const int> channelorder,
                      rust::Slice<const float> channelvalues,
                      const rust::Vec<rust::String>& newchannelnames,
                      bool shuffle_channel_names, int nthreads);

// Turning an image into a tiled, MIP-mapped texture file. Unlike every call
// above, this one writes a file rather than filling a destination buffer, so
// there is no ImageBuf in which OpenImageIO could record an error: it puts the
// message in the global error channel instead, and explains refusals on the
// stream it is handed. Both are collected into `error`, because neither on its
// own reliably holds the reason.
//
// `mode` is a MakeTextureMode; an out-of-range value is refused here rather
// than cast into whatever the enum happens to have at that position.
bool
imagebufalgo_make_texture_from_buffer(int32_t mode, const ImageBuf& input,
                                      const rust::Str outputfilename,
                                      const ImageSpec& config,
                                      rust::String& error);

bool
imagebufalgo_make_texture_from_file(int32_t mode, const rust::Str filename,
                                    const rust::Str outputfilename,
                                    const ImageSpec& config,
                                    rust::String& error);

}  // namespace oiio
