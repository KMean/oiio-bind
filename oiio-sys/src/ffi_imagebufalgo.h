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

// The right-angle rotations. Their `roi` selects part of the SOURCE, unlike
// most of the calls above.
bool
imagebufalgo_rotate90(ImageBuf& dst, const ImageBuf& src, const ROI& roi,
                      int nthreads);

bool
imagebufalgo_rotate180(ImageBuf& dst, const ImageBuf& src, const ROI& roi,
                       int nthreads);

bool
imagebufalgo_rotate270(ImageBuf& dst, const ImageBuf& src, const ROI& roi,
                       int nthreads);

// reorient takes no ROI at all, and returns false without recording anything
// when the source's Orientation attribute is outside 1..8; the shim supplies
// the message OpenImageIO omits.
bool
imagebufalgo_reorient(ImageBuf& dst, const ImageBuf& src, int nthreads);

// `angle` is in radians and turns clockwise, because y points down. An empty
// filter name means lanczos3, and a filter width of zero means that filter's
// own default. The wrap mode is fixed at black inside OpenImageIO; warp is the
// way to choose another.
//
// When `has_center` is false the overload that computes its own centre is
// called, so the default stays OpenImageIO's rather than being recomputed here
// from the display window and risking a different answer.
bool
imagebufalgo_rotate(ImageBuf& dst, const ImageBuf& src, float angle,
                    bool has_center, float center_x, float center_y,
                    const rust::Str filtername, float filterwidth,
                    bool recompute_roi, const ROI& roi, int nthreads);

// warp takes its options as OpenImageIO keyword arguments, which are assembled
// here rather than exposed across the bridge. `matrix` is nine floats in row
// order. An unrecognised option name would be ignored silently, so only the
// names OpenImageIO reads are ever sent.
bool
imagebufalgo_warp(ImageBuf& dst, const ImageBuf& src,
                  rust::Slice<const float> matrix, const rust::Str filtername,
                  float filterwidth, const rust::Str wrap, bool edgeclamp,
                  bool recompute_roi, const ROI& roi, int nthreads);

bool
imagebufalgo_st_warp(ImageBuf& dst, const ImageBuf& src, const ImageBuf& stbuf,
                     const rust::Str filtername, float filterwidth,
                     int chan_s, int chan_t, bool flip_s, bool flip_t,
                     const ROI& roi, int nthreads);

// OpenImageIO's mad, min and max take Image_or_Const, a parameter-passing
// class with a private tagged union that cxx cannot construct. Each
// combination gets its own entry point, as add/sub/mul/div already do.
bool
imagebufalgo_mad_iii(ImageBuf& dst, const ImageBuf& a, const ImageBuf& b,
                     const ImageBuf& c, const ROI& roi, int nthreads);

bool
imagebufalgo_mad_iic(ImageBuf& dst, const ImageBuf& a, const ImageBuf& b,
                     rust::Slice<const float> c, const ROI& roi, int nthreads);

bool
imagebufalgo_mad_ici(ImageBuf& dst, const ImageBuf& a,
                     rust::Slice<const float> b, const ImageBuf& c,
                     const ROI& roi, int nthreads);

bool
imagebufalgo_mad_icc(ImageBuf& dst, const ImageBuf& a,
                     rust::Slice<const float> b, rust::Slice<const float> c,
                     const ROI& roi, int nthreads);

bool
imagebufalgo_invert(ImageBuf& dst, const ImageBuf& a, const ROI& roi,
                    int nthreads);

// An empty `b` means an exponent of zero for every channel, so the result is
// one everywhere. That is OpenImageIO's padding rule, not an oversight, and
// the safe wrapper passes an empty slice through rather than substituting a
// default.
bool
imagebufalgo_pow(ImageBuf& dst, const ImageBuf& a, rust::Slice<const float> b,
                 const ROI& roi, int nthreads);

// Here an empty span means "do not clamp on this side", which is a different
// rule from pow's. Both are OpenImageIO's.
bool
imagebufalgo_clamp(ImageBuf& dst, const ImageBuf& src,
                   rust::Slice<const float> min,
                   rust::Slice<const float> max, bool clampalpha01,
                   const ROI& roi, int nthreads);

bool
imagebufalgo_min_images(ImageBuf& dst, const ImageBuf& a, const ImageBuf& b,
                        const ROI& roi, int nthreads);

bool
imagebufalgo_min_constant(ImageBuf& dst, const ImageBuf& a,
                          rust::Slice<const float> values, const ROI& roi,
                          int nthreads);

// See the implementation: the image/image form of max is not memory-safe in
// OpenImageIO 3.1 or 3.2, and this shim refuses the cases that would read or
// write out of bounds rather than passing them through.
bool
imagebufalgo_max_images(ImageBuf& dst, const ImageBuf& a, const ImageBuf& b,
                        const ROI& roi, int nthreads);

bool
imagebufalgo_max_constant(ImageBuf& dst, const ImageBuf& a,
                          rust::Slice<const float> values, const ROI& roi,
                          int nthreads);

// Six per-channel spans, each with its own default when empty: black 0,
// white 1, min 0, max 1, scontrast 1, sthresh 0.5.
bool
imagebufalgo_contrast_remap(ImageBuf& dst, const ImageBuf& src,
                            rust::Slice<const float> black,
                            rust::Slice<const float> white,
                            rust::Slice<const float> min,
                            rust::Slice<const float> max,
                            rust::Slice<const float> scontrast,
                            rust::Slice<const float> sthresh, const ROI& roi,
                            int nthreads);

bool
imagebufalgo_saturate(ImageBuf& dst, const ImageBuf& src, float scale,
                      int firstchannel, const ROI& roi, int nthreads);

// `srcroi` selects part of the SOURCE, unlike every other roi here, and the
// offsets are relative to src's origin rather than to srcroi.
bool
imagebufalgo_paste(ImageBuf& dst, int xbegin, int ybegin, int zbegin,
                   int chbegin, const ImageBuf& src, const ROI& srcroi,
                   int nthreads);

bool
imagebufalgo_cut(ImageBuf& dst, const ImageBuf& src, const ROI& roi,
                 int nthreads);

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
