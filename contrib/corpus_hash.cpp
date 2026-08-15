// The independent C++ arm of the corpus differential: reads image files with
// nothing but public OpenImageIO C++ API — none of oiio-sys' shims — and
// prints one line per subimage: path, subimage index, dimensions, and an
// FNV-1a 64 hash of the pixels converted to float. `examples/corpus_hash.rs`
// is the crate arm printing the identical format; diff the outputs.
//
// Reads file paths from stdin, one per line. Deep subimages print DEEP (the
// deep API is compared elsewhere); files or subimages that fail print ERROR.
//
// Build (MSVC, against the same vcpkg OpenImageIO the crate links):
//   cl /std:c++17 /EHsc /utf-8 /MD corpus_hash.cpp
//      /I C:\vcpkg\installed\x64-windows\include
//      /link /LIBPATH:C:\vcpkg\installed\x64-windows\lib
//      OpenImageIO.lib OpenImageIO_Util.lib
// Run with C:\vcpkg\installed\x64-windows\bin on PATH.

#include <cstdint>
#include <cstdio>
#include <iostream>
#include <string>
#include <vector>

#include <OpenImageIO/imageio.h>

static uint64_t
fnv1a64(const unsigned char* data, size_t len)
{
    uint64_t hash = 14695981039346656037ull;
    for (size_t i = 0; i < len; ++i) {
        hash ^= data[i];
        hash *= 1099511628211ull;
    }
    return hash;
}

int
main()
{
    std::string path;
    while (std::getline(std::cin, path)) {
        if (path.empty())
            continue;
        auto input = OIIO::ImageInput::open(path);
        if (!input) {
            // Drain the message so OpenImageIO's at-exit "pending error"
            // dump cannot land in the output being diffed.
            (void)OIIO::geterror(true);
            std::printf("%s\t-\tERROR\n", path.c_str());
            continue;
        }
        for (int s = 0;; ++s) {
            if (!input->seek_subimage(s, 0))
                break;
            const OIIO::ImageSpec& spec = input->spec();
            if (spec.deep) {
                std::printf("%s\t%d\tDEEP\n", path.c_str(), s);
                continue;
            }
            const uint64_t values = spec.image_pixels()
                                    * uint64_t(spec.nchannels);
            // The corpora's largest flat files are far below this; anything
            // over it would only slow the sweep down.
            if (values > (uint64_t(1) << 28)) {
                std::printf("%s\t%d\tSKIPPED-LARGE\n", path.c_str(), s);
                continue;
            }
            std::vector<float> pixels(static_cast<size_t>(values));
            if (!input->read_image(s, 0, 0, -1, OIIO::TypeFloat,
                                   pixels.data())) {
                (void)input->geterror(true);
                std::printf("%s\t%d\tERROR\n", path.c_str(), s);
                continue;
            }
            const unsigned char* bytes
                = reinterpret_cast<const unsigned char*>(pixels.data());
            const size_t nbytes = pixels.size() * sizeof(float);
            const uint64_t hash = fnv1a64(bytes, nbytes);
            std::printf("%s\t%d\t%dx%dx%d\t%016llx\n", path.c_str(), s,
                        spec.width, spec.height, spec.nchannels,
                        (unsigned long long)hash);
        }
        input->close();
    }
    return 0;
}
