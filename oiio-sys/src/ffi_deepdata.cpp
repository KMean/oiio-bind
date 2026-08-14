#include "ffi_deepdata.h"
#include <OpenImageIO/span.h>

namespace oiio {

// DeepData allocates its sample storage lazily: set_samples records the
// count, and the first write (or a capacity change) does the real
// m_data.resize with no try/catch of its own. Every shim below that can
// reach that resize is wrapped noexcept by cxx, so an escaping bad_alloc
// would be std::terminate -- the same hazard deepdata_init_from_spec already
// guards for the pixel axis. This helper catches and reports for the
// sample axis.
template<typename Mutation>
static bool
guarded_deep_mutation(rust::String& error, Mutation&& mutation)
{
    error = rust::String();
    try {
        mutation();
        return true;
    } catch (const std::exception& exception) {
        const std::string recorded = exception.what();
        error = rust::String::lossy(
            recorded.empty() ? "OpenImageIO could not allocate the deep samples"
                             : recorded.c_str());
        return false;
    }
}

std::unique_ptr<DeepData>
deepdata_default()
{
    return std::make_unique<DeepData>();
}

std::unique_ptr<DeepData>
deepdata_new_from_spec(const ImageSpec& spec)
{
    return std::make_unique<DeepData>(spec);
}

std::unique_ptr<DeepData>
deepdata_clone(const DeepData& src)
{
    return std::make_unique<DeepData>(src);
}

std::unique_ptr<DeepData>
deepdata_clone_with_channeltypes(const DeepData& src,
                                 rust::Slice<const TypeDesc> channeltypes)
{
    OIIO::cspan<TypeDesc> c_channeltypes(channeltypes.data(),
                                         channeltypes.size());
    return std::make_unique<DeepData>(src, c_channeltypes);
}

void
deepdata_clear(DeepData& deepdata)
{
    deepdata.clear();
}

void
deepdata_free(DeepData& deepdata)
{
    deepdata.free();
}

void
deepdata_init(DeepData& deepdata, int64_t npix, int nchan,
              rust::Slice<const TypeDesc> channeltypes,
              rust::Slice<const rust::String> channelnames)
{
    OIIO::cspan<TypeDesc> c_channeltypes(channeltypes.data(),
                                         channeltypes.size());
    std::vector<std::string> channelnames_vec;
    channelnames_vec.reserve(channelnames.size());

    for (auto& s : channelnames) {
        channelnames_vec.push_back(std::string(s));
    }

    OIIO::cspan<std::string> c_channelnames(channelnames_vec.data(),
                                            channelnames_vec.size());
    deepdata.init(npix, nchan, c_channeltypes, c_channelnames);
}

bool
deepdata_init_from_spec(DeepData& deepdata, const ImageSpec& spec,
                        rust::String& error)
{
    // DeepData::init resizes three per-pixel vectors with no try/catch, so a
    // deep spec whose pixel count is within the i32 cap but still too large for
    // the machine throws bad_alloc straight out. This shim is noexcept, so that
    // would be std::terminate. Catch it and report, as the flat path does.
    error = rust::String();
    try {
        deepdata.init(spec);
        return true;
    } catch (const std::exception& exception) {
        const std::string recorded = exception.what();
        error = rust::String::lossy(
            recorded.empty() ? "OpenImageIO could not allocate the deep image"
                             : recorded.c_str());
        return false;
    }
}

bool
deepdata_initialized(const DeepData& deepdata)
{
    return deepdata.initialized();
}

bool
deepdata_allocated(const DeepData& deepdata)
{
    return deepdata.allocated();
}

int64_t
deepdata_pixels(const DeepData& deepdata)
{
    return deepdata.pixels();
}

int
deepdata_channels(const DeepData& deepdata)
{
    return deepdata.channels();
}

int
deepdata_z_channel(const DeepData& deepdata)
{
    return deepdata.Z_channel();
}

int
deepdata_z_back_channel(const DeepData& deepdata)
{
    return deepdata.Zback_channel();
}

int
deepdata_a_channel(const DeepData& deepdata)
{
    return deepdata.A_channel();
}

int
deepdata_ar_channel(const DeepData& deepdata)
{
    return deepdata.AR_channel();
}

int
deepdata_ag_channel(const DeepData& deepdata)
{
    return deepdata.AG_channel();
}

int
deepdata_ab_channel(const DeepData& deepdata)
{
    return deepdata.AB_channel();
}

// channelname() returns a string_view over bytes copied verbatim from the
// file: OpenEXR checks channel names only for null-termination and length,
// never encoding. A borrowed rust::Str would validate UTF-8 in a throwing
// constructor inside a noexcept wrapper -- reading a deep file with one high
// byte in a channel name would be std::terminate -- and rust::Str has no
// lossy form, so the name is returned owned and lossy like the error
// strings. (An earlier version borrowed, which also risked pointing into a
// destroyed temporary via the std::string constructor.)
rust::String
deepdata_channelname(const DeepData& deepdata, int c)
{
    const OIIO::string_view name = deepdata.channelname(c);
    return rust::String::lossy(name.data(), name.size());
}

TypeDesc
deepdata_channeltype(const DeepData& deepdata, int c)
{
    return deepdata.channeltype(c);
}

size_t
deepdata_channelsize(const DeepData& deepdata, int c)
{
    return deepdata.channelsize(c);
}

size_t
deepdata_samplesize(const DeepData& deepdata)
{
    return deepdata.samplesize();
}

bool
deepdata_same_channeltypes(const DeepData& deepdata, const DeepData& other)
{
    return deepdata.same_channeltypes(other);
}

int
deepdata_samples(const DeepData& deepdata, int64_t pixel)
{
    return deepdata.samples(pixel);
}

bool
deepdata_set_samples(DeepData& deepdata, int64_t pixel, int samps,
                     rust::String& error)
{
    // On an already-allocated DeepData this becomes an insert, whose
    // capacity growth resizes the sample storage; see the helper.
    return guarded_deep_mutation(error,
                                 [&] { deepdata.set_samples(pixel, samps); });
}

bool
deepdata_set_all_samples(DeepData& deepdata,
                         rust::Slice<const unsigned int> samples,
                         rust::String& error)
{
    return guarded_deep_mutation(error, [&] {
        deepdata.set_all_samples(
            OIIO::cspan<unsigned int>(samples.data(), samples.size()));
    });
}

bool
deepdata_set_capacity(DeepData& deepdata, int64_t pixel, int samps,
                      rust::String& error)
{
    return guarded_deep_mutation(error,
                                 [&] { deepdata.set_capacity(pixel, samps); });
}

int
deepdata_capacity(const DeepData& deepdata, int64_t pixel)
{
    return deepdata.capacity(pixel);
}

bool
deepdata_insert_samples(DeepData& deepdata, int64_t pixel, int samplepos, int n,
                        rust::String& error)
{
    // Unlike samples()/capacity()/data_ptr()/deep_value(), which range-check
    // the pixel index and return 0/NULL, insert_samples and erase_samples index
    // m_nsamples[pixel] and m_capacity[pixel] with no check of their own, so an
    // out-of-range pixel is a heap read and write past those vectors. Bound it
    // here, which keeps these two callable safely like their guarded siblings.
    error = rust::String();
    if (pixel < 0 || pixel >= deepdata.pixels())
        return true;
    // Growth can reach set_capacity's storage resize; see the helper.
    return guarded_deep_mutation(
        error, [&] { deepdata.insert_samples(pixel, samplepos, n); });
}

void
deepdata_erase_samples(DeepData& deepdata, int64_t pixel, int samplepos, int n)
{
    if (pixel < 0 || pixel >= deepdata.pixels())
        return;
    deepdata.erase_samples(pixel, samplepos, n);
}

float
deepdata_deep_value(const DeepData& deepdata, int64_t pixel, int channel,
                    int sample)
{
    return deepdata.deep_value(pixel, channel, sample);
}

uint32_t
deepdata_deep_value_uint(const DeepData& deepdata, int64_t pixel, int channel,
                         int sample)
{
    return deepdata.deep_value_uint(pixel, channel, sample);
}

bool
deepdata_set_deep_value(DeepData& deepdata, int64_t pixel, int channel,
                        int sample, float value, rust::String& error)
{
    // The first write is what performs the deferred sample allocation.
    return guarded_deep_mutation(error, [&] {
        deepdata.set_deep_value(pixel, channel, sample, value);
    });
}

bool
deepdata_set_deep_value_uint(DeepData& deepdata, int64_t pixel, int channel,
                             int sample, uint32_t value, rust::String& error)
{
    return guarded_deep_mutation(error, [&] {
        deepdata.set_deep_value(pixel, channel, sample, value);
    });
}

uint8_t*
deepdata_mut_data_ptr(DeepData& deepdata, int64_t pixel, int channel,
                      int sample)
{
    // The mutable data_ptr performs the deferred sample allocation, so it can
    // throw bad_alloc; null already means "nothing at this address", and a
    // failed allocation is exactly that.
    try {
        return (uint8_t*)deepdata.data_ptr(pixel, channel, sample);
    } catch (const std::exception&) {
        return nullptr;
    }
}

const uint8_t*
deepdata_data_ptr(const DeepData& deepdata, int64_t pixel, int channel,
                  int sample)
{
    return (uint8_t*)deepdata.data_ptr(pixel, channel, sample);
}

rust::Slice<const TypeDesc>
deepdata_all_channeltypes(const DeepData& deepdata)
{
    OIIO::cspan<TypeDesc> c_all_channeltypes = deepdata.all_channeltypes();
    return rust::Slice<const TypeDesc>(c_all_channeltypes.data(),
                                       c_all_channeltypes.size());
}

rust::Slice<const unsigned int>
deepdata_all_samples(const DeepData& deepdata)
{
    OIIO::cspan<unsigned int> c_all_samples = deepdata.all_samples();
    return rust::Slice<const unsigned int>(c_all_samples.data(),
                                           c_all_samples.size());
}

rust ::Slice<const char>
deepdata_all_data(const DeepData& deepdata)
{
    // all_data also performs the deferred allocation despite being const;
    // an empty slice is the honest answer when it cannot be made.
    try {
        OIIO::cspan<char> c_all_data = deepdata.all_data();
        return rust::Slice<const char>(c_all_data.data(), c_all_data.size());
    } catch (const std::exception&) {
        return rust::Slice<const char>();
    }
}

size_t
deepdata_get_pointers(const DeepData& deepdata, rust::Slice<uint8_t*> pointers)
{
    // DeepData::get_pointers resizes the vector it is given to
    // pixels() * channels() and fills it, so the vector is an out parameter
    // rather than an in-out one. Copying the caller's slice in was pointless
    // and, worse, nothing was ever copied back: the function looked like an
    // out-parameter fill and was a guaranteed no-op.
    //
    // It also performs the deferred sample allocation, so it can throw; zero
    // pointers is the honest answer then, and distinguishable, since a deep
    // image with pixels always produces at least one entry.
    std::vector<void*> c_pointers;
    try {
        deepdata.get_pointers(c_pointers);
    } catch (const std::exception&) {
        return 0;
    }

    const size_t available = c_pointers.size();
    const size_t copied    = std::min(available, pointers.size());
    for (size_t i = 0; i < copied; ++i)
        pointers[i] = static_cast<uint8_t*>(c_pointers[i]);
    // The number OpenImageIO produced, so a caller given a short slice can
    // tell it was truncated rather than assume it saw everything.
    return available;
}

bool
deepdata_copy_deep_sample(DeepData& deepdata, int64_t pixel, int sample,
                          const DeepData& src, int64_t srcpixel, int srcsample)
{
    // Copying grows the destination pixel, which can reach the storage
    // resize; false already means the copy did not happen.
    try {
        return deepdata.copy_deep_sample(pixel, sample, src, srcpixel,
                                         srcsample);
    } catch (const std::exception&) {
        return false;
    }
}

bool
deepdata_copy_deep_pixel(DeepData& deepdata, int64_t pixel, const DeepData& src,
                         int64_t srcpixel)
{
    // Sizes the destination pixel with set_samples, which can reach the
    // storage resize; false already means the copy did not happen.
    try {
        return deepdata.copy_deep_pixel(pixel, src, srcpixel);
    } catch (const std::exception&) {
        return false;
    }
}

bool
deepdata_split(DeepData& deepdata, int64_t pixel, float depth)
{
    // Splitting inserts samples, which can grow the pixel's capacity; false
    // already means nothing was split.
    try {
        return deepdata.split(pixel, depth);
    } catch (const std::exception&) {
        return false;
    }
}

void
deepdata_sort(DeepData& deepdata, int64_t pixel)
{
    // Sorting builds a per-pixel temporary, so under memory pressure it can
    // throw; the pixel is then simply left unsorted, which is the state the
    // caller already had.
    try {
        deepdata.sort(pixel);
    } catch (const std::exception&) {
    }
}

void
deepdata_merge_overlaps(DeepData& deepdata, int64_t pixel)
{
    deepdata.merge_overlaps(pixel);
}

void
deepdata_merge_deep_pixels(DeepData& deepdata, int64_t pixel,
                           const DeepData& src, int srcpixel)
{
    // Merging grows the destination pixel, which can reach the storage
    // resize; on failure the pixel keeps its previous samples.
    try {
        deepdata.merge_deep_pixels(pixel, src, srcpixel);
    } catch (const std::exception&) {
    }
}

float
deepdata_opaque_z(const DeepData& deepdata, int64_t pixel)
{
    return deepdata.opaque_z(pixel);
}

void
deepdata_occlusion_cull(DeepData& deepdata, int64_t pixel)
{
    deepdata.occlusion_cull(pixel);
}
}  // namespace oiio
