#pragma once

#include <OpenImageIO/deepdata.h>
#include <OpenImageIO/imageio.h>
#include <OpenImageIO/typedesc.h>
#include <memory>
#include <rust/cxx.h>

namespace oiio {
using DeepData  = OIIO::DeepData;
using ImageSpec = OIIO::ImageSpec;
using TypeDesc  = OIIO::TypeDesc;

std::unique_ptr<DeepData>
deepdata_default();

std::unique_ptr<DeepData>
deepdata_new_from_spec(const ImageSpec& spec);

std::unique_ptr<DeepData>
deepdata_clone(const DeepData& src);

std::unique_ptr<DeepData>
deepdata_clone_with_channeltypes(const DeepData& src,
                                 rust::Slice<const TypeDesc> channeltypes);

void
deepdata_clear(DeepData& deepdata);

void
deepdata_free(DeepData& deepdata);

void
deepdata_init(DeepData& deepdata, int64_t npix, int nchan,
              rust::Slice<const TypeDesc> channeltypes,
              rust::Slice<const rust::String> channelnames);

bool
deepdata_init_from_spec(DeepData& deepdata, const ImageSpec& spec,
                        rust::String& error);

bool
deepdata_initialized(const DeepData& deepdata);

bool
deepdata_allocated(const DeepData& deepdata);

int64_t
deepdata_pixels(const DeepData& deepdata);

int
deepdata_channels(const DeepData& deepdata);

int
deepdata_z_channel(const DeepData& deepdata);

int
deepdata_z_back_channel(const DeepData& deepdata);

int
deepdata_a_channel(const DeepData& deepdata);

int
deepdata_ar_channel(const DeepData& deepdata);

int
deepdata_ag_channel(const DeepData& deepdata);

int
deepdata_ab_channel(const DeepData& deepdata);

rust::String
deepdata_channelname(const DeepData& deepdata, int c);

TypeDesc
deepdata_channeltype(const DeepData& deepdata, int c);

size_t
deepdata_channelsize(const DeepData& deepdata, int c);

size_t
deepdata_samplesize(const DeepData& deepdata);

bool
deepdata_same_channeltypes(const DeepData& deepdata, const DeepData& other);

int
deepdata_samples(const DeepData& deepdata, int64_t pixel);

bool
deepdata_set_samples(DeepData& deepdata, int64_t pixel, int samps,
                     rust::String& error);

bool
deepdata_set_all_samples(DeepData& deepdata,
                         rust::Slice<const unsigned int> samples,
                         rust::String& error);

bool
deepdata_set_capacity(DeepData& deepdata, int64_t pixel, int samps,
                      rust::String& error);

int
deepdata_capacity(const DeepData& deepdata, int64_t pixel);

bool
deepdata_insert_samples(DeepData& deepdata, int64_t pixel, int samplepos, int n,
                        rust::String& error);

void
deepdata_erase_samples(DeepData& deepdata, int64_t pixel, int samplepos, int n);

float
deepdata_deep_value(const DeepData& deepdata, int64_t pixel, int channel,
                    int sample);

uint32_t
deepdata_deep_value_uint(const DeepData& deepdata, int64_t pixel, int channel,
                         int sample);

bool
deepdata_set_deep_value(DeepData& deepdata, int64_t pixel, int channel,
                        int sample, float value, rust::String& error);

bool
deepdata_set_deep_value_uint(DeepData& deepdata, int64_t pixel, int channel,
                             int sample, uint32_t value, rust::String& error);

uint8_t*
deepdata_mut_data_ptr(DeepData& deepdata, int64_t pixel, int channel,
                      int sample);

const uint8_t*
deepdata_data_ptr(const DeepData& deepdata, int64_t pixel, int channel,
                  int sample);

rust::Slice<const TypeDesc>
deepdata_all_channeltypes(const DeepData& deepdata);

rust::Slice<const unsigned int>
deepdata_all_samples(const DeepData& deepdata);

rust ::Slice<const char>
deepdata_all_data(const DeepData& deepdata);

size_t
deepdata_get_pointers(const DeepData& deepdata, rust::Slice<uint8_t*> pointers);

bool
deepdata_copy_deep_sample(DeepData& deepdata, int64_t pixel, int sample,
                          const DeepData& src, int64_t srcpixel, int srcsample);

bool
deepdata_copy_deep_pixel(DeepData& deepdata, int64_t pixel, const DeepData& src,
                         int64_t srcpixel);

bool
deepdata_split(DeepData& deepdata, int64_t pixel, float depth);

void
deepdata_sort(DeepData& deepdata, int64_t pixel);

void
deepdata_merge_overlaps(DeepData& deepdata, int64_t pixel);

void
deepdata_merge_deep_pixels(DeepData& deepdata, int64_t pixel,
                           const DeepData& src, int srcpixel);

float
deepdata_opaque_z(const DeepData& deepdata, int64_t pixel);

void
deepdata_occlusion_cull(DeepData& deepdata, int64_t pixel);
}  // namespace oiio
