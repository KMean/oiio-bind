#include "ffi_imagecache.h"
#include "ffi_pixel.h"

namespace oiio {
namespace {
rust::String
take_cache_error(ImageCache& imagecache)
{
    return rust::String::lossy(imagecache.geterror(true));
}
}  // namespace

std::shared_ptr<ImageCache>
imagecache_create(bool shared)
{
    return OIIO::ImageCache::create(shared);
}

void
imagecache_destroy(std::shared_ptr<ImageCache> imagecache, bool teardown)
{
    OIIO::ImageCache::destroy(imagecache, teardown);
}

bool
imagecache_attribute(ImageCache& imagecache, rust::Str name, TypeDesc type,
                     const uint8_t* val)
{
    std::string_view c_name(name.data(), name.size());
    return imagecache.attribute(c_name, type, val);
}

bool
imagecache_attribute_int(ImageCache& imagecache, rust::Str name, int val)
{
    std::string_view c_name(name.data(), name.size());
    return imagecache.attribute(c_name, val);
}

bool
imagecache_attribute_int_with_error(ImageCache& imagecache, rust::Str name,
                                    int val, rust::String& error)
{
    error = rust::String();
    if (imagecache_attribute_int(imagecache, name, val))
        return true;
    error = take_cache_error(imagecache);
    return false;
}

bool
imagecache_attribute_float(ImageCache& imagecache, rust::Str name, float val)
{
    std::string_view c_name(name.data(), name.size());
    return imagecache.attribute(c_name, val);
}

bool
imagecache_attribute_float_with_error(ImageCache& imagecache, rust::Str name,
                                      float val, rust::String& error)
{
    error = rust::String();
    if (imagecache_attribute_float(imagecache, name, val))
        return true;
    error = take_cache_error(imagecache);
    return false;
}

bool
imagecache_attribute_double(ImageCache& imagecache, rust::Str name, double val)
{
    std::string_view c_name(name.data(), name.size());
    return imagecache.attribute(c_name, val);
}

bool
imagecache_attribute_str(ImageCache& imagecache, rust::Str name, rust::Str val)
{
    std::string_view c_name(name.data(), name.size());
    std::string_view c_val(val.data(), val.size());
    return imagecache.attribute(c_name, c_val);
}

bool
imagecache_getattribute(ImageCache& imagecache, rust::Str name, TypeDesc type,
                        const uint8_t* val)
{
    std::string_view c_name(name.data(), name.size());
    return imagecache.getattribute(c_name, type, (void*)val);
}

bool
imagecache_getattribute_int(ImageCache& imagecache, rust::Str name, int& val)
{
    std::string_view c_name(name.data(), name.size());
    return imagecache.getattribute(c_name, val);
}

bool
imagecache_getattribute_float(ImageCache& imagecache, rust::Str name,
                              float& val)
{
    std::string_view c_name(name.data(), name.size());
    return imagecache.getattribute(c_name, val);
}

bool
imagecache_getattribute_double(ImageCache& imagecache, rust::Str name,
                               double& val)
{
    std::string_view c_name(name.data(), name.size());
    return imagecache.getattribute(c_name, val);
}

bool
imagecache_getattribute_string(ImageCache& imagecache, rust::Str name,
                               std::string& val)
{
    std::string_view c_name(name.data(), name.size());
    return imagecache.getattribute(c_name, val);
}

TypeDesc
imagecache_getattributetype(ImageCache& imagecache, rust::Str name)
{
    std::string_view c_name(name.data(), name.size());
    return imagecache.getattributetype(c_name);
}

Perthread*
imagecache_get_perthread_info(ImageCache& imagecache, Perthread* thread_info)
{
    return imagecache.get_perthread_info(thread_info);
}

Perthread*
imagecache_create_thread_info(ImageCache& imagecache)
{
    return imagecache.create_thread_info();
}

void
imagecache_destroy_thread_info(ImageCache& imagecache, Perthread* thread_info)
{
    imagecache.destroy_thread_info(thread_info);
}

ImageHandle*
imagecache_get_image_handle(ImageCache& imagecache, rust::Str filename,
                            Perthread* thread_info, const TextureOpt* options)
{
    OIIO::ustring c_filename(filename.data(), filename.size());
    return imagecache.get_image_handle(c_filename, thread_info, options);
}

bool
imagecache_good(ImageCache& imagecache, ImageHandle* file)
{
    return imagecache.good(file);
}

rust::String
imagecache_filename_from_handle(ImageCache& imagecache, ImageHandle* handle)
{
    return rust::String::lossy(imagecache.filename_from_handle(handle).c_str());
}

bool
imagecache_get_image_info(ImageCache& imagecache, rust::Str filename,
                          int subimage, int miplevel, rust::Str dataname,
                          TypeDesc datatype, const uint8_t* data)
{
    OIIO::ustring c_filename(filename.data(), filename.size());
    OIIO::ustring c_dataname(dataname.data(), dataname.size());
    return imagecache.get_image_info(c_filename, subimage, miplevel, c_dataname,
                                     datatype, (void*)data);
}

bool
imagecache_get_image_info_with_handle(ImageCache& imagecache, ImageHandle* file,
                                      Perthread* thread_info, int subimage,
                                      int miplevel, rust::Str dataname,
                                      TypeDesc datatype, const uint8_t* data)
{
    OIIO::ustring c_dataname(dataname.data(), dataname.size());
    return imagecache.get_image_info(file, thread_info, subimage, miplevel,
                                     c_dataname, datatype, (void*)data);
}

bool
imagecache_get_imagespec(ImageCache& imagecache, rust::Str filename,
                         ImageSpec& spec, int subimage, int miplevel,
                         bool native)
{
    OIIO::ustring c_filename(filename.data(), filename.size());
    return imagecache.get_imagespec(c_filename, spec, subimage, miplevel,
                                    native);
}

std::unique_ptr<ImageSpec>
imagecache_get_imagespec_copy(ImageCache& imagecache, rust::Str filename,
                              int subimage)
{
    auto spec = std::make_unique<ImageSpec>();
    OIIO::ustring c_filename(filename.data(), filename.size());
    if (!imagecache.get_imagespec(c_filename, *spec, subimage))
        return {};
    return spec;
}

std::unique_ptr<ImageSpec>
imagecache_get_imagespec_copy_with_error(ImageCache& imagecache,
                                         rust::Str filename, int subimage,
                                         rust::String& error)
{
    error = rust::String();
    auto spec = imagecache_get_imagespec_copy(imagecache, filename, subimage);
    if (!spec)
        error = take_cache_error(imagecache);
    return spec;
}

std::unique_ptr<ImageSpec>
imagecache_get_cache_dimensions_copy(ImageCache& imagecache,
                                     rust::Str filename, int subimage,
                                     int miplevel)
{
    OIIO::ustring c_filename(filename.data(), filename.size());
    ImageSpec native_spec;
    if (!imagecache.get_imagespec(c_filename, native_spec, subimage))
        return {};

    // get_cache_dimensions only overwrites the compact ImageDims prefix.
    // Seed the object so the untouched format and semantic fields stay valid.
    auto spec = std::make_unique<ImageSpec>(native_spec);
    if (!imagecache.get_cache_dimensions(c_filename, *spec, subimage,
                                         miplevel))
        return {};
    return spec;
}

std::unique_ptr<ImageSpec>
imagecache_get_image_spec_at_copy_with_error(ImageCache& imagecache,
                                             rust::Str filename, int subimage,
                                             int miplevel,
                                             rust::String& error)
{
    error = rust::String();
    ImageSpec native_spec;
    OIIO::ustring c_filename(filename.data(), filename.size());
    if (!imagecache.get_imagespec(c_filename, native_spec, subimage)) {
        error = take_cache_error(imagecache);
        return {};
    }

    // Start with the native spec because get_cache_dimensions overwrites
    // only the compact, mip-varying ImageDims prefix. This retains the
    // native format, channel names, deep flag, and semantic channel indices.
    auto cache_spec = std::make_unique<ImageSpec>(native_spec);
    if (!imagecache.get_cache_dimensions(c_filename, *cache_spec, subimage,
                                         miplevel)) {
        error = take_cache_error(imagecache);
        return {};
    }
    return cache_spec;
}

bool
imagecache_get_imagespec_with_handle(ImageCache& imagecache, ImageHandle* file,
                                     Perthread* thread_info, ImageSpec& spec,
                                     int subimage, int miplevel, bool native)
{
    return imagecache.get_imagespec(file, thread_info, spec, subimage, miplevel,
                                    native);
}

const ImageSpec*
imagecache_imagespec(ImageCache& imagecache, rust::Str filename, int subimage,
                     int miplevel, bool native)
{
    OIIO::ustring c_filename(filename.data(), filename.size());
    return imagecache.imagespec(c_filename, subimage, miplevel, native);
}

const ImageSpec*
imagecache_imagespec_with_handle(ImageCache& imagecache, ImageHandle* file,
                                 Perthread* thread_info, int subimage,
                                 int miplevel, bool native)
{
    return imagecache.imagespec(file, thread_info, subimage, miplevel, native);
}

bool
imagecache_get_thumbnail(ImageCache& imagecache, rust::Str filename,
                         ImageBuf& thumbnail, int subimage)
{
    OIIO::ustring c_filename(filename.data(), filename.size());
    return imagecache.get_thumbnail(c_filename, thumbnail, subimage);
}

bool
imagecache_get_thumbnail_with_handle(ImageCache& imagecache, ImageHandle* file,
                                     Perthread* thread_info,
                                     ImageBuf& thumbnail, int subimage)
{
    return imagecache.get_thumbnail(file, thread_info, thumbnail, subimage);
}

bool
imagecache_get_pixels(ImageCache& imagecache, rust::Str filename, int subimage,
                      int miplevel, int xbegin, int xend, int ybegin, int yend,
                      int zbegin, int zend, int chbegin, int chend,
                      TypeDesc format, const uint8_t* result, int64_t xstride,
                      int64_t ystride, int64_t zstride, int cache_chbegin,
                      int cache_chend)
{
    OIIO::ustring c_filename(filename.data(), filename.size());
    return imagecache.get_pixels(c_filename, subimage, miplevel, xbegin, xend,
                                 ybegin, yend, zbegin, zend, chbegin, chend,
                                 format, (void*)result, xstride, ystride,
                                 zstride, cache_chbegin, cache_chend);
}

bool
imagecache_get_pixels_span(ImageCache& imagecache, rust::Str filename,
                           int subimage, int miplevel, const ROI& roi,
                           TypeDesc format, rust::Slice<uint8_t> result)
{
    const int64_t width = static_cast<int64_t>(roi.xend) - roi.xbegin;
    const int64_t height = static_cast<int64_t>(roi.yend) - roi.ybegin;
    const int64_t depth = static_cast<int64_t>(roi.zend) - roi.zbegin;
    const int64_t channels = static_cast<int64_t>(roi.chend) - roi.chbegin;

    detail::PixelLayout layout;
    if (!detail::bounded_pixel_layout(channels, width, height, depth, format,
                                      result.size(), layout))
        return false;

    OIIO::ustring c_filename(filename.data(), filename.size());
    const auto output = detail::writable_byte_span(result, layout);
    return imagecache.get_pixels(c_filename, subimage, miplevel, roi, format,
                                 output);
}

bool
imagecache_get_pixels_span_with_error(
    ImageCache& imagecache, rust::Str filename, int subimage, int miplevel,
    const ROI& roi, TypeDesc format, rust::Slice<uint8_t> result,
    rust::String& error)
{
    error = rust::String();

    const int64_t width = static_cast<int64_t>(roi.xend) - roi.xbegin;
    const int64_t height = static_cast<int64_t>(roi.yend) - roi.ybegin;
    const int64_t depth = static_cast<int64_t>(roi.zend) - roi.zbegin;
    const int64_t channels = static_cast<int64_t>(roi.chend) - roi.chbegin;
    detail::PixelLayout layout;
    if (!detail::bounded_pixel_layout(channels, width, height, depth, format,
                                      result.size(), layout)) {
        error = rust::String::lossy(
            "invalid pixel layout or destination buffer byte length");
        return false;
    }

    if (imagecache_get_pixels_span(imagecache, filename, subimage, miplevel,
                                   roi, format, result))
        return true;
    error = take_cache_error(imagecache);
    return false;
}

int
imagecache_handle_is_deep(ImageCache& imagecache, ImageHandle* file,
                          Perthread* thread_info, int subimage)
{
    if (file == nullptr)
        return -1;
    if (thread_info != nullptr)
        thread_info = imagecache.get_perthread_info(thread_info);
    ImageSpec spec;
    if (!imagecache.get_imagespec(file, thread_info, spec, subimage)) {
        // Drain, so the failure is not left pending on the cache for an
        // unrelated call to pick up.
        (void)imagecache.geterror(true);
        return -1;
    }
    return spec.deep ? 1 : 0;
}

bool
imagecache_get_pixels_handle_span_with_error(
    ImageCache& imagecache, ImageHandle* file, Perthread* thread_info,
    int subimage, int miplevel, const ROI& roi, TypeDesc format,
    rust::Slice<uint8_t> result, rust::String& error)
{
    error = rust::String();
    if (file == nullptr) {
        error = rust::String::lossy("null image handle");
        return false;
    }

    // OpenImageIO's header is explicit that a caller-managed Perthread must be
    // passed back through get_perthread_info before each use, because that is
    // where the cache does its housekeeping: an invalidate sets a purge flag on
    // the record, and nothing acts on it until this call. Skipping it leaves
    // the two-tile microcache holding pre-invalidation tiles, which is stale
    // data, and becomes a heap over-read if the replacement file is tiled more
    // coarsely, since the old buffer gets indexed with the new dimensions.
    if (thread_info != nullptr)
        thread_info = imagecache.get_perthread_info(thread_info);

    // The by-name read validates against the spec before crossing over; the
    // handle read used to skip it entirely, so a channel range outside the
    // image reached ImageCacheTile::data, which returns NULL out of range and
    // is dereferenced without a check. Validate here rather than in Rust: the
    // handle is already resolved, so this costs no second name lookup.
    ImageSpec spec;
    if (!imagecache.get_imagespec(file, thread_info, spec, subimage)) {
        error = take_cache_error(imagecache);
        if (error.empty())
            error = rust::String::lossy(
                "the image cache could not describe the image");
        return false;
    }
    if (spec.deep) {
        error = rust::String::lossy(
            "the image cache cannot read flat pixels from a deep image");
        return false;
    }
    if (roi.chbegin < 0 || roi.chend > spec.nchannels) {
        error = rust::String::lossy(
            OIIO::Strutil::fmt::format("channel range {}..{} extends outside "
                                       "the image's {} channels",
                                       roi.chbegin, roi.chend,
                                       spec.nchannels));
        return false;
    }

    const int64_t width    = static_cast<int64_t>(roi.xend) - roi.xbegin;
    const int64_t height   = static_cast<int64_t>(roi.yend) - roi.ybegin;
    const int64_t depth    = static_cast<int64_t>(roi.zend) - roi.zbegin;
    const int64_t channels = static_cast<int64_t>(roi.chend) - roi.chbegin;

    detail::PixelLayout layout;
    if (!detail::bounded_pixel_layout(channels, width, height, depth, format,
                                      result.size(), layout)) {
        error = rust::String::lossy(
            "invalid pixel layout or destination buffer byte length");
        return false;
    }

    const auto output = detail::writable_byte_span(result, layout);
    if (imagecache.get_pixels(file, thread_info, subimage, miplevel, roi,
                              format, output))
        return true;
    error = take_cache_error(imagecache);
    return false;
}

bool
imagecache_get_pixels_with_handle(ImageCache& imagecache, ImageHandle* file,
                                  Perthread* thread_info, int subimage,
                                  int miplevel, int xbegin, int xend,
                                  int ybegin, int yend, int zbegin, int zend,
                                  int chbegin, int chend, TypeDesc format,
                                  const uint8_t* result, int64_t xstride,
                                  int64_t ystride, int64_t zstride,
                                  int cache_chbegin, int cache_chend)
{
    return imagecache.get_pixels(file, thread_info, subimage, miplevel, xbegin,
                                 xend, ybegin, yend, zbegin, zend, chbegin,
                                 chend, format, (void*)result, xstride, ystride,
                                 zstride, cache_chbegin, cache_chend);
}

bool
imagecache_get_pixels_all_channels(ImageCache& imagecache, rust::Str filename,
                                   int subimage, int miplevel, int xbegin,
                                   int xend, int ybegin, int yend, int zbegin,
                                   int zend, TypeDesc format,
                                   const uint8_t* result)
{
    OIIO::ustring c_filename(filename.data(), filename.size());
    return imagecache.get_pixels(c_filename, subimage, miplevel, xbegin, xend,
                                 ybegin, yend, zbegin, zend, format,
                                 (void*)result);
}

bool
imagecache_get_pixels_all_channels_with_handle(
    ImageCache& imagecache, ImageHandle* file, Perthread* thread_info,
    int subimage, int miplevel, int xbegin, int xend, int ybegin, int yend,
    int zbegin, int zend, TypeDesc format, const uint8_t* result)
{
    return imagecache.get_pixels(file, thread_info, subimage, miplevel, xbegin,
                                 xend, ybegin, yend, zbegin, zend, format,
                                 (void*)result);
}

void
imagecache_invalidate(ImageCache& imagecache, rust::Str filename, bool force)
{
    OIIO::ustring c_filename(filename.data(), filename.size());
    imagecache.invalidate(c_filename, force);
}

void
imagecache_invalidate_with_handle(ImageCache& imagecache, ImageHandle* file,
                                  bool force)
{
    imagecache.invalidate(file, force);
}

void
imagecache_invalidate_all(ImageCache& imagecache, bool force)
{
    imagecache.invalidate_all(force);
}

void
imagecache_close(ImageCache& imagecache, rust::Str filename)
{
    OIIO::ustring c_filename(filename.data(), filename.size());
    imagecache.close(c_filename);
}

void
imagecache_close_all(ImageCache& imagecache)
{
    imagecache.close_all();
}


Tile*
imagecache_get_tile(ImageCache& imagecache, rust::Str filename, int subimage,
                    int miplevel, int x, int y, int z, int chbegin, int chend)
{
    OIIO::ustring c_filename(filename.data(), filename.size());
    return imagecache.get_tile(c_filename, subimage, miplevel, x, y, z, chbegin,
                               chend);
}

Tile*
imagecache_get_tile_with_handle(ImageCache& imagecache, ImageHandle* file,
                                Perthread* thread_info, int subimage,
                                int miplevel, int x, int y, int z, int chbegin,
                                int chend)
{
    return imagecache.get_tile(file, thread_info, subimage, miplevel, x, y, z,
                               chbegin, chend);
}

void
imagecache_release_tile(ImageCache& imagecache, Tile* tile)
{
    imagecache.release_tile(tile);
}

TypeDesc
imagecache_tile_format(ImageCache& imagecache, const Tile* tile)
{
    return imagecache.tile_format(tile);
}

ROI
imagecache_tile_roi(ImageCache& imagecache, const Tile* tile)
{
    return imagecache.tile_roi(tile);
}

const uint8_t*
imagecache_tile_pixels(ImageCache& imagecache, Tile* tile, TypeDesc& format)
{
    return (uint8_t*)(imagecache.tile_pixels(tile, format));
}

bool
imagecache_add_file(ImageCache& imagecache, rust::Str filename,
                    OIIO::ImageInput::Creator creator, const ImageSpec* config,
                    bool replace)
{
    OIIO::ustring c_filename(filename.data(), filename.size());
    return imagecache.add_file(c_filename, creator, config, replace);
}

bool
imagecache_add_tile_from_coordinates(ImageCache& imagecache, rust::Str filename,
                                     int subimage, int miplevel, int x, int y,
                                     int z, int chbegin, int chend,
                                     TypeDesc format, const uint8_t* buffer,
                                     int64_t xstride, int64_t ystride,
                                     int64_t zstride, bool copy)
{
    OIIO::ustring c_filename(filename.data(), filename.size());
    return imagecache.add_tile(c_filename, subimage, miplevel, x, y, z, chbegin,
                               chend, format, (void*)buffer, xstride, ystride,
                               zstride, copy);
}

bool
imagecache_has_error(ImageCache& imagecache)
{
    return imagecache.has_error();
}

rust::String
imagecache_geterror(ImageCache& imagecache, bool clear)
{
    return rust::String::lossy(imagecache.geterror(clear).c_str());
}

rust::String
imagecache_getstats(ImageCache& imagecache, int level)
{
    return rust::String::lossy(imagecache.getstats(level).c_str());
}

void
imagecache_reset_stats(ImageCache& imagecache)
{
    imagecache.reset_stats();
}
}  // namespace oiio
