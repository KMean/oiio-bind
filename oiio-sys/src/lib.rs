//! Low-level CXX bindings to OpenImageIO.
//!
//! This crate is the compatibility layer used by the safe `oiio` crate. Its
//! public surface is intentionally close to C++, includes partially modernized
//! legacy APIs, and exposes raw pointers and caller-enforced safety contracts.
//! Prefer `oiio` unless implementing or extending the high-level bindings.

pub mod color;
pub mod deepdata;
pub mod filesystem;
pub mod imagebuf;
pub mod imagebufalgo;
pub mod imagecache;
pub mod imageio;
pub mod texture;
pub mod typedesc;
