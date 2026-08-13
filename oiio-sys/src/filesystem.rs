pub use ffi::*;

// As in the other bridge modules: the safety contracts are documented on each
// declaration, but cxx does not carry those docs onto the signatures it
// generates, which is what the lint inspects.
#[allow(clippy::missing_safety_doc)]
#[cxx::bridge(namespace = oiio)]
mod ffi {
    unsafe extern "C++" {
        include!("oiio-sys/src/ffi_filesystem.h");

        pub type IOProxy;

        /// Create a read proxy over caller-owned memory.
        ///
        /// # Safety
        ///
        /// The proxy borrows `data` rather than copying it, so the buffer must
        /// stay alive, at a fixed address, and unmodified for as long as the
        /// returned proxy and anything reading through it.
        pub unsafe fn ioproxy_memreader_new(data: &[u8]) -> UniquePtr<IOProxy>;
        pub fn ioproxy_vecoutput_new() -> UniquePtr<IOProxy>;
        pub fn ioproxy_vecoutput_bytes(proxy: &IOProxy) -> Vec<u8>;
        pub fn ioproxy_proxytype(proxy: &IOProxy) -> &str;
        pub fn ioproxy_size(proxy: &IOProxy) -> u64;
        pub fn ioproxy_close(proxy: Pin<&mut IOProxy>);
    }
}
