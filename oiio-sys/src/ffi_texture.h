#pragma once

#include <OpenImageIO/texture.h>
#include <rust/cxx.h>
#include <memory>

namespace oiio {
using TextureSystem = OIIO::TextureSystem;

struct TextureLookupOptions;

// A texture system, sharing the process-wide one or private to this caller.
std::shared_ptr<TextureSystem>
texturesystem_create(bool shared);

// A filtered lookup. The derivatives describe how far the texture coordinate
// moves per pixel, which is what lets OpenImageIO pick a mip level and filter
// width; passing zeros asks for a point sample of the highest resolution.
//
// `result` receives one value per channel and must be exactly nchannels long.
bool
texturesystem_texture(TextureSystem& texturesystem, const rust::Str filename,
                      const TextureLookupOptions& options, float s, float t,
                      float dsdx, float dtdx, float dsdy, float dtdy,
                      rust::Slice<float> result, rust::String& error);

rust::String
texturesystem_geterror(TextureSystem& texturesystem);

bool
texturesystem_attribute_int(TextureSystem& texturesystem, const rust::Str name,
                            int value);

bool
texturesystem_attribute_float(TextureSystem& texturesystem,
                              const rust::Str name, float value);

rust::String
texturesystem_getstats(const TextureSystem& texturesystem, int level);

void
texturesystem_invalidate(TextureSystem& texturesystem, const rust::Str filename,
                         bool force);

void
texturesystem_invalidate_all(TextureSystem& texturesystem, bool force);

// Resolution of the texture, as the texture system sees it. Zero when the
// file cannot be read.
bool
texturesystem_resolution(TextureSystem& texturesystem, const rust::Str filename,
                         rust::Slice<int> resolution);

}  // namespace oiio
