// Reproduction for: ImageOutput::write_scanlines() taking an image_span
// mishandles a non-zero data window origin.
//
// With a data window at y=5 and height 4:
//   - rows 5..9 (the data window in image coordinates) are rejected with
//     "Invalid scanline range 5-9", while the pointer overload accepts them;
//   - rows 0..4 are accepted, but the file that results differs from the one
//     the pointer overload writes from the same buffer.
//
// Uses only public OpenImageIO API. Exits non-zero if the overloads disagree.
//
// Build (MSVC):
//   cl /std:c++17 /EHsc /utf-8 /MD span_scanline_origin_repro.cpp
//      /I <oiio>/include /I <deps>/include
//      /link /LIBPATH:<oiio>/lib OpenImageIO.lib OpenImageIO_Util.lib
//
// Build (gcc/clang):
//   c++ -std=c++17 span_scanline_origin_repro.cpp -lOpenImageIO -lOpenImageIO_Util

#include <OpenImageIO/image_span.h>
#include <OpenImageIO/imageio.h>

#include <cstddef>
#include <cstdio>
#include <string>
#include <vector>

using namespace OIIO;

namespace {

constexpr int NCHANNELS = 3;
constexpr int WIDTH     = 6;
constexpr int HEIGHT    = 4;
constexpr int ORIGIN_Y  = 5;

image_span<std::byte>
byte_span(std::vector<float>& buffer)
{
    const stride_t chansize = sizeof(float);
    return image_span<std::byte>(reinterpret_cast<std::byte*>(buffer.data()),
                                 uint32_t(NCHANNELS), uint32_t(WIDTH),
                                 uint32_t(HEIGHT), 1, chansize,
                                 chansize * NCHANNELS,
                                 chansize * NCHANNELS * WIDTH,
                                 chansize * NCHANNELS * WIDTH * HEIGHT,
                                 uint32_t(chansize));
}

ImageSpec offset_spec()
{
    ImageSpec spec(WIDTH, HEIGHT, NCHANNELS, TypeDesc::FLOAT);
    spec.y = ORIGIN_Y;
    return spec;
}

bool
read_back(const char* path, std::vector<float>& pixels, int& origin)
{
    auto in = ImageInput::open(path);
    if (!in)
        return false;
    origin = in->spec().y;
    pixels.resize(std::size_t(WIDTH) * HEIGHT * NCHANNELS);
    return in->read_image(0, 0, 0, NCHANNELS, TypeDesc::FLOAT, pixels.data());
}

}  // namespace

int
main()
{
    std::printf("OpenImageIO %s\n\n", OIIO_VERSION_STRING);
    std::printf("data window origin y=%d, height %d\n\n", ORIGIN_Y, HEIGHT);

    std::vector<float> pixels(std::size_t(WIDTH) * HEIGHT * NCHANNELS);
    for (std::size_t i = 0; i < pixels.size(); ++i)
        pixels[i] = float(i);

    // 1. The span overload, rows in image coordinates.
    bool absolute_ok = false;
    std::string absolute_error;
    {
        auto out    = ImageOutput::create("scanline_origin_absolute.exr");
        ImageSpec s = offset_spec();
        if (out && out->open("scanline_origin_absolute.exr", s)) {
            auto data   = byte_span(pixels);
            absolute_ok = out->write_scanlines(ORIGIN_Y, ORIGIN_Y + HEIGHT,
                                               TypeDesc::FLOAT, data);
            absolute_error = out->geterror();
            out->close();
        }
    }

    // 2. The span overload, rows relative to the data window.
    bool relative_ok = false;
    std::string relative_error;
    {
        auto out    = ImageOutput::create("scanline_origin_relative.exr");
        ImageSpec s = offset_spec();
        if (out && out->open("scanline_origin_relative.exr", s)) {
            auto data   = byte_span(pixels);
            relative_ok = out->write_scanlines(0, HEIGHT, TypeDesc::FLOAT, data);
            relative_error = out->geterror();
            out->close();
        }
    }

    // 3. The pointer overload, rows in image coordinates.
    bool pointer_ok = false;
    std::string pointer_error;
    {
        auto out    = ImageOutput::create("scanline_origin_pointer.exr");
        ImageSpec s = offset_spec();
        if (out && out->open("scanline_origin_pointer.exr", s)) {
            pointer_ok    = out->write_scanlines(ORIGIN_Y, ORIGIN_Y + HEIGHT, 0,
                                                 TypeDesc::FLOAT, pixels.data());
            pointer_error = out->geterror();
            out->close();
        }
    }

    std::printf("  write_scanlines(image_span), rows %d..%d : %s%s%s\n", ORIGIN_Y,
                ORIGIN_Y + HEIGHT, absolute_ok ? "ok" : "FAILED",
                absolute_error.empty() ? "" : ", error: ",
                absolute_error.c_str());
    std::printf("  write_scanlines(image_span), rows 0..%d  : %s%s%s\n", HEIGHT,
                relative_ok ? "ok" : "FAILED",
                relative_error.empty() ? "" : ", error: ",
                relative_error.c_str());
    std::printf("  write_scanlines(pointer),   rows %d..%d : %s%s%s\n", ORIGIN_Y,
                ORIGIN_Y + HEIGHT, pointer_ok ? "ok" : "FAILED",
                pointer_error.empty() ? "" : ", error: ", pointer_error.c_str());

    int failures = 0;
    if (absolute_ok != pointer_ok) {
        std::printf("\n  >>> MISMATCH: the two overloads disagree about the "
                    "same range\n");
        ++failures;
    }

    // Did the accepted 0-based call write the same image as the pointer call?
    if (relative_ok && pointer_ok) {
        std::vector<float> from_relative, from_pointer;
        int relative_origin = 0, pointer_origin = 0;
        const bool a = read_back("scanline_origin_relative.exr", from_relative,
                                 relative_origin);
        const bool b = read_back("scanline_origin_pointer.exr", from_pointer,
                                 pointer_origin);
        if (a && b) {
            const bool same_pixels = (from_relative == from_pointer);
            const bool same_origin = (relative_origin == pointer_origin);
            std::printf("\n  read back: origins %d and %d (%s), pixels %s\n",
                        pointer_origin, relative_origin,
                        same_origin ? "same" : "DIFFER",
                        same_pixels ? "identical" : "DIFFER");
            if (!same_pixels) {
                std::printf("  >>> the accepted 0-based call wrote different "
                            "data, so this is not simply a different\n"
                            "      coordinate convention\n");
                ++failures;
            }
        }
    }

    std::printf("\n%d mismatch(es)\n", failures);
    return failures ? 1 : 0;
}
