#include "ffi_texture.h"
#include "oiio-sys/src/texture.rs.h"

#include <Imath/ImathVec.h>

#include <limits>
#include <string>

namespace oiio {
namespace {

inline OIIO::string_view
to_string_view(const rust::Str text) noexcept
{
    return OIIO::string_view(text.data(), text.size());
}

// Clamp an option that crossed the bridge as an integer back onto its enum,
// so a value from outside cannot select something OpenImageIO does not have.
template<typename Enum>
inline Enum
to_enum(int value, int limit, Enum fallback)
{
    if (value < 0 || value >= limit)
        return fallback;
    return static_cast<Enum>(value);
}

OIIO::TextureOpt
to_texture_opt(const TextureLookupOptions& options)
{
    OIIO::TextureOpt opt;
    opt.firstchannel = options.first_channel;
    opt.subimage     = options.subimage;
    opt.swrap        = to_enum(options.s_wrap, int(OIIO::Tex::Wrap::Last),
                               OIIO::Tex::Wrap::Default);
    opt.twrap        = to_enum(options.t_wrap, int(OIIO::Tex::Wrap::Last),
                               OIIO::Tex::Wrap::Default);
    opt.mipmode      = to_enum(options.mip_mode, 5, OIIO::Tex::MipMode::Default);
    opt.interpmode   = to_enum(options.interp_mode, 4,
                               OIIO::Tex::InterpMode::SmartBicubic);
    opt.sblur        = options.s_blur;
    opt.tblur        = options.t_blur;
    opt.swidth       = options.s_width;
    opt.twidth       = options.t_width;
    opt.fill         = options.fill;
    return opt;
}

rust::String
take_texture_error(TextureSystem& texturesystem)
{
    const std::string error = texturesystem.geterror(true);
    return rust::String::lossy(error.data(), error.size());
}

}  // namespace

std::shared_ptr<TextureSystem>
texturesystem_create(bool shared)
{
    return OIIO::TextureSystem::create(shared);
}

bool
texturesystem_texture(TextureSystem& texturesystem, const rust::Str filename,
                      const TextureLookupOptions& options,
                      rust::Slice<const float> missing_color, float s, float t,
                      float dsdx, float dtdx, float dsdy, float dtdy,
                      rust::Slice<float> result, rust::String& error)
{
    error = rust::String();
    if (result.empty()
        || result.size() > std::size_t(std::numeric_limits<int>::max())) {
        error = rust::String::lossy(
            "the result slice must hold between one and INT_MAX channels");
        return false;
    }

    OIIO::TextureOpt opt = to_texture_opt(options);
    const OIIO::ustring name(filename.data(), filename.size());

    if (!missing_color.empty()) {
        // missing_texture() reads missingcolor[0..nchannels), so a short
        // slice would be read past its end.
        if (missing_color.size() < result.size()) {
            error = rust::String::lossy(
                "the missing color needs one value per result channel");
            return false;
        }
        opt.missingcolor = missing_color.data();
    }

    // Bound subimage and the channel range against the file before the lookup.
    // OpenImageIO checks neither: subimageinfo() indexes m_subimages with an
    // OIIO_DASSERT that release builds compile out, and while the samplers
    // clamp the channel *count* they compute their texel addresses from the
    // raw firstchannel, so both walk off the end of a cached tile. A tile
    // carries only OIIO_SIMD_MAX_SIZE_BYTES of slack past its last texel.
    int subimages = 0;
    if (!texturesystem.get_texture_info(name, 0, OIIO::ustring("subimages"),
                                        OIIO::TypeInt, &subimages)) {
        if (opt.missingcolor) {
            // The file is missing or broken, which is exactly what the
            // missing color exists for. The lookup takes the missing-texture
            // path before any subimage indexing, fills the result from the
            // missing color, and reports success.
            (void)texturesystem.geterror(true);
            if (texturesystem.texture(name, opt, s, t, dsdx, dtdx, dsdy, dtdy,
                                      int(result.size()), result.data()))
                return true;
            error = take_texture_error(texturesystem);
            return false;
        }
        error = take_texture_error(texturesystem);
        if (error.empty())
            error = rust::String::lossy("the texture could not be opened");
        return false;
    }
    if (opt.subimage < 0 || opt.subimage >= subimages) {
        error = rust::String::lossy(OIIO::Strutil::fmt::format(
            "subimage {} does not exist; the texture has {}", opt.subimage,
            subimages));
        return false;
    }

    int channels = 0;
    if (!texturesystem.get_texture_info(name, opt.subimage,
                                        OIIO::ustring("channels"),
                                        OIIO::TypeInt, &channels)
        || channels <= 0) {
        error = take_texture_error(texturesystem);
        if (error.empty())
            error = rust::String::lossy(
                "the texture did not report a channel count");
        return false;
    }
    if (opt.firstchannel < 0 || opt.firstchannel >= channels) {
        error = rust::String::lossy(OIIO::Strutil::fmt::format(
            "first channel {} is outside the texture's {} channels",
            opt.firstchannel, channels));
        return false;
    }

    // Asking for more channels than remain is allowed, and documented: the
    // extra ones take the fill value. OpenImageIO does not implement it that
    // way past four channels -- it recurses, walking firstchannel upward with
    // no bound -- so ask it only for what exists and fill the rest here.
    const int wanted    = int(result.size());
    const int available = channels - opt.firstchannel;
    const int asked     = std::min(wanted, available);

    if (!texturesystem.texture(name, opt, s, t, dsdx, dtdx, dsdy, dtdy, asked,
                               result.data())) {
        error = take_texture_error(texturesystem);
        return false;
    }
    for (int channel = asked; channel < wanted; ++channel)
        result[std::size_t(channel)] = opt.fill;
    return true;
}

bool
texturesystem_environment(TextureSystem& texturesystem,
                          const rust::Str filename,
                          const TextureLookupOptions& options,
                          rust::Slice<const float> missing_color, float r_x,
                          float r_y, float r_z, float drdx_x, float drdx_y,
                          float drdx_z, float drdy_x, float drdy_y,
                          float drdy_z, rust::Slice<float> result,
                          rust::String& error)
{
    // The same bounds the plain texture lookup carries, for the same
    // reasons: subimage and firstchannel are trusted unchecked past release
    // builds, and the past-the-file channel fill is done here because
    // OpenImageIO zero-fills instead of honouring the fill value. Derivative
    // outputs are not exposed: upstream's zeroing loop dereferences the
    // second output when only one is given.
    error = rust::String();
    if (result.empty()
        || result.size() > std::size_t(std::numeric_limits<int>::max())) {
        error = rust::String::lossy(
            "the result slice must hold between one and INT_MAX channels");
        return false;
    }

    OIIO::TextureOpt opt = to_texture_opt(options);
    const OIIO::ustring name(filename.data(), filename.size());

    if (!missing_color.empty()) {
        if (missing_color.size() < result.size()) {
            error = rust::String::lossy(
                "the missing color needs one value per result channel");
            return false;
        }
        opt.missingcolor = missing_color.data();
    }

    int subimages = 0;
    if (!texturesystem.get_texture_info(name, 0, OIIO::ustring("subimages"),
                                        OIIO::TypeInt, &subimages)) {
        if (opt.missingcolor) {
            (void)texturesystem.geterror(true);
            if (texturesystem.environment(name, opt,
                                          Imath::V3f(r_x, r_y, r_z),
                                          Imath::V3f(drdx_x, drdx_y, drdx_z),
                                          Imath::V3f(drdy_x, drdy_y, drdy_z),
                                          int(result.size()), result.data()))
                return true;
            error = take_texture_error(texturesystem);
            return false;
        }
        error = take_texture_error(texturesystem);
        if (error.empty())
            error = rust::String::lossy("the texture could not be opened");
        return false;
    }
    if (opt.subimage < 0 || opt.subimage >= subimages) {
        error = rust::String::lossy(OIIO::Strutil::fmt::format(
            "subimage {} does not exist; the texture has {}", opt.subimage,
            subimages));
        return false;
    }

    int channels = 0;
    if (!texturesystem.get_texture_info(name, opt.subimage,
                                        OIIO::ustring("channels"),
                                        OIIO::TypeInt, &channels)
        || channels <= 0) {
        error = take_texture_error(texturesystem);
        if (error.empty())
            error = rust::String::lossy(
                "the texture did not report a channel count");
        return false;
    }
    if (opt.firstchannel < 0 || opt.firstchannel >= channels) {
        error = rust::String::lossy(OIIO::Strutil::fmt::format(
            "first channel {} is outside the texture's {} channels",
            opt.firstchannel, channels));
        return false;
    }

    const int wanted    = int(result.size());
    const int available = channels - opt.firstchannel;
    const int asked     = std::min(wanted, available);

    if (!texturesystem.environment(name, opt, Imath::V3f(r_x, r_y, r_z),
                                   Imath::V3f(drdx_x, drdx_y, drdx_z),
                                   Imath::V3f(drdy_x, drdy_y, drdy_z), asked,
                                   result.data())) {
        error = take_texture_error(texturesystem);
        return false;
    }
    for (int channel = asked; channel < wanted; ++channel)
        result[std::size_t(channel)] = opt.fill;
    return true;
}

rust::String
texturesystem_geterror(TextureSystem& texturesystem)
{
    const std::string error = texturesystem.geterror(true);
    return rust::String::lossy(error.data(), error.size());
}

bool
texturesystem_is_udim(TextureSystem& texturesystem, const rust::Str filename)
{
    return texturesystem.is_udim(
        OIIO::ustring(filename.data(), filename.size()));
}

rust::String
texturesystem_resolve_udim(TextureSystem& texturesystem,
                           const rust::Str pattern, float s, float t)
{
    const OIIO::ustring name(pattern.data(), pattern.size());
    OIIO::TextureSystem::TextureHandle* handle
        = texturesystem.resolve_udim(name, s, t);
    if (!handle)
        return rust::String();
    const OIIO::ustring resolved = texturesystem.filename_from_handle(handle);
    return rust::String::lossy(resolved.data(), resolved.size());
}

void
texturesystem_inventory_udim(TextureSystem& texturesystem,
                             const rust::Str pattern,
                             rust::Vec<rust::String>& filenames, int& nutiles,
                             int& nvtiles)
{
    const OIIO::ustring name(pattern.data(), pattern.size());
    std::vector<OIIO::ustring> tiles;
    nutiles = 0;
    nvtiles = 0;
    texturesystem.inventory_udim(name, tiles, nutiles, nvtiles);
    filenames.clear();
    filenames.reserve(tiles.size());
    for (const OIIO::ustring& tile : tiles) {
        // An unpopulated tile is a default ustring whose data pointer is
        // null; hand it over as an empty string rather than a null read.
        if (tile.size())
            filenames.push_back(rust::String::lossy(tile.data(), tile.size()));
        else
            filenames.push_back(rust::String());
    }
}

bool
texturesystem_attribute_int(TextureSystem& texturesystem, const rust::Str name,
                            int value)
{
    return texturesystem.attribute(to_string_view(name), value);
}

bool
texturesystem_attribute_float(TextureSystem& texturesystem,
                              const rust::Str name, float value)
{
    return texturesystem.attribute(to_string_view(name), value);
}

rust::String
texturesystem_getstats(const TextureSystem& texturesystem, int level)
{
    const std::string stats = texturesystem.getstats(level);
    return rust::String::lossy(stats.data(), stats.size());
}

void
texturesystem_invalidate(TextureSystem& texturesystem, const rust::Str filename,
                         bool force)
{
    texturesystem.invalidate(OIIO::ustring(filename.data(), filename.size()),
                             force);
}

void
texturesystem_invalidate_all(TextureSystem& texturesystem, bool force)
{
    texturesystem.invalidate_all(force);
}

bool
texturesystem_resolution(TextureSystem& texturesystem, const rust::Str filename,
                         rust::Slice<int> resolution)
{
    if (resolution.size() < 2)
        return false;
    resolution[0] = 0;
    resolution[1] = 0;

    const OIIO::ustring name(filename.data(), filename.size());
    int values[2] = { 0, 0 };
    if (!texturesystem.get_texture_info(name, 0, OIIO::ustring("resolution"),
                                        OIIO::TypeDesc(OIIO::TypeDesc::INT, 2),
                                        values))
        return false;
    resolution[0] = values[0];
    resolution[1] = values[1];
    return true;
}

}  // namespace oiio
