// Standalone reproduction of two image_span behaviours in OpenImageIO.
//
// 1. ImageInput::read_image() taking an image_span fails, without recording an
//    error, for a tiled image whose width or height is not an exact multiple
//    of the tile size. The pointer overload reads the same file correctly.
//
// 2. ImageOutput::write_scanlines() taking an image_span rejects a scanline
//    range expressed in image coordinates when the data window origin is
//    non-zero. The pointer overload accepts the same range.
//
// Build: see compile_repro.bat. Uses only public OpenImageIO API.

#include <OpenImageIO/image_span.h>
#include <OpenImageIO/imageio.h>

#include <cstddef>
#include <cstdio>
#include <string>
#include <vector>

using namespace OIIO;

namespace {

constexpr int NCHANNELS = 3;

std::vector<float>
ramp(std::size_t count)
{
    std::vector<float> values(count);
    for (std::size_t i = 0; i < count; ++i)
        values[i] = float(i);
    return values;
}

// An image_span over a contiguous, exactly sized float buffer.
image_span<std::byte>
byte_span(std::vector<float>& buffer, int nchannels, int width, int height)
{
    const stride_t chansize = sizeof(float);
    return image_span<std::byte>(reinterpret_cast<std::byte*>(buffer.data()),
                                 uint32_t(nchannels), uint32_t(width),
                                 uint32_t(height), 1, chansize,
                                 chansize * nchannels,
                                 chansize * nchannels * width,
                                 chansize * nchannels * width * height,
                                 uint32_t(chansize));
}

bool
write_tiled(const std::string& path, int width, int height, int tile)
{
    auto out = ImageOutput::create(path);
    if (!out) {
        std::printf("  could not create a writer for %s\n", path.c_str());
        return false;
    }
    ImageSpec spec(width, height, NCHANNELS, TypeDesc::FLOAT);
    spec.tile_width  = tile;
    spec.tile_height = tile;
    spec.tile_depth  = 1;
    if (!out->open(path, spec)) {
        std::printf("  open failed: %s\n", out->geterror().c_str());
        return false;
    }
    auto pixels = ramp(std::size_t(width) * height * NCHANNELS);
    if (!out->write_image(TypeDesc::FLOAT, pixels.data())) {
        std::printf("  write_image failed: %s\n", out->geterror().c_str());
        return false;
    }
    return out->close();
}

// Returns true when the span-based read succeeded.
bool
read_with_span(const std::string& path, int width, int height,
               std::string& error)
{
    auto in = ImageInput::open(path);
    if (!in) {
        error = "could not open the file";
        return false;
    }
    auto buffer   = std::vector<float>(std::size_t(width) * height * NCHANNELS);
    auto data     = byte_span(buffer, NCHANNELS, width, height);
    const bool ok = in->read_image(0, 0, 0, NCHANNELS, TypeDesc::FLOAT, data);
    error         = in->geterror();
    return ok;
}

bool
read_with_pointer(const std::string& path, int width, int height,
                  std::string& error)
{
    auto in = ImageInput::open(path);
    if (!in) {
        error = "could not open the file";
        return false;
    }
    auto buffer   = std::vector<float>(std::size_t(width) * height * NCHANNELS);
    const bool ok = in->read_image(0, 0, 0, NCHANNELS, TypeDesc::FLOAT,
                                   buffer.data());
    error         = in->geterror();
    return ok;
}

int failures = 0;

void
tiled_case(int width, int height, int tile)
{
    const bool partial = (width % tile) || (height % tile);
    const std::string path = "span_repro_" + std::to_string(width) + "x"
                             + std::to_string(height) + "_t"
                             + std::to_string(tile) + ".exr";

    std::printf("%dx%d, %dx%d tiles (%s)\n", width, height, tile, tile,
                partial ? "PARTIAL edge tiles" : "exact multiple");
    if (!write_tiled(path, width, height, tile)) {
        std::printf("  could not write the fixture\n");
        ++failures;
        return;
    }

    std::string span_error, pointer_error;
    const bool span_ok    = read_with_span(path, width, height, span_error);
    const bool pointer_ok = read_with_pointer(path, width, height,
                                              pointer_error);

    std::printf("  read_image(image_span) : %s%s%s\n", span_ok ? "ok" : "FAILED",
                span_error.empty() ? "" : ", error: ", span_error.c_str());
    std::printf("  read_image(pointer)    : %s%s%s\n",
                pointer_ok ? "ok" : "FAILED",
                pointer_error.empty() ? "" : ", error: ",
                pointer_error.c_str());

    if (span_ok != pointer_ok) {
        std::printf("  >>> MISMATCH: the two overloads disagree on the same "
                    "file\n");
        ++failures;
    }
    std::printf("\n");
}

void
offset_origin_case()
{
    const int width = 6, height = 4, origin_y = 5;
    const std::string path = "span_repro_offset_origin.exr";
    std::printf("data window origin y=%d, writing scanlines %d..%d\n", origin_y,
                origin_y, origin_y + height);

    auto pixels = ramp(std::size_t(width) * height * NCHANNELS);

    // Span overload.
    bool span_ok = false;
    std::string span_error;
    {
        auto out = ImageOutput::create(path);
        ImageSpec spec(width, height, NCHANNELS, TypeDesc::FLOAT);
        spec.y = origin_y;
        if (out && out->open(path, spec)) {
            auto data = byte_span(pixels, NCHANNELS, width, height);
            span_ok   = out->write_scanlines(origin_y, origin_y + height,
                                             TypeDesc::FLOAT, data);
            span_error = out->geterror();
            out->close();
        }
    }

    // Pointer overload, same specification and same range.
    bool pointer_ok = false;
    std::string pointer_error;
    {
        auto out = ImageOutput::create(path);
        ImageSpec spec(width, height, NCHANNELS, TypeDesc::FLOAT);
        spec.y = origin_y;
        if (out && out->open(path, spec)) {
            pointer_ok = out->write_scanlines(origin_y, origin_y + height, 0,
                                              TypeDesc::FLOAT, pixels.data());
            pointer_error = out->geterror();
            out->close();
        }
    }

    std::printf("  write_scanlines(image_span) : %s%s%s\n",
                span_ok ? "ok" : "FAILED",
                span_error.empty() ? "" : ", error: ", span_error.c_str());
    std::printf("  write_scanlines(pointer)    : %s%s%s\n",
                pointer_ok ? "ok" : "FAILED",
                pointer_error.empty() ? "" : ", error: ",
                pointer_error.c_str());
    if (span_ok != pointer_ok) {
        std::printf("  >>> MISMATCH: the two overloads disagree on the same "
                    "range\n");
        ++failures;
    }
    std::printf("\n");
}

}  // namespace

int
main()
{
    std::printf("OpenImageIO %s (build %s)\n\n", OIIO_VERSION_STRING,
                OIIO_INTRO_STRING);

    std::printf("== 1. tiled reads ==\n\n");
    tiled_case(32, 32, 16);  // exact multiple, expected to work
    tiled_case(16, 16, 16);  // single tile, expected to work
    tiled_case(40, 32, 16);  // partial in x
    tiled_case(32, 24, 16);  // partial in y
    tiled_case(40, 24, 16);  // partial in both

    std::printf("== 2. scanline writes with a non-zero data window origin ==\n\n");
    offset_origin_case();

    std::printf("%d mismatch(es)\n", failures);
    return failures ? 1 : 0;
}
