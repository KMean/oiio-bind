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
//
// A non-empty `missing_color` (at least one value per result channel) makes
// a missing or broken texture fill the result with it and succeed, which is
// OpenImageIO's own missingcolor contract; empty means missing files error.
bool
texturesystem_texture(TextureSystem& texturesystem, const rust::Str filename,
                      const TextureLookupOptions& options,
                      rust::Slice<const float> missing_color, float s, float t,
                      float dsdx, float dtdx, float dsdy, float dtdy,
                      rust::Slice<float> result, rust::String& error);

// An environment lookup by direction vector; the guards, the fill of the
// channels past the file's, and the missing-color contract are the plain
// texture lookup's.
bool
texturesystem_environment(TextureSystem& texturesystem,
                          const rust::Str filename,
                          const TextureLookupOptions& options,
                          rust::Slice<const float> missing_color, float r_x,
                          float r_y, float r_z, float drdx_x, float drdx_y,
                          float drdx_z, float drdy_x, float drdy_y,
                          float drdy_z, rust::Slice<float> result,
                          rust::String& error);

// Whether the name is a UDIM pattern such as "tex.<UDIM>.exr".
bool
texturesystem_is_udim(TextureSystem& texturesystem, const rust::Str filename);

// The concrete tile file a UDIM pattern and texture coordinates refer to,
// or an empty string when that tile is unpopulated. The TextureHandle the
// resolution produces never crosses the bridge; it is turned back into a
// filename here.
rust::String
texturesystem_resolve_udim(TextureSystem& texturesystem,
                           const rust::Str pattern, float s, float t);

// Every concrete file of a UDIM set, indexed as utile + vtile * nutiles,
// with empty strings for unpopulated tiles.
void
texturesystem_inventory_udim(TextureSystem& texturesystem,
                             const rust::Str pattern,
                             rust::Vec<rust::String>& filenames, int& nutiles,
                             int& nvtiles);

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
