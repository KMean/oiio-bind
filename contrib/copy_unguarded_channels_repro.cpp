// Reproducer: ImageBufAlgo::copy() writes destination channels the source
// does not have, taking the values from an unchecked read past the source's
// pixel.
//
// ImageBufAlgo::paste(), a hundred lines above copy_() in the same file,
// guards the identical loop with `c < src_nchans ? s[c] : D(0)`. copy_()
// does not. This program runs both over the same shapes so the asymmetry is
// visible side by side.
//
// Build (Windows, MSVC), adjusting the OIIO prefix:
//
//   cl /std:c++17 /EHsc /utf-8 copy_unguarded_channels_repro.cpp ^
//      /I <oiio-prefix>\include ^
//      /link /LIBPATH:<oiio-prefix>\lib OpenImageIO.lib
//
// Build (Linux/macOS):
//
//   c++ -std=c++17 copy_unguarded_channels_repro.cpp -lOpenImageIO -o repro
//
// Exits non-zero if the leftover channels are not copies of the source's
// only channel, i.e. if the bug is present.

#include <OpenImageIO/imagebuf.h>
#include <OpenImageIO/imagebufalgo.h>
#include <OpenImageIO/imageio.h>

#include <cstdio>
#include <string>
#include <vector>

using namespace OIIO;

namespace {

constexpr int WIDTH    = 4;
constexpr int HEIGHT   = 4;
constexpr int DST_CHAN = 5;
constexpr float SENTINEL = -1.0f;

// A one-channel source whose every pixel holds a different, predictable
// value: pixel (x, y) holds y * WIDTH + x, so 0 .. 15 in raster order.
//
// The distinct values are the point. If the source were filled with a single
// constant, an over-read into the next pixel would return that same constant
// and the bug would be invisible.
ImageBuf
make_source()
{
    ImageBuf src(ImageSpec(WIDTH, HEIGHT, 1, TypeDesc::FLOAT));
    for (int y = 0; y < HEIGHT; ++y) {
        for (int x = 0; x < WIDTH; ++x) {
            float v = float(y * WIDTH + x);
            src.setpixel(x, y, &v, 1);
        }
    }
    return src;
}

ImageBuf
make_destination()
{
    ImageBuf dst(ImageSpec(WIDTH, HEIGHT, DST_CHAN, TypeDesc::FLOAT));
    std::vector<float> fill(DST_CHAN, SENTINEL);
    ImageBufAlgo::fill(dst, fill);
    return dst;
}

void
report(const char* what, const ImageBuf& buf)
{
    std::printf("  %s:\n", what);
    for (int y = 0; y < HEIGHT; ++y) {
        for (int x = 0; x < WIDTH; ++x) {
            float px[DST_CHAN] = {};
            buf.getpixel(x, y, px, DST_CHAN);
            std::printf("    (%d,%d) =", x, y);
            for (int c = 0; c < DST_CHAN; ++c)
                std::printf(" %8g", px[c]);
            std::printf("\n");
        }
    }
}

}  // namespace

int
main()
{
    // The headers compiled against, and the library actually linked. A
    // machine with more than one OpenImageIO on it can easily measure the
    // wrong one.
    std::printf("compiled against OIIO %s; linked lib reports \"%s\" (%d)\n\n",
                OIIO_VERSION_STRING,
                std::string(get_string_attribute("version")).c_str(),
                openimageio_version());

    const ImageBuf src = make_source();

    // --- Case 1: copy, 1-channel source into a pre-allocated 5-channel
    // destination, no ROI. ---
    std::printf("Case 1  copy(dst[5ch], src[1ch]), no ROI\n");
    ImageBuf dst = make_destination();
    if (!ImageBufAlgo::copy(dst, src)) {
        std::printf("FATAL: copy failed: %s\n", dst.geterror().c_str());
        return 2;
    }
    std::printf("  copy returned true, geterror: \"%s\"\n",
                dst.geterror().c_str());
    report("result", dst);

    // --- Case 2: paste, the same shapes, for contrast. ---
    std::printf("\nCase 2  paste(dst[5ch], 0,0,0,0, src[1ch])  (control)\n");
    ImageBuf pasted = make_destination();
    if (!ImageBufAlgo::paste(pasted, 0, 0, 0, 0, src)) {
        std::printf("FATAL: paste failed: %s\n", pasted.geterror().c_str());
        return 2;
    }
    report("result", pasted);

    // --- Verdict, read off case 1. ---
    //
    // Channel 0 must be the source. Channels 1..4 are channels the source
    // does not have: whatever copy decides to put there, it cannot be read
    // from the source, so the only defensible values are the sentinel left
    // in place or zero.
    std::printf("\n");
    int bad = 0;
    for (int y = 0; y < HEIGHT; ++y) {
        for (int x = 0; x < WIDTH; ++x) {
            float px[DST_CHAN] = {};
            dst.getpixel(x, y, px, DST_CHAN);

            const float expected0 = float(y * WIDTH + x);
            if (px[0] != expected0) {
                std::printf("channel 0 at (%d,%d) is %g, expected %g\n", x, y,
                            px[0], expected0);
                ++bad;
            }
            for (int c = 1; c < DST_CHAN; ++c) {
                if (px[c] != SENTINEL && px[c] != 0.0f) {
                    std::printf(
                        "(%d,%d) channel %d = %g -- the source has no channel "
                        "%d; this is the value %d element(s) past its pixel\n",
                        x, y, c, px[c], c, c);
                    ++bad;
                }
            }
        }
    }

    if (bad) {
        std::printf(
            "\nFAIL: %d destination value(s) came from past the source's "
            "pixel.\n", bad);
        return 1;
    }
    std::printf("\nPASS: channels the source does not have were left alone.\n");
    return 0;
}
