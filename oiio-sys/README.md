# oiio-sys

[oiio-sys](https://crates.io/crates/oiio-sys) is a cxx-based low-level
sys binding to the C++
[OpenImageIO](https://github.com/AcademySoftwareFoundation/OpenImageIO) API.

`oiio-sys` provides a low-level API that is used as the foundation for the
high-level [oiio](https://crates.io/crates/oiio) crate.

## Building

The crate currently targets OpenImageIO 3.1.x and requires an installed
OpenImageIO development package.

Discovery checks explicit configuration first. Set `OIIO_ROOT` for an install
prefix containing `include/OpenImageIO` and `lib`, or set both
`OIIO_INCLUDE_DIR` and `OIIO_LIBRARY_DIR` when those directories are elsewhere.
On Windows, `OIIO_DLL_DIR` can name a separate runtime DLL directory.

Windows MSVC builds otherwise use vcpkg. Set `VCPKG_ROOT` and install
`openimageio` for the target triplet. The default is the dynamic
`x64-windows`, `x86-windows`, or `arm64-windows` triplet matching the Rust
target. `OIIO_VCPKG_TRIPLET` overrides it; `VCPKG_DEFAULT_TRIPLET` is used when
the OIIO-specific override is absent.

Other targets use `pkg-config` and look for the `OpenImageIO` package. Set
`PKG_CONFIG_PATH` when its `.pc` file is outside the standard search paths.

For Windows executables run outside Cargo, ensure `OpenImageIO.dll`,
`OpenImageIO_Util.dll`, and their dependency DLLs are beside the executable or
on `PATH`. vcpkg discovery stages the DLLs for Cargo commands. Explicit installs
stage them from `OIIO_DLL_DIR`, or from `OIIO_ROOT/bin` when present.

If discovery fails, check that the selected vcpkg triplet or explicit library
directory matches the Rust target architecture and MSVC ABI.


## Links

- [source repository](https://github.com/vfx-rs/oiio-bind)
- [oiio-sys on crates.io](https://crates.io/crates/oiio-sys/latest)
- [oiio-sys documentation](https://docs.rs/crate/oiio-sys/latest)
- [OpenImageIO C++ documentation](https://openimageio.readthedocs.io/en/latest/)
