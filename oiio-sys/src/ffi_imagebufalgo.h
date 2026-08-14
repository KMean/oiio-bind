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
struct RangeCheckCounts;
struct PixelStatistics;

// The measurements. None of these fills a destination buffer, so there is no
// dst to carry an error; each reports through an out parameter instead.
//
// Several also need guarding. OpenImageIO's statistics do not all clamp their
// region the way the rest of ImageBufAlgo does, and none of them checks for a
// deep image, whose iterators have no pixel pointer at all. The guards are
// noted at each one.

// The success test inside OpenImageIO is `!src.has_error()`, which is the
// buffer's sticky flag rather than anything this call did, so a stale error
// left on `src` by an earlier operation would be read as this one failing.
// Clearing it first is what makes the result mean what it says.
PixelStatistics
imagebufalgo_pixel_stats(const ImageBuf& src, const ROI& roi, int nthreads);

// histogram is the one statistic that never clamps the channel range to the
// image's channel count. A default-constructed ROI carries chend = 10000, and
// with ignore_empty the inner loop reads every channel up to it.
rust::Vec<uint64_t>
imagebufalgo_histogram(const ImageBuf& src, int channel, int bins, float min,
                       float max, bool ignore_empty, const ROI& roi,
                       int nthreads, rust::String& error);

// `color` is filled only when the answer is true: OpenImageIO returns early
// on the second differing pixel, before the writeback, so on false the
// caller's buffer would otherwise keep whatever it held.
//
// A region beginning at a channel other than zero is refused: the sizes of
// the reference vector and the loop that fills it disagree, and it writes off
// the end of the heap allocation.
bool
imagebufalgo_is_constant_color(const ImageBuf& src, float threshold,
                               rust::Slice<float> color, const ROI& roi,
                               int nthreads, rust::String& error);

bool
imagebufalgo_is_constant_channel(const ImageBuf& src, int channel, float value,
                                 float threshold, const ROI& roi, int nthreads,
                                 rust::String& error);

bool
imagebufalgo_is_monochrome(const ImageBuf& src, float threshold, const ROI& roi,
                           int nthreads, rust::String& error);

// Reaches the same out-of-bounds write as is_constant_color, which it is built
// on, so it carries the same guard.
ROI
imagebufalgo_nonzero_region(const ImageBuf& src, const ROI& roi, int nthreads,
                            rust::String& error);

// The block size is deliberately not exposed. Splitting the work changes the
// digest for the same pixels, which OpenImageIO documents, and a block size
// with a region that does not start at the image's first row indexes past the
// end of the results vector.
rust::String
imagebufalgo_pixel_hash_sha1(const ImageBuf& src, const rust::Str extrainfo,
                             const ROI& roi, int nthreads,
                             rust::String& error);


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

// compare reports through an out parameter like the measurements do, because
// CompareResults is a plain aggregate: when the comparison cannot be made,
// OpenImageIO sets only its `error` flag and every measurement is left
// uninitialised.
CompareSummary
imagebufalgo_compare(const ImageBuf& a, const ImageBuf& b, float failthresh,
                     float warnthresh, float failrelative, float warnrelative,
                     const ROI& roi, int nthreads, rust::String& error);

// Colour space conversion using the default configuration, which is whatever
// $OCIO names or OpenImageIO's built-in one.
bool
imagebufalgo_colorconvert(ImageBuf& dst, const ImageBuf& src,
                          const rust::Str fromspace, const rust::Str tospace,
                          bool unpremult, const ROI& roi, int nthreads);

// The remaining colour operations. Every one of them shares OpenImageIO's
// colorconvert pixel engine, which corrupts channels past the fourth, so each
// shim repairs those afterwards; see the implementation.
//
// None takes a ColorConfig: they always use the default one. Passing nothing
// is not an option, because ociolook dereferences the configuration before it
// checks whether one was given.
bool
imagebufalgo_colormatrixtransform(ImageBuf& dst, const ImageBuf& src,
                                  rust::Slice<const float> matrix,
                                  bool unpremult, const ROI& roi, int nthreads);

bool
imagebufalgo_ociolook(ImageBuf& dst, const ImageBuf& src, const rust::Str looks,
                      const rust::Str fromspace, const rust::Str tospace,
                      bool unpremult, bool inverse, const rust::Str context_key,
                      const rust::Str context_value, const ROI& roi,
                      int nthreads);

bool
imagebufalgo_ociodisplay(ImageBuf& dst, const ImageBuf& src,
                         const rust::Str display, const rust::Str view,
                         const rust::Str fromspace, const rust::Str looks,
                         bool unpremult, bool inverse,
                         const rust::Str context_key,
                         const rust::Str context_value, const ROI& roi,
                         int nthreads);

bool
imagebufalgo_ociofiletransform(ImageBuf& dst, const ImageBuf& src,
                               const rust::Str name, bool unpremult,
                               bool inverse, const ROI& roi, int nthreads);

bool
imagebufalgo_ocionamedtransform(ImageBuf& dst, const ImageBuf& src,
                                const rust::Str name, bool unpremult,
                                bool inverse, const rust::Str context_key,
                                const rust::Str context_value, const ROI& roi,
                                int nthreads);

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
imagebufalgo_repremult(ImageBuf& dst, const ImageBuf& src, const ROI& roi,
                       int nthreads);

bool
imagebufalgo_zover(ImageBuf& dst, const ImageBuf& a, const ImageBuf& b,
                   bool z_zeroisinf, const ROI& roi, int nthreads);

bool
imagebufalgo_scale(ImageBuf& dst, const ImageBuf& a, const ImageBuf& b,
                   const ROI& roi, int nthreads);

bool
imagebufalgo_fix_non_finite(ImageBuf& dst, const ImageBuf& src, int mode,
                            int64_t& pixels_fixed, const ROI& roi,
                            int nthreads);

bool
imagebufalgo_rangecompress(ImageBuf& dst, const ImageBuf& src, bool use_luma,
                           const ROI& roi, int nthreads);

bool
imagebufalgo_rangeexpand(ImageBuf& dst, const ImageBuf& src, bool use_luma,
                         const ROI& roi, int nthreads);

bool
imagebufalgo_channel_append(ImageBuf& dst, const ImageBuf& a, const ImageBuf& b,
                            int nthreads);

bool
imagebufalgo_maxchan(ImageBuf& dst, const ImageBuf& src, const ROI& roi,
                     int nthreads);

bool
imagebufalgo_minchan(ImageBuf& dst, const ImageBuf& src, const ROI& roi,
                     int nthreads);

bool
imagebufalgo_color_range_check(const ImageBuf& src,
                               rust::Slice<const float> low,
                               rust::Slice<const float> high,
                               RangeCheckCounts& counts, const ROI& roi,
                               int nthreads, rust::String& error);

bool
imagebufalgo_color_map(ImageBuf& dst, const ImageBuf& src, int srcchannel,
                       int nknots, int channels, rust::Slice<const float> knots,
                       const ROI& roi, int nthreads);

bool
imagebufalgo_color_map_named(ImageBuf& dst, const ImageBuf& src,
                             int srcchannel, rust::Str mapname, const ROI& roi,
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

// Generators and drawing.

// The two gradient forms of fill. Values are indexed by absolute channel
// number, and the ramp is parameterised over the region rather than the image,
// so filling part of an image gives a full ramp inside that part.
bool
imagebufalgo_fill_vertical(ImageBuf& dst, rust::Slice<const float> top,
                           rust::Slice<const float> bottom, const ROI& roi,
                           int nthreads);

bool
imagebufalgo_fill_corners(ImageBuf& dst, rust::Slice<const float> topleft,
                          rust::Slice<const float> topright,
                          rust::Slice<const float> bottomleft,
                          rust::Slice<const float> bottomright, const ROI& roi,
                          int nthreads);

// The three sizes are divisors, and nothing upstream checks them, so a zero is
// a division by zero rather than an error.
bool
imagebufalgo_checker(ImageBuf& dst, int width, int height, int depth,
                     rust::Slice<const float> color1,
                     rust::Slice<const float> color2, int xoffset, int yoffset,
                     int zoffset, const ROI& roi, int nthreads);

// Noise is added to whatever the destination already holds, rather than
// replacing it; "salt" is the exception and assigns.
bool
imagebufalgo_noise(ImageBuf& dst, const rust::Str noisetype, float a, float b,
                   bool mono, int seed, const ROI& roi, int nthreads);

bool
imagebufalgo_render_point(ImageBuf& dst, int x, int y,
                          rust::Slice<const float> color, const ROI& roi,
                          int nthreads);

bool
imagebufalgo_render_line(ImageBuf& dst, int x1, int y1, int x2, int y2,
                         rust::Slice<const float> color, bool skip_first_point,
                         const ROI& roi, int nthreads);

bool
imagebufalgo_render_box(ImageBuf& dst, int x1, int y1, int x2, int y2,
                        rust::Slice<const float> color, bool fill,
                        const ROI& roi, int nthreads);

// Text whose glyphs all fail to rasterise — an empty string, or one holding
// only line breaks — leaves the measured box inverted, and render_text builds
// an ImageSpec straight from it, underflowing its width. The shim refuses that
// before it can happen.
bool
imagebufalgo_render_text(ImageBuf& dst, int x, int y, const rust::Str text,
                         int fontsize, const rust::Str fontname,
                         rust::Slice<const float> color, int alignx, int aligny,
                         int shadow, const ROI& roi, int nthreads);

// text_size reports failure only by returning an undefined region: it records
// nothing, on the buffer or globally, so the message has to be invented here.
ROI
imagebufalgo_text_size(const rust::Str text, int fontsize,
                       const rust::Str fontname);

// The deep compositing operations. Every one of them reports through `dst`,
// and three of the four return a bool that is not a real success signal: only
// the early guards can make it false. The safe wrappers surface dst's message
// whenever there is one.
//
// flatten reads its per-pixel accumulator past the end of a stack allocation
// when the destination has more channels than the source, so that is refused.
bool
imagebufalgo_flatten(ImageBuf& dst, const ImageBuf& src, const ROI& roi,
                     int nthreads);

bool
imagebufalgo_deepen(ImageBuf& dst, const ImageBuf& src, float zvalue,
                    const ROI& roi, int nthreads);

bool
imagebufalgo_deep_merge(ImageBuf& dst, const ImageBuf& a, const ImageBuf& b,
                        bool occlusion_cull, const ROI& roi, int nthreads);

bool
imagebufalgo_deep_holdout(ImageBuf& dst, const ImageBuf& src,
                          const ImageBuf& holdout, const ROI& roi,
                          int nthreads);

// Filtering. Several of these need guarding; the reasons are at each one and
// in the implementation.

// make_kernel returns a buffer rather than filling one, and on an unknown
// filter name it returns a usable box kernel with the complaint recorded on
// that buffer. Convolving with a silent fallback is worse than failing, so the
// shim checks.
bool
imagebufalgo_make_kernel(ImageBuf& dst, const rust::Str name, float width,
                         float height, float depth, bool normalize);

// An uninitialized kernel is not refused upstream: the accumulation runs zero
// times, normalising divides by that zero, and every pixel comes back NaN with
// the call reporting success.
bool
imagebufalgo_convolve(ImageBuf& dst, const ImageBuf& src,
                      const ImageBuf& kernel, bool normalize, const ROI& roi,
                      int nthreads);

bool
imagebufalgo_laplacian(ImageBuf& dst, const ImageBuf& src, const ROI& roi,
                       int nthreads);

// unsharp_mask dispatches on the destination's pixel type alone and then reads
// the source through it. A destination whose type differs from the source's
// therefore reinterprets the source's bytes, and reads past the end when the
// destination's type is wider.
bool
imagebufalgo_unsharp_mask(ImageBuf& dst, const ImageBuf& src,
                          const rust::Str kernel, float width, float contrast,
                          float threshold, const ROI& roi, int nthreads);

// A width of one is an off-by-one rather than a no-op: the window becomes the
// single pixel one to the left and above, so the image comes back translated.
// dilate and erode additionally write -FLT_MAX or +FLT_MAX into any
// destination pixel that has no source pixel under it, so the region is
// clamped to the source's data window.
bool
imagebufalgo_median_filter(ImageBuf& dst, const ImageBuf& src, int width,
                           int height, const ROI& roi, int nthreads);

bool
imagebufalgo_dilate(ImageBuf& dst, const ImageBuf& src, int width, int height,
                    const ROI& roi, int nthreads);

bool
imagebufalgo_erode(ImageBuf& dst, const ImageBuf& src, int width, int height,
                   const ROI& roi, int nthreads);

bool
imagebufalgo_fft(ImageBuf& dst, const ImageBuf& src, const ROI& roi,
                 int nthreads);

// ifft casts the source's pixel address to a complex pointer and guards it
// only with an assertion, which a release build reduces to a message on
// stderr. A buffer that is backed by the image cache rather than by local
// pixels returns null from that address, so this insists on local pixels.
bool
imagebufalgo_ifft(ImageBuf& dst, const ImageBuf& src, const ROI& roi,
                  int nthreads);

bool
imagebufalgo_polar_to_complex(ImageBuf& dst, const ImageBuf& src, const ROI& roi,
                              int nthreads);

bool
imagebufalgo_complex_to_polar(ImageBuf& dst, const ImageBuf& src, const ROI& roi,
                              int nthreads);

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
