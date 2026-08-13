#pragma once

#include <OpenImageIO/image_span.h>
#include <OpenImageIO/imageio.h>
#include <OpenImageIO/oiioversion.h>
#include <OpenImageIO/typedesc.h>
#include <rust/cxx.h>
#include <cstddef>
#include <cstdint>
#include <limits>

// Deliberately not written as !OIIO_VERSION_GREATER_EQUAL(3, 1, 4). That macro
// expands to an unparenthesised `OIIO_VERSION >= ...`, so a leading `!` binds
// to OIIO_VERSION alone and the test silently evaluates to `0 >= version`,
// which is always false. Comparing OIIO_VERSION directly cannot misparse.
#if OIIO_VERSION < OIIO_MAKE_VERSION(3, 1, 4)
#    error "oiio-bind's bounded pixel API requires OpenImageIO 3.1.4 or newer"
#endif

namespace oiio::detail {

struct PixelLayout {
    uint32_t channels;
    uint32_t width;
    uint32_t height;
    uint32_t depth;
    uint32_t channel_size;
    OIIO::stride_t channel_stride;
    OIIO::stride_t x_stride;
    OIIO::stride_t y_stride;
    OIIO::stride_t z_stride;
};

inline bool
checked_multiply(std::size_t left, std::size_t right,
                 std::size_t& result) noexcept
{
    if (left != 0 && right > std::numeric_limits<std::size_t>::max() / left)
        return false;
    result = left * right;
    return true;
}

inline bool
bounded_pixel_layout(int64_t channels, int64_t width, int64_t height,
                     int64_t depth, OIIO::TypeDesc format,
                     std::size_t buffer_bytes, PixelLayout& layout) noexcept
{
    if (channels <= 0 || width <= 0 || height <= 0 || depth <= 0)
        return false;
    if (channels > std::numeric_limits<uint32_t>::max()
        || width > std::numeric_limits<uint32_t>::max()
        || height > std::numeric_limits<uint32_t>::max()
        || depth > std::numeric_limits<uint32_t>::max())
        return false;
    if (format.aggregate != OIIO::TypeDesc::SCALAR || format.arraylen != 0)
        return false;

    switch (static_cast<OIIO::TypeDesc::BASETYPE>(format.basetype)) {
    case OIIO::TypeDesc::UINT8:
    case OIIO::TypeDesc::UINT16:
    case OIIO::TypeDesc::HALF:
    case OIIO::TypeDesc::FLOAT: break;
    default: return false;
    }

    const std::size_t channel_size = format.basesize();
    if (channel_size == 0
        || channel_size > std::numeric_limits<uint32_t>::max())
        return false;

    std::size_t x_stride;
    std::size_t y_stride;
    std::size_t z_stride;
    std::size_t required_bytes;
    if (!checked_multiply(static_cast<std::size_t>(channels), channel_size,
                          x_stride)
        || !checked_multiply(static_cast<std::size_t>(width), x_stride,
                             y_stride)
        || !checked_multiply(static_cast<std::size_t>(height), y_stride,
                             z_stride)
        || !checked_multiply(static_cast<std::size_t>(depth), z_stride,
                             required_bytes)
        || required_bytes != buffer_bytes)
        return false;

    constexpr auto stride_max
        = static_cast<std::size_t>(std::numeric_limits<OIIO::stride_t>::max());
    if (x_stride > stride_max || y_stride > stride_max
        || z_stride > stride_max)
        return false;

    layout.channels       = static_cast<uint32_t>(channels);
    layout.width          = static_cast<uint32_t>(width);
    layout.height         = static_cast<uint32_t>(height);
    layout.depth          = static_cast<uint32_t>(depth);
    layout.channel_size   = static_cast<uint32_t>(channel_size);
    layout.channel_stride = static_cast<OIIO::stride_t>(channel_size);
    layout.x_stride       = static_cast<OIIO::stride_t>(x_stride);
    layout.y_stride       = static_cast<OIIO::stride_t>(y_stride);
    layout.z_stride       = static_cast<OIIO::stride_t>(z_stride);
    return true;
}

inline OIIO::image_span<std::byte>
writable_byte_span(rust::Slice<uint8_t> data, const PixelLayout& layout)
{
    return OIIO::image_span<std::byte>(
        reinterpret_cast<std::byte*>(data.data()), layout.channels,
        layout.width, layout.height, layout.depth, layout.channel_stride,
        layout.x_stride, layout.y_stride, layout.z_stride,
        layout.channel_size);
}

}  // namespace oiio::detail
