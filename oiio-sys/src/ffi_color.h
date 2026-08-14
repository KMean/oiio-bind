#pragma once

#include <OpenImageIO/color.h>
#include <rust/cxx.h>
#include <memory>

namespace oiio {
using ColorConfig = OIIO::ColorConfig;

// The default configuration: whatever $OCIO names, or OpenImageIO's built-in
// one when that is unset.
std::unique_ptr<ColorConfig>
colorconfig_default();

// A named configuration file.
std::unique_ptr<ColorConfig>
colorconfig_from_file(const rust::Str filename);

bool
colorconfig_supports_opencolorio();

rust::String
colorconfig_name(const ColorConfig& config);

rust::String
colorconfig_geterror(const ColorConfig& config);

int
colorconfig_color_space_index(const ColorConfig& config, rust::Str name);

rust::Vec<rust::String>
colorconfig_color_space_names(const ColorConfig& config);

rust::Vec<rust::String>
colorconfig_role_names(const ColorConfig& config);

// The colour space a role such as "scene_linear" or "default" resolves to,
// empty when the role is not defined.
rust::String
colorconfig_color_space_for_role(const ColorConfig& config, const rust::Str role);

}  // namespace oiio
