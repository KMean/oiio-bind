#include "ffi_filesystem.h"

#include <cstring>

namespace oiio {

std::unique_ptr<IOProxy>
ioproxy_memreader_new(rust::Slice<const uint8_t> data)
{
    // IOMemReader borrows the buffer; the caller keeps it alive.
    return std::make_unique<OIIO::Filesystem::IOMemReader>(data.data(),
                                                           data.size());
}

std::unique_ptr<IOProxy>
ioproxy_vecoutput_new()
{
    // The default constructor gives the proxy its own buffer.
    return std::make_unique<OIIO::Filesystem::IOVecOutput>();
}

rust::Vec<uint8_t>
ioproxy_vecoutput_bytes(const IOProxy& proxy)
{
    rust::Vec<uint8_t> bytes;
    const auto* output
        = dynamic_cast<const OIIO::Filesystem::IOVecOutput*>(&proxy);
    if (output == nullptr)
        return bytes;

    const std::vector<unsigned char>& buffer = output->buffer();
    bytes.reserve(buffer.size());
    for (unsigned char byte : buffer)
        bytes.push_back(byte);
    return bytes;
}

rust::Str
ioproxy_proxytype(const IOProxy& proxy)
{
    const char* type = proxy.proxytype();
    return type ? rust::Str(type) : rust::Str("");
}

uint64_t
ioproxy_size(const IOProxy& proxy)
{
    return static_cast<uint64_t>(proxy.size());
}

void
ioproxy_close(IOProxy& proxy)
{
    proxy.close();
}

}  // namespace oiio
