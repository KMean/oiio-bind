// Reproducer: ImageBufAlgo::colorconvert() corrupts channels past the fourth
// when the destination is a different ImageBuf and the transform is not a no-op.
//
// src/libOpenImageIO/color_ocio.cpp, colorconvert_impl():
//     if (channelsToCopy < roi.chend && (&R != &A)) {
//         // If there are "leftover" channels, just copy them
//         // unaltered from the source.
//         ...
//         r[c] = 0.5 + 10 * a[c];     <-- not a copy
//     }
//
// Public API only. Prints channels 4 and 5 for several cases.

#include <OpenImageIO/color.h>
#include <OpenImageIO/imagebuf.h>
#include <OpenImageIO/imagebufalgo.h>
#include <OpenImageIO/imageio.h>

#include <cstdio>
#include <cstdlib>
#include <string>
#include <vector>

using namespace OIIO;

static const int XRES = 4;
static const int YRES = 2;

// Channel fill values. Channels 4 and 5 are the ones under test.
static const float FILL[6] = { 0.18f, 0.18f, 0.18f, 1.0f, 0.125f, 0.875f };

static ImageBuf
make_buf(int nchannels)
{
    ImageSpec spec(XRES, YRES, nchannels, TypeDesc::FLOAT);
    if (nchannels == 6)
        spec.channelnames = { "R", "G", "B", "A", "Z", "mask" };
    ImageBuf buf(spec);
    if (!ImageBufAlgo::fill(buf, cspan<float>(FILL, FILL + nchannels))) {
        std::printf("FATAL: fill failed: %s\n", buf.geterror().c_str());
        std::exit(1);
    }
    return buf;
}

static void
report(const char* label, const ImageBuf& buf, int nchannels)
{
    std::vector<float> px(nchannels, 0.0f);
    buf.getpixel(0, 0, px.data(), nchannels);
    std::printf("  %-34s", label);
    for (int c = 0; c < nchannels; ++c)
        std::printf(" c%d=%-9.6g", c, px[c]);
    std::printf("\n");
}

int
main()
{
    // Headers we compiled against, and the library we actually linked, so the
    // report cannot be accused of measuring the wrong build.
    std::printf("compiled against OIIO %s; linked lib reports \"%s\" (%d)\n",
                OIIO_VERSION_STRING,
                std::string(get_string_attribute("version")).c_str(),
                openimageio_version());

    ColorConfig config;
    if (config.has_error()) {
        std::printf("FATAL: ColorConfig: %s\n", config.geterror().c_str());
        return 1;
    }
    std::printf("OCIO config: %s\n", config.configname().c_str());

    const char* FROM = "ACEScg";
    const char* TO   = "sRGB - Display";

    // Confirm both spaces really exist in whatever config we got.
    std::vector<std::string> names = config.getColorSpaceNames();
    std::printf("config has %d color spaces\n", (int)names.size());
    bool have_from = false, have_to = false;
    for (const std::string& n : names) {
        if (n == FROM)
            have_from = true;
        if (n == TO)
            have_to = true;
    }
    if (!have_from || !have_to) {
        std::printf("FATAL: need both \"%s\" (found=%d) and \"%s\" (found=%d).\n"
                    "Available:\n",
                    FROM, (int)have_from, TO, (int)have_to);
        for (const std::string& n : names)
            std::printf("    %s\n", n.c_str());
        return 1;
    }

    // A real, non-identity transform. Fail loudly if it is a no-op, because a
    // no-op would return early through ImageBufAlgo::copy() and prove nothing.
    ColorProcessorHandle proc = config.createColorProcessor(FROM, TO);
    if (!proc) {
        std::printf("FATAL: createColorProcessor(%s -> %s) failed: %s\n", FROM,
                    TO, config.geterror().c_str());
        return 1;
    }
    if (proc->isNoOp()) {
        std::printf("FATAL: %s -> %s is a no-op; test would be vacuous.\n", FROM,
                    TO);
        return 1;
    }
    std::printf("processor %s -> %s: isNoOp=0 (real transform)\n", FROM, TO);

    // And an identity transform, for the control case.
    ColorProcessorHandle ident = config.createColorProcessor(FROM, FROM);
    if (!ident) {
        std::printf("FATAL: createColorProcessor(%s -> %s) failed: %s\n", FROM,
                    FROM, config.geterror().c_str());
        return 1;
    }
    std::printf("processor %s -> %s: isNoOp=%d\n", FROM, FROM,
                (int)ident->isNoOp());

    std::printf("\ninput fill:\n");
    report("6 channels, as filled", make_buf(6), 6);

    // --- Case 1: the bug. 6 channels, distinct dst, real transform. ---
    std::printf("\nCase 1  6ch, DISTINCT dst, real transform  (expect c4/c5 "
                "unchanged)\n");
    {
        ImageBuf src = make_buf(6);
        ImageBuf dst;
        if (!ImageBufAlgo::colorconvert(dst, src, proc.get(),
                                        /*unpremult=*/false)) {
            std::printf("FATAL: colorconvert failed: %s\n",
                        dst.geterror().c_str());
            return 1;
        }
        report("result", dst, 6);

        std::vector<float> px(6, 0.0f);
        dst.getpixel(0, 0, px.data(), 6);
        std::printf("  %-34s c4=%-9.6g c5=%-9.6g\n", "expected (a copy)",
                    FILL[4], FILL[5]);
        std::printf("  %-34s c4=%-9.6g c5=%-9.6g\n", "expected if 0.5+10*x",
                    0.5f + 10.0f * FILL[4], 0.5f + 10.0f * FILL[5]);
        bool is_copy = px[4] == FILL[4] && px[5] == FILL[5];
        bool is_bug  = px[4] == 0.5f + 10.0f * FILL[4]
                      && px[5] == 0.5f + 10.0f * FILL[5];
        std::printf("  VERDICT: copy=%d  0.5+10*x=%d  -> %s\n", (int)is_copy,
                    (int)is_bug,
                    is_bug ? "CORRUPTION REPRODUCED"
                           : (is_copy ? "correct" : "neither"));
    }

    // --- Case 2: control. 4 channels, distinct dst, real transform. ---
    std::printf("\nCase 2  4ch control, DISTINCT dst, real transform  (no "
                "leftover channels)\n");
    {
        ImageBuf src = make_buf(4);
        ImageBuf dst;
        if (!ImageBufAlgo::colorconvert(dst, src, proc.get(),
                                        /*unpremult=*/false)) {
            std::printf("FATAL: colorconvert failed: %s\n",
                        dst.geterror().c_str());
            return 1;
        }
        report("result", dst, 4);
    }

    // --- Case 3: control. 6 channels, distinct dst, identity transform. ---
    std::printf("\nCase 3  6ch, DISTINCT dst, IDENTITY transform  (early-outs "
                "via copy)\n");
    {
        ImageBuf src = make_buf(6);
        ImageBuf dst;
        if (!ImageBufAlgo::colorconvert(dst, src, ident.get(),
                                        /*unpremult=*/false)) {
            std::printf("FATAL: colorconvert failed: %s\n",
                        dst.geterror().c_str());
            return 1;
        }
        report("result", dst, 6);
    }

    // --- Case 4: control. 6 channels, IN-PLACE, real transform. ---
    std::printf("\nCase 4  6ch, IN-PLACE dst==src, real transform  (&R==&A "
                "guard)\n");
    {
        ImageBuf buf = make_buf(6);
        if (!ImageBufAlgo::colorconvert(buf, buf, proc.get(),
                                        /*unpremult=*/false)) {
            std::printf("FATAL: colorconvert failed: %s\n",
                        buf.geterror().c_str());
            return 1;
        }
        report("result", buf, 6);
    }

    return 0;
}
