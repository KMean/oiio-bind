// Reproduction for: ImageInput::read_image() taking an image_span mishandles
// every tiled image — the image_span overload of read_tiles forwards to the
// pointer overload with the x and y ranges swapped. This program compares
// only return values (silently false for some geometries); even the cases
// that print ok return wrong or partly uninitialised data. The pointer
// overload reads the same files correctly.
//
// Uses only public OpenImageIO API. Exits non-zero if the overloads disagree.
//
// Build (MSVC):
//   cl /std:c++17 /EHsc /utf-8 /MD span_tiled_read_repro.cpp
//      /I <oiio>/include /I <deps>/include
//      /link /LIBPATH:<oiio>/lib OpenImageIO.lib OpenImageIO_Util.lib
//
// Build (gcc/clang):
//   c++ -std=c++17 span_tiled_read_repro.cpp -lOpenImageIO -lOpenImageIO_Util

#include <OpenImageIO/image_span.h>
#include <OpenImageIO/imageio.h>

#include <cstddef>
#include <cstdio>
#include <string>
#include <vector>

using namespace OIIO;

namespace {

constexpr int NCHANNELS = 3;
int failures            = 0;

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
    std::vector<float> pixels(std::size_t(width) * height * NCHANNELS);
    for (std::size_t i = 0; i < pixels.size(); ++i)
        pixels[i] = float(i);
    if (!out->write_image(TypeDesc::FLOAT, pixels.data())) {
        std::printf("  write_image failed: %s\n", out->geterror().c_str());
        return false;
    }
    return out->close();
}

// image_span<std::byte> over a contiguous, exactly sized buffer, with every
// stride spelled out.
bool
read_explicit_strides(const std::string& path, int width, int height,
                      std::string& error)
{
    auto in = ImageInput::open(path);
    if (!in) {
        error = "could not open the file";
        return false;
    }
    std::vector<float> buffer(std::size_t(width) * height * NCHANNELS);
    const stride_t chansize = sizeof(float);
    const image_span<std::byte> data(reinterpret_cast<std::byte*>(buffer.data()),
                                     uint32_t(NCHANNELS), uint32_t(width),
                                     uint32_t(height), 1, chansize,
                                     chansize * NCHANNELS,
                                     chansize * NCHANNELS * width,
                                     chansize * NCHANNELS * width * height,
                                     uint32_t(chansize));
    const bool ok = in->read_image(0, 0, 0, NCHANNELS, TypeDesc::FLOAT, data);
    error         = in->geterror();
    return ok;
}

// The simplest spelling: a typed image_span with OpenImageIO computing every
// stride itself, so the result cannot depend on the caller's arithmetic.
bool
read_default_strides(const std::string& path, int width, int height,
                     std::string& error)
{
    auto in = ImageInput::open(path);
    if (!in) {
        error = "could not open the file";
        return false;
    }
    std::vector<float> buffer(std::size_t(width) * height * NCHANNELS);
    const image_span<float> data(buffer.data(), uint32_t(NCHANNELS),
                                 uint32_t(width), uint32_t(height));
    const bool ok = in->read_image(0, 0, 0, NCHANNELS, data);
    error         = in->geterror();
    return ok;
}

bool
read_pointer(const std::string& path, int width, int height, std::string& error)
{
    auto in = ImageInput::open(path);
    if (!in) {
        error = "could not open the file";
        return false;
    }
    std::vector<float> buffer(std::size_t(width) * height * NCHANNELS);
    const bool ok = in->read_image(0, 0, 0, NCHANNELS, TypeDesc::FLOAT,
                                   buffer.data());
    error         = in->geterror();
    return ok;
}

void
tiled_case(int width, int height, int tile)
{
    const bool partial     = (width % tile) || (height % tile);
    const std::string path = "span_tiled_" + std::to_string(width) + "x"
                             + std::to_string(height) + "_t"
                             + std::to_string(tile) + ".exr";

    std::printf("%dx%d, %dx%d tiles (%s)\n", width, height, tile, tile,
                partial ? "PARTIAL edge tiles" : "exact multiple");
    if (!write_tiled(path, width, height, tile)) {
        std::printf("  could not write the fixture\n");
        ++failures;
        return;
    }

    std::string explicit_error, default_error, pointer_error;
    const bool explicit_ok = read_explicit_strides(path, width, height,
                                                   explicit_error);
    const bool default_ok  = read_default_strides(path, width, height,
                                                  default_error);
    const bool pointer_ok  = read_pointer(path, width, height, pointer_error);

    std::printf("  read_image(image_span, explicit strides) : %s%s%s\n",
                explicit_ok ? "ok" : "FAILED",
                explicit_error.empty() ? "" : ", error: ",
                explicit_error.c_str());
    std::printf("  read_image(image_span, default strides)  : %s%s%s\n",
                default_ok ? "ok" : "FAILED",
                default_error.empty() ? "" : ", error: ", default_error.c_str());
    std::printf("  read_image(pointer)                      : %s%s%s\n",
                pointer_ok ? "ok" : "FAILED",
                pointer_error.empty() ? "" : ", error: ",
                pointer_error.c_str());

    if (explicit_ok != pointer_ok || default_ok != pointer_ok) {
        std::printf("  >>> MISMATCH: the overloads disagree on the same file\n");
        ++failures;
    }
    std::printf("\n");
}

}  // namespace

int
main()
{
    std::printf("OpenImageIO %s\n\n", OIIO_VERSION_STRING);

    tiled_case(32, 32, 16);  // exact multiple
    tiled_case(16, 16, 16);  // a single tile
    tiled_case(40, 32, 16);  // partial in x
    tiled_case(32, 24, 16);  // partial in y
    tiled_case(40, 24, 16);  // partial in both

    std::printf("%d mismatch(es)\n", failures);
    return failures ? 1 : 0;
}
