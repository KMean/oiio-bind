#include "ffi_pixel.h"
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

// One lookup target, by name or by handle; the difference between the two
// public paths is confined to how get_texture_info is asked.
struct LookupTarget {
    TextureSystem& system;
    OIIO::ustring name;
    OIIO::TextureSystem::TextureHandle* handle = nullptr;

    bool info(int subimage, const char* dataname, int* value) const
    {
        // A malformed UDIM-like name ("+<UDIM>.exr") throws std::regex_error
        // out of OpenImageIO's udim_setup, which compiles the pattern
        // unguarded (imagecache.cpp:4159); uncaught it would cross the cxx
        // noexcept boundary and abort the process. A thrown probe is a
        // failed probe.
        try {
            if (handle)
                return system.get_texture_info(handle, nullptr, subimage,
                                               OIIO::ustring(dataname),
                                               OIIO::TypeInt, value);
            return system.get_texture_info(name, subimage,
                                           OIIO::ustring(dataname),
                                           OIIO::TypeInt, value);
        } catch (const std::exception&) {
            return false;
        }
    }
};

enum class Prepared { Proceed, AnswerMissing, Refused };

// The shared preamble of every filtered lookup: bound the result slice, wire
// the missing color, and validate subimage and firstchannel against the
// file. OpenImageIO checks neither: subimageinfo() indexes m_subimages with
// an OIIO_DASSERT that release builds compile out, and while the samplers
// clamp the channel *count* they compute their texel addresses from the raw
// firstchannel, so both walk off the end of a cached tile. A tile carries
// only OIIO_SIMD_MAX_SIZE_BYTES of slack past its last texel.
//
// Sets `asked` to the channel count the lookup should request: asking for
// more channels than remain is allowed and documented — the extra ones take
// the fill value — but OpenImageIO does not implement it that way past four
// channels (it recurses, walking firstchannel upward with no bound), so the
// lookup asks only for what exists and the caller fills the rest.
//
// AnswerMissing means the file cannot be opened and a missing color is set:
// the caller runs the lookup as-is with the full channel count, whose
// missing-texture path fills the result before any subimage indexing. The
// probe also fails for an existing UDIM set whose tiles disagree on the
// answer; only the missing file may bypass the bounds checks, because an
// existing file would carry the raw subimage into an unchecked vector index
// upstream.
// The exact missing-color fill. OpenImageIO's own fill would repeat the
// color's first four values across wider lookups — its >4-channel recursion
// advances the result pointer but never the missingcolor pointer — so the
// shim answers directly; the color was validated to one value per result
// channel before it was wired into the options.
void
fill_missing(rust::Slice<const float> missing_color, rust::Slice<float> result)
{
    for (std::size_t i = 0; i < result.size(); ++i)
        result[i] = missing_color[i];
}

Prepared
prepare_lookup(const LookupTarget& target, OIIO::TextureOpt& opt,
               rust::Slice<const float> missing_color,
               rust::Slice<float> result, int& asked, rust::String& error)
{
    error = rust::String();
    if (result.empty()
        || result.size() > std::size_t(std::numeric_limits<int>::max())) {
        error = rust::String::lossy(
            "the result slice must hold between one and INT_MAX channels");
        return Prepared::Refused;
    }
    if (!missing_color.empty()) {
        // missing_texture() reads missingcolor[0..nchannels), so a short
        // slice would be read past its end.
        if (missing_color.size() < result.size()) {
            error = rust::String::lossy(
                "the missing color needs one value per result channel");
            return Prepared::Refused;
        }
        opt.missingcolor = missing_color.data();
    }

    int subimages       = 0;
    const bool answered = target.info(0, "subimages", &subimages);
    if (!answered || subimages <= 0) {
        int exists = 0;
        (void)target.info(0, "exists", &exists);
        // `answered && subimages <= 0` is upstream's UDIM aggregate copying
        // uninitialized stack as a success — it happens only when every
        // populated tile is unreadable, which is exactly what the missing
        // color exists for. A probe that failed outright on an existing
        // file stays a refusal: an inconsistent UDIM set answers false
        // with healthy, readable tiles, and filling a missing color there
        // would lie.
        const bool unreadable = !exists || (answered && subimages <= 0);
        if (opt.missingcolor && unreadable) {
            (void)target.system.geterror(true);
            return Prepared::AnswerMissing;
        }
        error = take_texture_error(target.system);
        if (error.empty())
            error = rust::String::lossy(
                unreadable ? "the texture could not be opened"
                           : "the texture exists but could not answer its "
                             "subimage count; for a UDIM pattern, its tiles "
                             "may disagree");
        return Prepared::Refused;
    }
    if (opt.subimage < 0 || opt.subimage >= subimages) {
        error = rust::String::lossy(OIIO::Strutil::fmt::format(
            "subimage {} does not exist; the texture has {}", opt.subimage,
            subimages));
        return Prepared::Refused;
    }

    int channels          = 0;
    const bool ch_answered = target.info(opt.subimage, "channels", &channels);
    if (!ch_answered || channels <= 0) {
        // The same uninitialized-success possibility as the subimage count.
        if (opt.missingcolor && ch_answered && channels <= 0) {
            (void)target.system.geterror(true);
            return Prepared::AnswerMissing;
        }
        error = take_texture_error(target.system);
        if (error.empty())
            error = rust::String::lossy(
                "the texture did not report a channel count");
        return Prepared::Refused;
    }
    if (opt.firstchannel < 0 || opt.firstchannel >= channels) {
        error = rust::String::lossy(OIIO::Strutil::fmt::format(
            "first channel {} is outside the texture's {} channels",
            opt.firstchannel, channels));
        return Prepared::Refused;
    }

    asked = std::min(int(result.size()), channels - opt.firstchannel);
    return Prepared::Proceed;
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
    if (detail::malformed_udim_pattern(to_string_view(filename))) {
        // Stopped before find_file: OpenImageIO would throw compiling the
        // tile regex and leak a file-cache bin lock (see ffi_pixel.h).
        error = rust::String::lossy(
            "the UDIM-like name cannot form a valid tile pattern");
        return false;
    }
    OIIO::TextureOpt opt = to_texture_opt(options);
    const OIIO::ustring name(filename.data(), filename.size());
    int asked = 0;
    switch (prepare_lookup({ texturesystem, name }, opt, missing_color, result,
                           asked, error)) {
    case Prepared::Refused: return false;
    case Prepared::AnswerMissing: fill_missing(missing_color, result); return true;
    case Prepared::Proceed: break;
    }

    if (!texturesystem.texture(name, opt, s, t, dsdx, dtdx, dsdy, dtdy, asked,
                               result.data())) {
        error = take_texture_error(texturesystem);
        return false;
    }
    for (int channel = asked; channel < int(result.size()); ++channel)
        result[std::size_t(channel)] = opt.fill;
    return true;
}

bool
texturesystem_texture_by_handle(TextureSystem& texturesystem,
                                TextureHandle* handle,
                                const TextureLookupOptions& options,
                                rust::Slice<const float> missing_color,
                                float s, float t, float dsdx, float dtdx,
                                float dsdy, float dtdy,
                                rust::Slice<float> result, rust::String& error)
{
    if (!handle) {
        error = rust::String::lossy("the texture handle is null");
        return false;
    }
    OIIO::TextureOpt opt = to_texture_opt(options);
    int asked            = 0;
    switch (prepare_lookup({ texturesystem, OIIO::ustring(), handle }, opt,
                           missing_color, result, asked, error)) {
    case Prepared::Refused: return false;
    case Prepared::AnswerMissing: fill_missing(missing_color, result); return true;
    case Prepared::Proceed: break;
    }

    if (!texturesystem.texture(handle, nullptr, opt, s, t, dsdx, dtdx, dsdy,
                               dtdy, asked, result.data())) {
        error = take_texture_error(texturesystem);
        return false;
    }
    for (int channel = asked; channel < int(result.size()); ++channel)
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
    // The bounds and fill are the plain texture lookup's, shared through
    // prepare_lookup; the past-the-file channel fill is done here because
    // OpenImageIO zero-fills instead of honouring the fill value. Derivative
    // outputs are not exposed: upstream's zeroing loop dereferences the
    // second output when only one is given.
    if (detail::malformed_udim_pattern(to_string_view(filename))) {
        error = rust::String::lossy(
            "the UDIM-like name cannot form a valid tile pattern");
        return false;
    }
    OIIO::TextureOpt opt = to_texture_opt(options);
    const OIIO::ustring name(filename.data(), filename.size());
    int asked = 0;
    switch (prepare_lookup({ texturesystem, name }, opt, missing_color, result,
                           asked, error)) {
    case Prepared::Refused: return false;
    case Prepared::AnswerMissing: fill_missing(missing_color, result); return true;
    case Prepared::Proceed: break;
    }

    if (!texturesystem.environment(name, opt, Imath::V3f(r_x, r_y, r_z),
                                   Imath::V3f(drdx_x, drdx_y, drdx_z),
                                   Imath::V3f(drdy_x, drdy_y, drdy_z), asked,
                                   result.data())) {
        error = take_texture_error(texturesystem);
        return false;
    }
    for (int channel = asked; channel < int(result.size()); ++channel)
        result[std::size_t(channel)] = opt.fill;
    return true;
}

bool
texturesystem_environment_by_handle(TextureSystem& texturesystem,
                                    TextureHandle* handle,
                                    const TextureLookupOptions& options,
                                    rust::Slice<const float> missing_color,
                                    float r_x, float r_y, float r_z,
                                    float drdx_x, float drdx_y, float drdx_z,
                                    float drdy_x, float drdy_y, float drdy_z,
                                    rust::Slice<float> result,
                                    rust::String& error)
{
    if (!handle) {
        error = rust::String::lossy("the texture handle is null");
        return false;
    }
    OIIO::TextureOpt opt = to_texture_opt(options);
    int asked            = 0;
    switch (prepare_lookup({ texturesystem, OIIO::ustring(), handle }, opt,
                           missing_color, result, asked, error)) {
    case Prepared::Refused: return false;
    case Prepared::AnswerMissing: fill_missing(missing_color, result); return true;
    case Prepared::Proceed: break;
    }

    if (!texturesystem.environment(handle, nullptr, opt,
                                   Imath::V3f(r_x, r_y, r_z),
                                   Imath::V3f(drdx_x, drdx_y, drdx_z),
                                   Imath::V3f(drdy_x, drdy_y, drdy_z), asked,
                                   result.data())) {
        error = take_texture_error(texturesystem);
        return false;
    }
    for (int channel = asked; channel < int(result.size()); ++channel)
        result[std::size_t(channel)] = opt.fill;
    return true;
}

TextureHandle*
texturesystem_get_texture_handle(TextureSystem& texturesystem,
                                 const rust::Str filename)
{
    // Stopped before find_file: OpenImageIO would throw compiling the tile
    // regex and leak a file-cache bin lock (see ffi_pixel.h). The catch is
    // belt and braces.
    if (detail::malformed_udim_pattern(to_string_view(filename)))
        return nullptr;
    try {
        return texturesystem.get_texture_handle(
            OIIO::ustring(filename.data(), filename.size()));
    } catch (const std::exception&) {
        return nullptr;
    }
}

bool
texturesystem_handle_good(TextureSystem& texturesystem, TextureHandle* handle)
{
    return handle && texturesystem.good(handle);
}

bool
texturesystem_handle_exists(TextureSystem& texturesystem, TextureHandle* handle)
{
    // good() alone is only the broken flag, which a never-opened record has
    // not earned yet — a handle to a missing file passes it. The exists
    // query verifies the file (opening its header if needed) and eats any
    // error it generates, by upstream design.
    if (!handle)
        return false;
    int exists = 0;
    if (!texturesystem.get_texture_info(handle, nullptr, 0,
                                        OIIO::ustring("exists"), OIIO::TypeInt,
                                        &exists))
        return false;
    return exists != 0;
}

rust::String
texturesystem_handle_filename(TextureSystem& texturesystem,
                              TextureHandle* handle)
{
    if (!handle)
        return rust::String();
    const OIIO::ustring name = texturesystem.filename_from_handle(handle);
    if (!name.size())
        return rust::String();
    return rust::String::lossy(name.data(), name.size());
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
    // A name that cannot even be compiled into a tile pattern is not a
    // usable UDIM set; stopping it here also keeps OpenImageIO from
    // throwing mid-lock (see ffi_pixel.h).
    if (detail::malformed_udim_pattern(to_string_view(filename)))
        return false;
    try {
        return texturesystem.is_udim(
            OIIO::ustring(filename.data(), filename.size()));
    } catch (const std::exception&) {
        return false;
    }
}

rust::String
texturesystem_resolve_udim(TextureSystem& texturesystem,
                           const rust::Str pattern, float s, float t)
{
    if (detail::malformed_udim_pattern(to_string_view(pattern)))
        return rust::String();
    try {
        const OIIO::ustring name(pattern.data(), pattern.size());
        OIIO::TextureSystem::TextureHandle* handle
            = texturesystem.resolve_udim(name, s, t);
        if (!handle)
            return rust::String();
        const OIIO::ustring resolved
            = texturesystem.filename_from_handle(handle);
        return rust::String::lossy(resolved.data(), resolved.size());
    } catch (const std::exception&) {
        // Regex barrier; an uncompilable pattern resolves nothing.
        return rust::String();
    }
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
    if (detail::malformed_udim_pattern(to_string_view(pattern))) {
        filenames.clear();
        return;
    }
    try {
        texturesystem.inventory_udim(name, tiles, nutiles, nvtiles);
    } catch (const std::exception&) {
        // Regex barrier; an uncompilable pattern has no inventory.
        tiles.clear();
        nutiles = 0;
        nvtiles = 0;
    }
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

    if (detail::malformed_udim_pattern(to_string_view(filename)))
        return false;
    const OIIO::ustring name(filename.data(), filename.size());
    int values[2] = { 0, 0 };
    try {
        if (!texturesystem.get_texture_info(name, 0,
                                            OIIO::ustring("resolution"),
                                            OIIO::TypeDesc(OIIO::TypeDesc::INT,
                                                           2),
                                            values))
            return false;
    } catch (const std::exception&) {
        // Regex barrier, as in LookupTarget::info.
        return false;
    }
    resolution[0] = values[0];
    resolution[1] = values[1];
    return true;
}

}  // namespace oiio
