// Reproducer: colorconvert() and the five operations that share its kernel
// read past a narrower source's pixel when the destination is pre-allocated
// with more channels. Both of the kernel's channel loops do it.
//
// This is NOT the defect reported in #5638, and PR #5640 does not fix it.
// #5638 is the arithmetic in the "leftover" loop, `r[c] = 0.5 + 10 * a[c]`
// where the comment above it says the channels are copied unaltered. This
// program is about the `a[c]` in that line, and in the loop above it: the
// read itself is unbounded.
//
// `colorconvert` calls `IBAprep(roi, &dst, &src)` with no flags
// (`color_ocio.cpp:3189`), so no `IBAprep_CLAMP_MUTUAL_NCHANNELS`. With an
// initialized destination IBAprep takes `roi` from the destination alone
// ("Valid destination image. Just need to worry about ROI.",
// `imagebufalgo.cpp:120-128`) and never consults the source's channel count.
// `channelsToCopy = std::min(4, roi.nchannels())` (`:3028`) is therefore
// bounded by 4 and by the destination, never by the source, and
// `ImageBuf::ConstIterator::operator[]` does not bounds-check: it indexes a
// proxy built over the current pixel's raw pointer. So for a channel the
// source does not have, the read runs into the following pixel, and at the
// end of the buffer past the allocation.
//
// Contrast `ImageBufAlgo::paste`, which guards the same shape of loop with
// `c < src_nchans ? s[c] : D(0)`.
//
// Build (Windows, MSVC), adjusting the OIIO prefix:
//
//   cl /std:c++17 /EHsc /utf-8 colorconvert_unguarded_source_repro.cpp ^
//      /I <oiio-prefix>\include ^
//      /link /LIBPATH:<oiio-prefix>\lib OpenImageIO.lib OpenImageIO_Util.lib
//
// Build (Linux/macOS):
//
//   c++ -std=c++17 colorconvert_unguarded_source_repro.cpp -lOpenImageIO -o repro
//
// Exits non-zero if channels the source does not have received anything
// other than the sentinel they were filled with, or zero.

#include <OpenImageIO/Imath.h>
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
constexpr int SRC_CHAN = 2;  // narrow source
constexpr int DST_CHAN = 6;  // pre-allocated wider destination
constexpr float SENTINEL = -1000.0f;

// Every (pixel, channel) holds a different, predictable value:
// pixel (x, y) channel c holds 100 * (y * WIDTH + x) + c.
//
// The distinct values are the point. A constant-filled source would make an
// over-read return that same constant and hide the defect entirely. The
// hundreds place names the pixel the value really came from, so the output
// says how far past its own pixel each read reached.
ImageBuf
make_source()
{
    ImageBuf src(ImageSpec(WIDTH, HEIGHT, SRC_CHAN, TypeDesc::FLOAT));
    for (int y = 0; y < HEIGHT; ++y) {
        for (int x = 0; x < WIDTH; ++x) {
            float px[SRC_CHAN];
            for (int c = 0; c < SRC_CHAN; ++c)
                px[c] = 100.0f * float(y * WIDTH + x) + float(c);
            src.setpixel(x, y, px, SRC_CHAN);
        }
    }
    return src;
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

    ImageBuf dst(ImageSpec(WIDTH, HEIGHT, DST_CHAN, TypeDesc::FLOAT));
    std::vector<float> fill(DST_CHAN, SENTINEL);
    ImageBufAlgo::fill(dst, fill);

    // A non-identity matrix, so the processor is not a no-op. An identity
    // one returns early through ImageBufAlgo::copy and never reaches the
    // kernel. colormatrixtransform funnels into the same colorconvert_impl
    // as colorconvert itself, and needs no OCIO config to be present.
    const Imath::M44f m(2, 0, 0, 0, 0, 2, 0, 0, 0, 0, 2, 0, 0, 0, 0, 1);
    if (!ImageBufAlgo::colormatrixtransform(dst, src, m, /*unpremult=*/false)) {
        std::printf("FATAL: colormatrixtransform failed: %s\n",
                    dst.geterror().c_str());
        return 2;
    }
    std::printf("colormatrixtransform returned true, geterror: \"%s\"\n\n",
                dst.geterror().c_str());

    std::printf("source has %d channels, destination has %d; channels %d..%d\n"
                "do not exist in the source at all.\n\n",
                SRC_CHAN, DST_CHAN, SRC_CHAN, DST_CHAN - 1);

    int bad = 0;
    for (int y = 0; y < HEIGHT; ++y) {
        for (int x = 0; x < WIDTH; ++x) {
            float px[DST_CHAN] = {};
            dst.getpixel(x, y, px, DST_CHAN);
            std::printf("  (%d,%d) =", x, y);
            for (int c = 0; c < DST_CHAN; ++c)
                std::printf(" %10g", px[c]);
            std::printf("\n");
            for (int c = SRC_CHAN; c < DST_CHAN; ++c)
                if (px[c] != SENTINEL && px[c] != 0.0f)
                    ++bad;
        }
    }

    if (bad) {
        std::printf("\nFAIL: %d value(s) in channels the source does not have "
                    "were written from an unguarded read past its pixel.\n",
                    bad);
        return 1;
    }
    std::printf("\nPASS: channels the source does not have were left alone.\n");
    return 0;
}
