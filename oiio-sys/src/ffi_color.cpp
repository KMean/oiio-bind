#include "ffi_color.h"

#include <string>

namespace oiio {
namespace {
inline OIIO::string_view
to_string_view(const rust::Str text) noexcept
{
    return OIIO::string_view(text.data(), text.size());
}

// Names come back as const char*, which may be null when an index or role is
// unknown. Empty is the honest answer for those.
inline rust::String
to_rust_string(const char* text)
{
    if (text == nullptr)
        return rust::String();
    return rust::String::lossy(text, std::char_traits<char>::length(text));
}
}  // namespace

std::unique_ptr<ColorConfig>
colorconfig_default()
{
    return std::make_unique<ColorConfig>();
}

std::unique_ptr<ColorConfig>
colorconfig_from_file(const rust::Str filename)
{
    return std::make_unique<ColorConfig>(
        std::string(filename.data(), filename.size()));
}

bool
colorconfig_supports_opencolorio()
{
    return OIIO::ColorConfig::supportsOpenColorIO();
}

rust::String
colorconfig_name(const ColorConfig& config)
{
    const std::string name = config.configname();
    return rust::String::lossy(name.data(), name.size());
}

rust::String
colorconfig_geterror(const ColorConfig& config)
{
    const std::string error = config.geterror(true);
    return rust::String::lossy(error.data(), error.size());
}

int
colorconfig_color_space_index(const ColorConfig& config, rust::Str name)
{
    // Exactly the resolution a conversion performs, so the two agree by
    // construction: getColorSpaceIndex matches case-insensitively with
    // Strutil::iequals and falls back to equivalent() for aliases — and
    // equivalent() runs both names through ColorConfig::resolve, which also
    // accepts defined roles like "scene_linear" (color.h documents resolve
    // as taking "a color space, an alias, a role"). So a defined role
    // matches here too, just as it does in colorconvert. Comparing against
    // the enumerated names instead rejected every alias, every casing
    // difference, and every role that colorconvert accepts.
    const std::string_view c_name(name.data(), name.size());
    return config.getColorSpaceIndex(c_name);
}

rust::Vec<rust::String>
colorconfig_color_space_names(const ColorConfig& config)
{
    rust::Vec<rust::String> names;
    const int count = config.getNumColorSpaces();
    for (int index = 0; index < count; ++index)
        names.push_back(to_rust_string(config.getColorSpaceNameByIndex(index)));
    return names;
}

rust::Vec<rust::String>
colorconfig_role_names(const ColorConfig& config)
{
    rust::Vec<rust::String> names;
    const int count = config.getNumRoles();
    for (int index = 0; index < count; ++index)
        names.push_back(to_rust_string(config.getRoleByIndex(index)));
    return names;
}

rust::String
colorconfig_color_space_for_role(const ColorConfig& config, const rust::Str role)
{
    return to_rust_string(config.getColorSpaceNameByRole(to_string_view(role)));
}

}  // namespace oiio
