#include "ffi_imageio.h"
#include "ffi_pixel.h"
#include "oiio-sys/src/imageio.rs.h"
#include <OpenImageIO/imageio.h>
#include <OpenImageIO/string_view.h>
#include <cstddef>
#include <limits>
#include <memory>
#include <stdexcept>
#include <stdio.h>

namespace oiio {
#pragma region ROI
ROI
roi_default() noexcept
{
    return ROI();
}

ROI
roi_new(int xbegin, int xend, int ybegin, int yend, int zbegin, int zend,
        int chbegin, int chend) noexcept
{
    return OIIO::ROI(xbegin, xend, ybegin, yend, zbegin, zend, chbegin, chend);
}

ROI
roi_new_all() noexcept
{
    return OIIO::ROI::All();
}

bool
roi_defined(const ROI& roi) noexcept
{
    return roi.defined();
}

int
roi_width(const ROI& roi) noexcept
{
    return roi.width();
}

int
roi_height(const ROI& roi) noexcept
{
    return roi.height();
}

int
roi_depth(const ROI& roi) noexcept
{
    return roi.depth();
}

int
roi_nchannels(const ROI& roi) noexcept
{
    return roi.nchannels();
}

uint64_t
roi_npixels(const ROI& roi) noexcept
{
    return roi.npixels();
}

bool
roi_eq_roi(const ROI& roi, const ROI& other) noexcept
{
    return roi == other;
}

bool
roi_ne_roi(const ROI& roi, const ROI& other) noexcept
{
    return roi != other;
}

bool
roi_contains(const ROI& roi, int x, int y, int z, int ch) noexcept
{
    return roi.contains(x, y, z, ch);
}

bool
roi_contains_roi(const ROI& roi, const ROI& other) noexcept
{
    return roi.contains(other);
}

ROI
roi_union(const ROI& roi, const ROI& other) noexcept
{
    return OIIO::roi_union(roi, other);
}

ROI
roi_intersection(const ROI& roi, const ROI& other) noexcept
{
    return OIIO::roi_intersection(roi, other);
}
#pragma endregion

#pragma region ImageSpec
std::unique_ptr<ImageSpec>
imagespec_from_resolution(int xres, int yres, int nchans)
{
    OIIO::ImageSpec* spec = new OIIO::ImageSpec(xres, yres, nchans);
    return std::unique_ptr<ImageSpec>(spec);
}

int
imagespec_x(const ImageSpec& spec)
{
    return spec.x;
}

int
imagespec_y(const ImageSpec& spec)
{
    return spec.y;
}

int
imagespec_z(const ImageSpec& spec)
{
    return spec.z;
}

int
imagespec_width(const ImageSpec& spec)
{
    return spec.width;
}

int
imagespec_height(const ImageSpec& spec)
{
    return spec.height;
}

int
imagespec_depth(const ImageSpec& spec)
{
    return spec.depth;
}

int
imagespec_full_x(const ImageSpec& spec)
{
    return spec.full_x;
}

int
imagespec_full_y(const ImageSpec& spec)
{
    return spec.full_y;
}

int
imagespec_full_z(const ImageSpec& spec)
{
    return spec.full_z;
}

int
imagespec_full_width(const ImageSpec& spec)
{
    return spec.full_width;
}

int
imagespec_full_height(const ImageSpec& spec)
{
    return spec.full_height;
}

int
imagespec_full_depth(const ImageSpec& spec)
{
    return spec.full_depth;
}

int
imagespec_tile_width(const ImageSpec& spec)
{
    return spec.tile_width;
}

int
imagespec_tile_height(const ImageSpec& spec)
{
    return spec.tile_height;
}

int
imagespec_tile_depth(const ImageSpec& spec)
{
    return spec.tile_depth;
}

int
imagespec_nchannels(const ImageSpec& spec)
{
    return spec.nchannels;
}

int
imagespec_alpha_channel(const ImageSpec& spec)
{
    return spec.alpha_channel;
}

int
imagespec_z_channel(const ImageSpec& spec)
{
    return spec.z_channel;
}

bool
imagespec_deep(const ImageSpec& spec)
{
    return spec.deep;
}

bool
imagespec_valid(const ImageSpec& spec)
{
    return spec.format != OIIO::TypeDesc::UNKNOWN;
}

std::unique_ptr<std::vector<std::string>>
imagespec_channel_names(const ImageSpec& spec)
{
    std::vector<std::string>* channel_names = new std::vector<std::string>(
        spec.channelnames);
    return std::unique_ptr<std::vector<std::string>>(channel_names);
}

namespace {
inline OIIO::string_view
to_string_view(const rust::Str text) noexcept
{
    return OIIO::string_view(text.data(), text.size());
}
}  // namespace

std::unique_ptr<ImageSpec>
imagespec_new(int xres, int yres, int nchans, TypeDesc format)
{
    return std::make_unique<ImageSpec>(xres, yres, nchans, format);
}

std::unique_ptr<ImageSpec>
imagespec_copy(const ImageSpec& spec)
{
    return std::make_unique<ImageSpec>(spec);
}

TypeDesc
imagespec_format(const ImageSpec& spec)
{
    return spec.format;
}

void
imagespec_set_format(ImageSpec& spec, TypeDesc format)
{
    spec.set_format(format);
}

void
imagespec_set_origin(ImageSpec& spec, int x, int y, int z)
{
    spec.x = x;
    spec.y = y;
    spec.z = z;
}

void
imagespec_set_dimensions(ImageSpec& spec, int width, int height, int depth)
{
    spec.width  = width;
    spec.height = height;
    spec.depth  = depth;
}

void
imagespec_set_full(ImageSpec& spec, int full_x, int full_y, int full_z,
                   int full_width, int full_height, int full_depth)
{
    spec.full_x      = full_x;
    spec.full_y      = full_y;
    spec.full_z      = full_z;
    spec.full_width  = full_width;
    spec.full_height = full_height;
    spec.full_depth  = full_depth;
}

void
imagespec_set_tile_size(ImageSpec& spec, int width, int height, int depth)
{
    spec.tile_width  = width;
    spec.tile_height = height;
    spec.tile_depth  = depth;
}

void
imagespec_set_channel_names(ImageSpec& spec,
                            const rust::Vec<rust::String>& names)
{
    spec.channelnames.clear();
    spec.channelnames.reserve(names.size());
    for (const rust::String& name : names)
        spec.channelnames.emplace_back(name.data(), name.size());
}

void
imagespec_set_alpha_channel(ImageSpec& spec, int index)
{
    spec.alpha_channel = index;
}

void
imagespec_set_z_channel(ImageSpec& spec, int index)
{
    spec.z_channel = index;
}

void
imagespec_set_deep(ImageSpec& spec, bool deep)
{
    spec.deep = deep;
}

void
imagespec_attribute_int(ImageSpec& spec, const rust::Str name, int value)
{
    spec.attribute(to_string_view(name), value);
}

void
imagespec_attribute_float(ImageSpec& spec, const rust::Str name, float value)
{
    spec.attribute(to_string_view(name), value);
}

void
imagespec_attribute_string(ImageSpec& spec, const rust::Str name,
                           const rust::Str value)
{
    spec.attribute(to_string_view(name), to_string_view(value));
}

bool
imagespec_erase_attribute(ImageSpec& spec, const rust::Str name)
{
    const std::size_t before = spec.extra_attribs.size();
    spec.erase_attribute(std::string(name.data(), name.size()));
    return spec.extra_attribs.size() != before;
}

bool
imagespec_has_attribute(const ImageSpec& spec, const rust::Str name)
{
    return spec.find_attribute(to_string_view(name)) != nullptr;
}

TypeDesc
imagespec_attribute_type(const ImageSpec& spec, const rust::Str name)
{
    return spec.getattributetype(to_string_view(name));
}

int
imagespec_get_int_attribute(const ImageSpec& spec, const rust::Str name,
                            int defaultval)
{
    return spec.get_int_attribute(to_string_view(name), defaultval);
}

float
imagespec_get_float_attribute(const ImageSpec& spec, const rust::Str name,
                              float defaultval)
{
    return spec.get_float_attribute(to_string_view(name), defaultval);
}

rust::String
imagespec_get_string_attribute(const ImageSpec& spec, const rust::Str name,
                               const rust::Str defaultval)
{
    const OIIO::string_view value
        = spec.get_string_attribute(to_string_view(name),
                                    to_string_view(defaultval));
    // Metadata is arbitrary file content, so never assume it is valid UTF-8.
    return rust::String::lossy(value.data(), value.size());
}

rust::String
imagespec_attribute_to_string(const ImageSpec& spec, const rust::Str name)
{
    const OIIO::ParamValue* attribute = spec.find_attribute(
        to_string_view(name));
    if (attribute == nullptr)
        return rust::String();
    const std::string value = attribute->get_string();
    return rust::String::lossy(value.data(), value.size());
}

rust::Vec<uint8_t>
imagespec_attribute_bytes(const ImageSpec& spec, const rust::Str name)
{
    rust::Vec<uint8_t> bytes;
    const OIIO::ParamValue* attribute = spec.find_attribute(
        to_string_view(name));
    if (attribute == nullptr)
        return bytes;

    const int size = attribute->datasize();
    if (size <= 0)
        return bytes;
    const auto* data = static_cast<const uint8_t*>(attribute->data());
    if (data == nullptr)
        return bytes;

    bytes.reserve(std::size_t(size));
    for (int i = 0; i < size; ++i)
        bytes.push_back(data[i]);
    return bytes;
}

bool
attribute_bytes_are_writable(const rust::Str type_name, size_t length)
{
    const OIIO::TypeDesc type(std::string(type_name.data(), type_name.size()));
    if (type == OIIO::TypeUnknown)
        return false;
    // An array whose length is not concrete defeats every check below.
    // TypeDesc::fromstring presets arraylen to -1, so "float[]" parses to -1
    // and "uint8[-3]" to -3, while TypeDesc::size() clamps the count to at
    // least one -- so "float[]" measures four bytes and a four byte payload
    // would sail through. The clamp is not applied on the way back out: both
    // sprint_type and format_type size their loop as `arraylen ? arraylen : 1`
    // with the raw value, and size_t(-1) walks off the end of the stored value.
    if (type.arraylen < 0)
        return false;
    // The value must be exactly the size the type describes, or OpenImageIO
    // would read past what was handed to it.
    if (type.size() != length)
        return false;
    if (type.basetype == OIIO::TypeDesc::STRING)
        return false;
    // A pointer or a hashed string carries a raw process address, which is
    // meaningless in another process and would be written into the file.
    if (type.basetype == OIIO::TypeDesc::PTR
        || type.basetype == OIIO::TypeDesc::USTRINGHASH)
        return false;
    return true;
}

bool
imagespec_attribute_set_bytes(ImageSpec& spec, const rust::Str name,
                              const rust::Str type_name,
                              rust::Slice<const uint8_t> bytes)
{
    if (!attribute_bytes_are_writable(type_name, bytes.size()))
        return false;
    const OIIO::TypeDesc type(std::string(type_name.data(), type_name.size()));
    if (type == OIIO::TypeUnknown)
        return false;
    spec.attribute(to_string_view(name), type, bytes.data());
    return true;
}

rust::Vec<rust::String>
imagespec_attribute_strings(const ImageSpec& spec, const rust::Str name)
{
    rust::Vec<rust::String> values;
    const OIIO::ParamValue* attribute = spec.find_attribute(
        to_string_view(name));
    if (attribute == nullptr || attribute->type().basetype != OIIO::TypeDesc::STRING)
        return values;

    const int count = attribute->type().basevalues();
    const auto* strings = static_cast<const OIIO::ustring*>(attribute->data());
    if (strings == nullptr)
        return values;
    for (int i = 0; i < count; ++i) {
        const OIIO::string_view text = strings[i];
        values.push_back(rust::String::lossy(text.data(), text.size()));
    }
    return values;
}

bool
imagespec_attribute_set_strings(ImageSpec& spec, const rust::Str name,
                                const rust::Str type_name,
                                const rust::Vec<rust::String>& values)
{
    const OIIO::TypeDesc type(std::string(type_name.data(), type_name.size()));
    if (type == OIIO::TypeUnknown || type.basetype != OIIO::TypeDesc::STRING)
        return false;
    if (std::size_t(type.basevalues()) != values.size())
        return false;

    // ustrings live for the life of the process, so the pointers handed to
    // OpenImageIO stay valid after this vector goes away.
    std::vector<OIIO::ustring> interned;
    interned.reserve(values.size());
    for (const rust::String& value : values)
        interned.emplace_back(value.data(), value.size());

    spec.attribute(to_string_view(name), type, interned.data());
    return true;
}

std::unique_ptr<std::vector<ImageSpec>>
imagespec_vector_new()
{
    return std::make_unique<std::vector<ImageSpec>>();
}

void
imagespec_vector_push(std::vector<ImageSpec>& specs, const ImageSpec& spec)
{
    specs.push_back(spec);
}

rust::Vec<rust::String>
imagespec_attribute_names(const ImageSpec& spec)
{
    rust::Vec<rust::String> names;
    for (const OIIO::ParamValue& attribute : spec.extra_attribs) {
        const OIIO::string_view name = attribute.name();
        names.push_back(rust::String::lossy(name.data(), name.size()));
    }
    return names;
}
#pragma endregion

#pragma region ImageInput
std::unique_ptr<ImageInput>
imageinput_open_with_config(const rust::Str filename, const ImageSpec& config)
{
    std::unique_ptr<OIIO::ImageInput> image_input(
        OIIO::ImageInput::open(std::string(filename), &config));

    if (image_input) {
        return image_input;
    } else {
        throw std::runtime_error(OIIO::geterror());
    }
}

std::unique_ptr<ImageInput>
imageinput_open_without_config(const rust::Str filename)
{
    std::unique_ptr<OIIO::ImageInput> image_input(
        OIIO::ImageInput::open(std::string(filename), nullptr));

    return image_input;
}

std::unique_ptr<ImageInput>
imageinput_create_with_config(const rust::Str filename, bool do_open,
                              const ImageSpec& config,
                              const rust::Str plugin_searchpath)
{
    OIIO::string_view c_plugin_searchpath(plugin_searchpath.data(),
                                          plugin_searchpath.size());
    std::unique_ptr<OIIO::ImageInput> image_input(
        OIIO::ImageInput::create(std::string(filename), do_open, &config,
                                  nullptr, c_plugin_searchpath));

    if (image_input) {
        return image_input;
    } else {
        throw std::runtime_error(OIIO::geterror());
    }
}

std::unique_ptr<ImageInput>
imageinput_create_without_config(const rust::Str filename, bool do_open,
                                 const rust::Str plugin_searchpath)
{
    OIIO::string_view c_plugin_searchpath(plugin_searchpath.data(),
                                          plugin_searchpath.size());
    std::unique_ptr<OIIO::ImageInput> image_input(
        OIIO::ImageInput::create(std::string(filename), do_open, nullptr,
                                  nullptr, c_plugin_searchpath));

    if (image_input) {
        return image_input;
    } else {
        throw std::runtime_error(OIIO::geterror());
    }
}

rust::Str
imageinput_format_name(const ImageInput& imageinput)
{
    return rust::Str(imageinput.format_name());
}

bool
imageinput_supports(const ImageInput& imageinput, const rust::Str feature)
{
    return imageinput.supports(
        OIIO::string_view(feature.data(), feature.size()));
}

bool
imageinput_valid_file(const ImageInput& imageinput, const rust::Str filename)
{
    return imageinput.valid_file(std::string(filename));
}

const ImageSpec&
imageinput_spec(const OIIO::ImageInput& imageinput)
{
    return imageinput.spec();
}

std::unique_ptr<ImageSpec>
imageinput_spec_subimage_miplevel(OIIO::ImageInput& imageinput,
                                  int32_t subimage, int32_t miplevel)
{
    return std::make_unique<ImageSpec>(imageinput.spec(subimage, miplevel));
}

std::unique_ptr<ImageSpec>
imageinput_spec_dimensions(OIIO::ImageInput& imageinput, int32_t subimage,
                           int32_t miplevel)
{
    return std::make_unique<ImageSpec>(
        imageinput.spec_dimensions(subimage, miplevel));
}

std::unique_ptr<ImageInput>
imageinput_open_with_ioproxy(const rust::Str filename, IOProxy* ioproxy)
{
    return OIIO::ImageInput::open(std::string(filename), nullptr, ioproxy);
}

bool
imageinput_close(ImageInput& imageinput)
{
    return imageinput.close();
}

int
imageinput_current_subimage(const ImageInput& imageinput)
{
    return imageinput.current_subimage();
}

int
imageinput_current_miplevel(const ImageInput& imageinput)
{
    return imageinput.current_miplevel();
}

bool
imageinput_seek_subimage(ImageInput& imageinput, int subimage, int miplevel)
{
    return imageinput.seek_subimage(subimage, miplevel);
}

bool
imageinput_read_scanline(ImageInput& imageinput, int y, int z, TypeDesc format,
                         rust::Slice<uint8_t> data, int64_t xstride)
{
    return imageinput.read_scanline(y, z, format, data.data(), xstride);
}

bool
imageinput_read_scanlines(ImageInput& imageinput, int subimage, int miplevel,
                          int ybegin, int yend, int z, int chbegin, int chend,
                          TypeDesc format, rust::Slice<uint8_t> data,
                          int64_t xstride, int64_t ystride)
{
    return imageinput.read_scanlines(subimage, miplevel, ybegin, yend, z,
                                     chbegin, chend, format, data.data(),
                                     xstride, ystride);
}

bool
imageinput_read_image(ImageInput& imageinput, int subimage, int miplevel,
                      int chbegin, int chend, TypeDesc format,
                      rust::Slice<uint8_t> data, int64_t xstride,
                      int64_t ystride, int64_t zstride)
{
    return imageinput.read_image(subimage, miplevel, chbegin, chend, format,
                                 data.data(), xstride, ystride, zstride);
}

bool
imageinput_read_image_span(ImageInput& imageinput, int subimage, int miplevel,
                           int chbegin, int chend, TypeDesc format,
                           rust::Slice<uint8_t> data)
{
    const ImageSpec spec = imageinput.spec_dimensions(subimage, miplevel);
    if (chend < 0 || chend > spec.nchannels)
        chend = spec.nchannels;

    detail::PixelLayout layout;
    if (!imagespec_valid(spec) || chbegin < 0 || chbegin >= chend
        || !detail::bounded_pixel_layout(
            static_cast<int64_t>(chend) - chbegin, spec.width, spec.height,
            spec.depth, format, data.size(), layout)) {
        imageinput.errorfmt("invalid dimensions or destination buffer for bounded image read");
        return false;
    }

    // The layout above is the safety contract: the buffer holds exactly one
    // contiguous value per channel, pixel, row, and slice, and its byte size
    // was checked against those dimensions.
    //
    // The strides are then passed explicitly instead of through OpenImageIO's
    // `image_span` overload. Measured against OpenImageIO 3.1.12, that
    // overload returns false without recording an error when a tiled image's
    // width or height is not an exact multiple of the tile size, which no
    // exactly sized destination buffer can satisfy. The explicit-stride
    // overload reads the same images correctly.
    return imageinput.read_image(subimage, miplevel, chbegin, chend, format,
                                 data.data(), layout.x_stride, layout.y_stride,
                                 layout.z_stride);
}

namespace {
// Shared front half of the bounded partial reads: resolve the level's
// dimensions and normalise the channel range.
inline bool
bounded_read_setup(ImageInput& imageinput, int subimage, int miplevel,
                   int chbegin, int& chend, ImageSpec& spec)
{
    spec = imageinput.spec_dimensions(subimage, miplevel);
    if (chend < 0 || chend > spec.nchannels)
        chend = spec.nchannels;
    return imagespec_valid(spec) && chbegin >= 0 && chbegin < chend;
}
}  // namespace

bool
imageinput_read_scanlines_span(ImageInput& imageinput, int subimage,
                               int miplevel, int ybegin, int yend, int z,
                               int chbegin, int chend, TypeDesc format,
                               rust::Slice<uint8_t> data)
{
    ImageSpec spec;
    detail::PixelLayout layout;
    if (!bounded_read_setup(imageinput, subimage, miplevel, chbegin, chend,
                            spec)
        || ybegin >= yend || ybegin < spec.y
        || yend > static_cast<int64_t>(spec.y) + spec.height || z < spec.z
        || z >= static_cast<int64_t>(spec.z) + spec.depth
        || !detail::bounded_pixel_layout(static_cast<int64_t>(chend) - chbegin,
                                         spec.width,
                                         static_cast<int64_t>(yend) - ybegin, 1,
                                         format, data.size(), layout)) {
        imageinput.errorfmt(
            "invalid scanline range or destination buffer for bounded read");
        return false;
    }

    return imageinput.read_scanlines(subimage, miplevel, ybegin, yend, z,
                                     chbegin, chend, format, data.data(),
                                     layout.x_stride, layout.y_stride);
}

bool
imageinput_read_tiles_span(ImageInput& imageinput, int subimage, int miplevel,
                           int xbegin, int xend, int ybegin, int yend,
                           int zbegin, int zend, int chbegin, int chend,
                           TypeDesc format, rust::Slice<uint8_t> data)
{
    ImageSpec spec;
    detail::PixelLayout layout;
    if (!bounded_read_setup(imageinput, subimage, miplevel, chbegin, chend,
                            spec)
        || spec.tile_width <= 0 || xbegin >= xend || ybegin >= yend
        || zbegin >= zend || xbegin < spec.x
        || xend > static_cast<int64_t>(spec.x) + spec.width || ybegin < spec.y
        || yend > static_cast<int64_t>(spec.y) + spec.height || zbegin < spec.z
        || zend > static_cast<int64_t>(spec.z) + spec.depth
        || !detail::bounded_pixel_layout(static_cast<int64_t>(chend) - chbegin,
                                         static_cast<int64_t>(xend) - xbegin,
                                         static_cast<int64_t>(yend) - ybegin,
                                         static_cast<int64_t>(zend) - zbegin,
                                         format, data.size(), layout)) {
        imageinput.errorfmt(
            "invalid tile range or destination buffer for bounded read");
        return false;
    }

    return imageinput.read_tiles(subimage, miplevel, xbegin, xend, ybegin, yend,
                                 zbegin, zend, chbegin, chend, format,
                                 data.data(), layout.x_stride, layout.y_stride,
                                 layout.z_stride);
}

bool
imageinput_read_native_deep_scanlines(ImageInput& imageinput, int subimage,
                                      int miplevel, int ybegin, int yend, int z,
                                      int chbegin, int chend, DeepData& data)
{
    return imageinput.read_native_deep_scanlines(subimage, miplevel, ybegin,
                                                 yend, z, chbegin, chend, data);
}

bool
imageinput_read_native_deep_tiles(ImageInput& imageinput, int subimage,
                                  int miplevel, int xbegin, int xend,
                                  int ybegin, int yend, int zbegin, int zend,
                                  int chbegin, int chend, DeepData& data)
{
    return imageinput.read_native_deep_tiles(subimage, miplevel, xbegin, xend,
                                             ybegin, yend, zbegin, zend,
                                             chbegin, chend, data);
}

bool
imageinput_read_native_deep_image(ImageInput& imageinput, int subimage,
                                  int miplevel, DeepData& data)
{
    return imageinput.read_native_deep_image(subimage, miplevel, data);
}

bool
imageinput_read_native_scanline(ImageInput& imageinput, int subimage,
                                int miplevel, int y, int z,
                                rust::Slice<uint8_t> data)
{
    return imageinput.read_native_scanline(subimage, miplevel, y, z,
                                           data.data());
}

bool
imageinput_read_native_scanlines(ImageInput& imageinput, int subimage,
                                 int miplevel, int ybegin, int yend, int z,
                                 int chbegin, int chend,
                                 rust::Slice<uint8_t> data)
{
    return imageinput.read_native_scanlines(subimage, miplevel, ybegin, yend, z,
                                            chbegin, chend, data.data());
}

bool
imageinput_read_native_tile(ImageInput& imageinput, int subimage, int miplevel,
                            int x, int y, int z, rust::Slice<uint8_t> data)
{
    return imageinput.read_native_tile(subimage, miplevel, x, y, z,
                                       data.data());
}

bool
imageinput_read_native_tiles(ImageInput& imageinput, int xbegin, int xend,
                             int ybegin, int yend, int zbegin, int zend,
                             int chbegin, int chend, rust::Slice<uint8_t> data)
{
    const int subimage = imageinput.current_subimage();
    const int miplevel = imageinput.current_miplevel();
    const ImageSpec dimensions
        = imageinput.spec_dimensions(subimage, miplevel);
    OIIO::span<std::byte> c_data(reinterpret_cast<std::byte*>(data.data()),
                                 data.size());

    if (dimensions.depth > 1) {
        return imageinput.read_native_volumetric_tiles(
            subimage, miplevel, xbegin, xend, ybegin, yend, zbegin, zend,
            chbegin, chend, c_data);
    }

    if (zbegin != dimensions.z || zend != dimensions.z + 1) {
        return false;
    }

    return imageinput.read_native_tiles(subimage, miplevel, xbegin, xend,
                                        ybegin, yend, chbegin, chend, c_data);
}

bool
imageinput_set_ioproxy(ImageInput& imageinput, IOProxy* ioproxy)
{
    return imageinput.set_ioproxy(ioproxy);
}

bool
imageinput_has_error(const ImageInput& imageinput)
{
    return imageinput.has_error();
}

rust::String
imageinput_geterror(ImageInput& imageinput)
{
// OpenImageIO builds its error text from the file: a damaged EXR whose
// attribute name is arbitrary bytes comes back quoted verbatim. cxx's
// throwing rust::String constructor would turn that into std::terminate,
// because the shim it is called from is noexcept. Never assume UTF-8.
    return rust::String::lossy(imageinput.geterror());
}

void
imageinput_seterror(ImageInput& imageinput, const rust::Str message)
{
    // Two things are wrong with passing message.data() here. A rust::Str is a
    // pointer and a length over a Rust &str and carries no NUL, so errorfmt
    // scans past the end of any string that is a prefix of a larger one. And
    // errorfmt with no arguments is still a runtime format call, so a "{}"
    // anywhere in the caller's text throws fmt::format_error out of a shim
    // that cxx declares noexcept, which is std::terminate. Passing the text as
    // an argument to a fixed format string settles both.
    imageinput.errorfmt("{}", to_string_view(message));
}

void
imageinput_set_threads(ImageInput& imageinput, int n)
{
    return imageinput.threads(n);
}

int
imageinput_threads(const ImageInput& imageinput)
{
    return imageinput.threads();
}
#pragma endregion

#pragma region ImageOutput
std::unique_ptr<ImageOutput>
imageoutput_create(const rust::Str filename, IOProxy* ioproxy,
                   const rust::Str plugin_searchpath)
{
    OIIO::string_view c_filename(filename.data(), filename.size());
    OIIO::string_view c_plugin_searchpath(plugin_searchpath.data(),
                                          plugin_searchpath.size());
    return ImageOutput::create(c_filename, ioproxy, c_plugin_searchpath);
}

rust::Str
imageoutput_format_name(const ImageOutput& imageoutput)
{
    return imageoutput.format_name();
}

int
imageoutput_supports(const ImageOutput& imageoutput, const rust::Str feature)
{
    return imageoutput.supports(
        OIIO::string_view(feature.data(), feature.size()));
}

bool
imageoutput_open(ImageOutput& imageoutput, const rust::Str filename,
                 const ImageSpec& newspec, OpenMode mode)
{
    return imageoutput.open(std::string(filename), newspec, mode);
}

bool
imageoutput_open_multi_subimage(ImageOutput& imageoutput,
                                const rust::Str filename, int subimages,
                                const ImageSpec* specs)
{
    return imageoutput.open(std::string(filename), subimages, specs);
}

bool
imageoutput_open_specs(ImageOutput& imageoutput, const rust::Str filename,
                       const std::vector<ImageSpec>& specs)
{
    if (specs.empty()
        || specs.size() > static_cast<std::size_t>(
               std::numeric_limits<int>::max())) {
        imageoutput.errorfmt("invalid subimage count for a multi-part write");
        return false;
    }
    return imageoutput.open(std::string(filename),
                            static_cast<int>(specs.size()), specs.data());
}

const ImageSpec&
imageoutput_spec(const ImageOutput& imageoutput)
{
    return imageoutput.spec();
}

bool
imageoutput_close(ImageOutput& imageoutput)
{
    return imageoutput.close();
}

bool
imageoutput_write_scanline(ImageOutput& imageoutput, int y, int z,
                           TypeDesc format, const rust::Slice<uint8_t> data,
                           int64_t xstride)
{
    return imageoutput.write_scanline(y, z, format, data.data(), xstride);
}

bool
imageoutput_write_scanlines(ImageOutput& imageoutput, int ybegin, int yend,
                            int z, TypeDesc format,
                            const rust::Slice<uint8_t> data, int64_t xstride,
                            int64_t ystride)
{
    return imageoutput.write_scanlines(ybegin, yend, z, format, data.data(),
                                       xstride, ystride);
}

bool
imageoutput_write_tile(ImageOutput& imageoutput, int x, int y, int z,
                       TypeDesc format, const rust::Slice<uint8_t> data,
                       int64_t xstride, int64_t ystride, int64_t zstride)
{
    return imageoutput.write_tile(x, y, z, format, data.data(), xstride,
                                  ystride, zstride);
}

bool
imageoutput_write_tiles(ImageOutput& imageoutput, int xbegin, int xend,
                        int ybegin, int yend, int zbegin, int zend,
                        TypeDesc format, const rust::Slice<uint8_t> data,
                        int64_t xstride, int64_t ystride, int64_t zstride)
{
    return imageoutput.write_tiles(xbegin, xend, ybegin, yend, zbegin, zend,
                                   format, data.data(), xstride, ystride,
                                   zstride);
}

bool
imageoutput_write_rectangle(ImageOutput& imageoutput, int xbegin, int xend,
                            int ybegin, int yend, int zbegin, int zend,
                            TypeDesc format, const rust::Slice<uint8_t> data,
                            int64_t xstride, int64_t ystride, int64_t zstride)
{
    return imageoutput.write_rectangle(xbegin, xend, ybegin, yend, zbegin, zend,
                                       format, data.data(), xstride, ystride,
                                       zstride);
}

bool
imageoutput_write_image(ImageOutput& imageoutput, TypeDesc format,
                        const rust::Slice<uint8_t> data, int64_t xstride,
                        int64_t ystride, int64_t zstride)
{
    return imageoutput.write_image(format, data.data(), xstride, ystride,
                                   zstride);
}

bool
imageoutput_write_image_span(ImageOutput& imageoutput, TypeDesc format,
                             const rust::Slice<const uint8_t> data)
{
    const ImageSpec& spec = imageoutput.spec();

    detail::PixelLayout layout;
    if (!imagespec_valid(spec)
        || !detail::bounded_pixel_layout(spec.nchannels, spec.width,
                                         spec.height, spec.depth, format,
                                         data.size(), layout)) {
        imageoutput.errorfmt(
            "invalid dimensions or source buffer for bounded image write");
        return false;
    }

    // Explicit strides rather than OpenImageIO's `image_span` overload; see
    // the note in imageinput_read_image_span.
    return imageoutput.write_image(format, data.data(), layout.x_stride,
                                   layout.y_stride, layout.z_stride);
}

bool
imageoutput_write_scanlines_span(ImageOutput& imageoutput, int ybegin, int yend,
                                 TypeDesc format,
                                 const rust::Slice<const uint8_t> data)
{
    const ImageSpec& spec = imageoutput.spec();

    detail::PixelLayout layout;
    if (!imagespec_valid(spec) || ybegin < spec.y
        || yend > static_cast<int64_t>(spec.y) + spec.height || ybegin >= yend
        || !detail::bounded_pixel_layout(spec.nchannels, spec.width,
                                         static_cast<int64_t>(yend) - ybegin, 1,
                                         format, data.size(), layout)) {
        imageoutput.errorfmt(
            "invalid scanline range or source buffer for bounded write");
        return false;
    }

    // Measured against OpenImageIO 3.1.12, the `image_span` overload of
    // write_scanlines rejects ranges expressed in image coordinates when the
    // data window origin is non-zero ("Invalid scanline range"), so the
    // explicit-stride overload is used here as well.
    return imageoutput.write_scanlines(ybegin, yend, spec.z, format,
                                       data.data(), layout.x_stride,
                                       layout.y_stride);
}

bool
imageoutput_write_tiles_span(ImageOutput& imageoutput, int xbegin, int xend,
                             int ybegin, int yend, int zbegin, int zend,
                             TypeDesc format,
                             const rust::Slice<const uint8_t> data)
{
    const ImageSpec& spec = imageoutput.spec();

    detail::PixelLayout layout;
    if (!imagespec_valid(spec) || spec.tile_width <= 0 || xbegin >= xend
        || ybegin >= yend || zbegin >= zend || xbegin < spec.x
        || xend > static_cast<int64_t>(spec.x) + spec.width || ybegin < spec.y
        || yend > static_cast<int64_t>(spec.y) + spec.height || zbegin < spec.z
        || zend > static_cast<int64_t>(spec.z) + spec.depth
        || !detail::bounded_pixel_layout(spec.nchannels,
                                         static_cast<int64_t>(xend) - xbegin,
                                         static_cast<int64_t>(yend) - ybegin,
                                         static_cast<int64_t>(zend) - zbegin,
                                         format, data.size(), layout)) {
        imageoutput.errorfmt(
            "invalid tile range or source buffer for bounded write");
        return false;
    }

    return imageoutput.write_tiles(xbegin, xend, ybegin, yend, zbegin, zend,
                                   format, data.data(), layout.x_stride,
                                   layout.y_stride, layout.z_stride);
}

bool
imageoutput_write_deep_scanlines(ImageOutput& imageoutput, int ybegin, int yend,
                                 int z, const DeepData& deepdata)
{
    return imageoutput.write_deep_scanlines(ybegin, yend, z, deepdata);
}

bool
imageoutput_write_deep_tiles(ImageOutput& imageoutput, int xbegin, int xend,
                             int ybegin, int yend, int zbegin, int zend,
                             const DeepData& deepdata)
{
    return imageoutput.write_deep_tiles(xbegin, xend, ybegin, yend, zbegin,
                                        zend, deepdata);
}

bool
imageoutput_write_deep_image(ImageOutput& imageoutput, const DeepData& deepdata)
{
    return imageoutput.write_deep_image(deepdata);
}

bool
imageoutput_set_thumbnail(ImageOutput& imageoutput, const ImageBuf& thumb)
{
    return imageoutput.set_thumbnail(thumb);
}

bool
imageoutput_copy_image(ImageOutput& imageoutput, ImageInput* imageinput)
{
    return imageoutput.copy_image(imageinput);
}

bool
imageoutput_set_ioproxy(ImageOutput& imageoutput, IOProxy* ioproxy)
{
    return imageoutput.set_ioproxy(ioproxy);
}

bool
imageoutput_has_error(const ImageOutput& imageoutput)
{
    return imageoutput.has_error();
}

rust::String
imageoutput_geterror(const ImageOutput& imageoutput, bool clear)
{
// OpenImageIO builds its error text from the file: a damaged EXR whose
// attribute name is arbitrary bytes comes back quoted verbatim. cxx's
// throwing rust::String constructor would turn that into std::terminate,
// because the shim it is called from is noexcept. Never assume UTF-8.
    return rust::String::lossy(imageoutput.geterror(clear));
}

void
imageoutput_seterror(ImageOutput& imageoutput, const rust::Str message)
{
    // See imageinput_seterror.
    imageoutput.errorfmt("{}", to_string_view(message));
}

void
imageoutput_set_threads(ImageOutput& imageoutput, int n)
{
    imageoutput.threads(n);
}

int
imageoutput_threads(const ImageOutput& imageoutput)
{
    return imageoutput.threads();
}
#pragma endregion

#pragma region Utility Functions
void
shutdown()
{
    OIIO::shutdown();
}

int
openimageio_version()
{
    return OIIO::openimageio_version();
}

int
openimageio_build_version()
{
    return OIIO_VERSION;
}

bool
has_error()
{
    return OIIO::has_error();
}

rust::String
get_error(bool clear)
{
    // Returning the std::string directly would convert through cxx's throwing
    // constructor, and this shim is noexcept, so a message OpenImageIO built
    // out of bytes taken from the file would be std::terminate rather than an
    // error. Every constructor called from these shims must be the lossy one.
    const std::string error = OIIO::geterror(clear);
    return rust::String::lossy(error.data(), error.size());
}

bool
attribute(const rust::Str name, TypeDesc type, rust::Slice<uint8_t> value)
{
    return OIIO::attribute(std::string_view(name.data(), name.length()), type,
                           value.data());
}

bool
attribute_float(const rust::Str name, float value)
{
    return OIIO::attribute(std::string_view(name.data(), name.length()), value);
}

bool
attribute_int(const rust::Str name, const int value)
{
    return OIIO::attribute(std::string_view(name.data(), name.length()), value);
}

bool
getattribute(const rust::Str name, const TypeDesc type,
             rust::Slice<uint8_t> value)
{
    return OIIO::getattribute(std::string_view(name.data(), name.length()),
                              type, value.data());
}

bool
getattribute_int(const rust::Str name, int& value)
{
    return OIIO::getattribute(std::string_view(name.data(), name.length()),
                              value);
}

bool
getattribute_float(const rust::Str name, float& value)
{
    {
        return OIIO::getattribute(std::string_view(name.data(), name.length()),
                                  value);
    }
}

bool
getattribute_string(const rust::Str name, rust::String& value)
{
    std::string c_value;
    bool result
        = OIIO::getattribute(std::string_view(name.data(), name.length()),
                             c_value);
    value = rust::String::lossy(c_value);
    return result;
}

int
get_int_attribute(const rust::Str name, int defaultval)
{
    return OIIO::get_int_attribute(std::string_view(name.data(), name.length()),
                                   defaultval);
}

float
get_float_attribute(const rust::Str name, float defaultval)
{
    return OIIO::get_float_attribute(std::string_view(name.data(),
                                                      name.length()),
                                     defaultval);
}

rust::String
get_string_attribute(const rust::Str name, const rust::Str defaultval)
{
    std::string c_defaultval(defaultval.data(), defaultval.length());
    std::string c_value = OIIO::get_string_attribute(
        std::string_view(name.data(), name.length()), c_defaultval);
    return rust::String::lossy(c_value);
}

// void
// declare_imageio_format(const rust::Str name,
//                        rust::Fn<ImageInput*(ImageInput*)> input_creator,
//                        const rust::Slice<const rust::Str> input_extensions,
//                        rust::Fn<ImageOutput*(ImageOutput*)> output_creator,
//                        const rust::Slice<const rust::Str> output_extensions,
//                        const rust::Str lib_version)
// {
//     auto c_input_creator = [&](OIIO::ImageInput*) -> OIIO::ImageInput* {
//         return input_creator();
//     };

//     std::vector<const char*> c_input_extensions;
//     c_input_extensions.reserve(input_extensions.size() + 1);
//     std::vector<const char*> c_output_extensions;
//     c_output_extensions.reserve(output_extensions.size() + 1);
//     std::string c_name(name.data(), name.length());
//     std::string c_lib_version(lib_version.data(), lib_version.length());

//     for (auto& ext : input_extensions) {
//         c_input_extensions.push_back(ext.data());
//     }
//     c_input_extensions.push_back(nullptr);

//     for (auto& ext : output_extensions) {
//         c_output_extensions.push_back(ext.data());
//     }
//     c_output_extensions.push_back(nullptr);

//     OIIO::declare_imageio_format(c_name, c_input_creator,
//                                  c_input_extensions.data(),
//                                  (ImageOutput::Creator)(&output_creator),
//                                  c_output_extensions.data(),
//                                  c_lib_version.data());
// }

bool
is_imageio_format_name(const rust::Str name)
{
    return OIIO::is_imageio_format_name(std::string(name));
}

rust::Vec<ExtensionMapItem>
get_extension_map()
{
    auto map = OIIO::get_extension_map();
    rust::Vec<ExtensionMapItem> result;

    for (auto& item : map) {
        ExtensionMapItem i {};
        rust::Vec<rust::String> values {};

        for (auto& value : item.second) {
            values.push_back(rust::String::lossy(value));
        }

        i.key   = rust::String::lossy(item.first);
        i.value = values;
        result.push_back(i);
    }

    return result;
}


bool
convert_pixel_values(TypeDesc src_type, rust::Slice<const uint8_t> src,
                     TypeDesc dst_type, rust::Slice<uint8_t> dst)
{
    return OIIO::convert_pixel_values(src_type, src.data(), dst_type,
                                      dst.data());
}

bool
convert_image(int nchannels, int width, int height, int depth,
              rust::Slice<const uint8_t> src, TypeDesc src_type,
              int64_t src_xstride, int64_t src_ystride, int64_t src_zstride,
              rust::Slice<uint8_t> dst, TypeDesc dst_type, int64_t dst_xstride,
              int64_t dst_ystride, int64_t dst_zstride)
{
    return OIIO::convert_image(nchannels, width, height, depth, src.data(),
                               src_type, src_xstride, src_ystride, src_zstride,
                               dst.data(), dst_type, dst_xstride, dst_ystride,
                               dst_zstride);
}

bool
parallel_convert_image(int nchannels, int width, int height, int depth,
                       rust::Slice<const uint8_t> src, TypeDesc src_type,
                       int64_t src_xstride, int64_t src_ystride,
                       int64_t src_zstride, rust::Slice<uint8_t> dst,
                       TypeDesc dst_type, int64_t dst_xstride,
                       int64_t dst_ystride, int64_t dst_zstride, int nthreads)
{
    return OIIO::parallel_convert_image(nchannels, width, height, depth,
                                        src.data(), src_type, src_xstride,
                                        src_ystride, src_zstride, dst.data(),
                                        dst_type, dst_xstride, dst_ystride,
                                        dst_zstride, nthreads);
}

void
add_dither(int nchannels, int width, int height, int depth, float* data,
           int64_t xstride, int64_t ystride, int64_t zstride,
           float ditheramplitude, int alpha_channel, int z_channel,
           unsigned int ditherseed, int chorigin, int xorigin, int yorigin,
           int zorigin)
{
    OIIO::add_dither(nchannels, width, height, depth, data, xstride, ystride,
                     zstride, ditheramplitude, alpha_channel, z_channel,
                     ditherseed, chorigin, xorigin, yorigin, zorigin);
}

void
premult(int nchannels, int width, int height, int depth, int chbegin, int chend,
        TypeDesc datatype, rust::Slice<uint8_t> data, int64_t xstride,
        int64_t ystride, int64_t zstride, int alpha_channel, int z_channel)
{
    OIIO::premult(nchannels, width, height, depth, chbegin, chend, datatype,
                  data.data(), xstride, ystride, zstride, alpha_channel,
                  z_channel);
}

bool
copy_image(int nchannels, int width, int height, int depth,
           rust::Slice<const uint8_t> src, int64_t pixelsize,
           int64_t src_xstride, int64_t src_ystride, int64_t src_zstride,
           rust::Slice<uint8_t> dst, int64_t dst_xstride, int64_t dst_ystride,
           int64_t dst_zstride)
{
    return OIIO::copy_image(nchannels, width, height, depth, src.data(),
                            pixelsize, src_xstride, src_ystride, src_zstride,
                            dst.data(), dst_xstride, dst_ystride, dst_zstride);
}

bool
wrap_black(int& coord, int origin, int width)
{
    return OIIO::wrap_black(coord, origin, width);
}

bool
wrap_clamp(int& coord, int origin, int width)
{
    return OIIO::wrap_clamp(coord, origin, width);
}

bool
wrap_periodic(int& coord, int origin, int width)
{
    return OIIO::wrap_periodic(coord, origin, width);
}

bool
wrap_periodic_pow2(int& coord, int origin, int width)
{
    return OIIO::wrap_periodic_pow2(coord, origin, width);
}

bool
wrap_mirror(int& coord, int origin, int width)
{
    return OIIO::wrap_mirror(coord, origin, width);
}

void
debug(const rust::Str message)
{
    // See imageinput_seterror: OIIO::debug takes a string_view but the
    // const char* conversion would scan for a NUL that is not there.
    OIIO::debug(to_string_view(message));
}
#pragma endregion
}  // namespace oiio
