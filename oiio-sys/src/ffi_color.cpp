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
