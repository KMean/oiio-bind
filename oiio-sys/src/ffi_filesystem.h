#pragma once

#include <OpenImageIO/filesystem.h>
#include <rust/cxx.h>
#include <cstdint>
#include <memory>

namespace oiio {
using IOProxy = OIIO::Filesystem::IOProxy;

// A read proxy over caller-owned memory. The buffer is borrowed, not copied,
// so it must outlive the proxy.
std::unique_ptr<IOProxy>
ioproxy_memreader_new(rust::Slice<const uint8_t> data);

// A write proxy that owns the buffer it fills.
std::unique_ptr<IOProxy>
ioproxy_vecoutput_new();

// Copy out what a write proxy has accumulated. Empty for any other proxy kind.
rust::Vec<uint8_t>
ioproxy_vecoutput_bytes(const IOProxy& proxy);

rust::Str
ioproxy_proxytype(const IOProxy& proxy);

uint64_t
ioproxy_size(const IOProxy& proxy);

void
ioproxy_close(IOProxy& proxy);

}  // namespace oiio
