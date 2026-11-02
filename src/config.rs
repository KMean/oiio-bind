//! Process-wide OpenImageIO settings.
//!
//! Everything in this module changes OpenImageIO for the whole process, not
//! for one image or one cache. That makes it the right place to say how much
//! of the machine the library may use, and what it should refuse to read.
//!
//! Two situations need it. A library embedded in a host that owns the CPU
//! budget has to stop OpenImageIO spawning a thread per core:
//!
//! ```
//! # fn main() -> oiio::Result<()> {
//! oiio::config::set_threads(4)?;
//! # Ok(())
//! # }
//! ```
//!
//! And a service reading files it did not create wants a ceiling on what a
//! header can ask for, since the limits exist to turn a hostile file into a
//! refusal rather than an allocation:
//!
//! ```
//! # fn main() -> oiio::Result<()> {
//! use oiio::config::Limits;
//!
//! oiio::config::set_limits(
//!     &Limits::current().with_channels(64).with_image_size_mb(512),
//! )?;
//! # Ok(())
//! # }
//! ```
//!
//! OpenImageIO guards its own attribute table with a recursive mutex, so these
//! are safe to call from any thread at any time. They are still process-wide
//! mutable state: a library that sets them is deciding for the whole program,
//! including code it does not own, which is why a host application usually
//! wants to be the one calling them.
//!
//! [`set_attribute_int`], [`set_attribute_float`] and [`set_attribute_string`]
//! reach anything the typed functions do not, and [`attribute_int`],
//! [`attribute_float`] and [`attribute_string`] read it back. The full list of
//! names is in OpenImageIO's own documentation.

use crate::{sys, Error, Result};

/// An attribute OpenImageIO would accept but this crate will not set, because
/// turning it on would undermine a guarantee the crate makes elsewhere.
///
/// `imagebuf:use_imagecache` makes every file-backed [`ImageBuf`] a client of
/// OpenImageIO's process-wide shared `ImageCache`. Dropping such a buffer
/// invalidates that cache, and because [`ImageBuf`] is `Send` the drop can
/// land on any thread, with no borrow for the compiler to check it against.
/// This crate deliberately does not offer the shared cache at all for the
/// same reason; see the comment above [`ImageCache`]'s `Send` impl.
///
/// [`ImageBuf`]: crate::ImageBuf
/// [`ImageCache`]: crate::ImageCache
const REFUSED: &[(&str, &str)] = &[(
    "imagebuf:use_imagecache",
    "this would make every file-backed ImageBuf a client of OpenImageIO's \
     shared cache, whose invalidation on drop would then land on arbitrary \
     threads; this crate does not offer the shared cache for the same reason",
)];

fn check_allowed(name: &str) -> Result<()> {
    for (refused, reason) in REFUSED {
        if name == *refused {
            return Err(Error::operation(
                "set attribute",
                format!("{name} is not settable through this crate: {reason}"),
            ));
        }
    }
    Ok(())
}

fn set_int(operation: &'static str, name: &str, value: i32) -> Result<()> {
    check_allowed(name)?;
    if sys::imageio::attribute_int(name, value) {
        Ok(())
    } else {
        Err(Error::operation(
            operation,
            format!("OpenImageIO does not accept {value} for {name}"),
        ))
    }
}

/// How many threads OpenImageIO may use for its own parallel work.
///
/// Zero means one per hardware core, which is OpenImageIO's default and is
/// usually wrong inside a host that is already running its own pool. This does
/// not bound OpenEXR's threads; see [`set_exr_threads`].
pub fn set_threads(count: u32) -> Result<()> {
    set_int(
        "set threads",
        "threads",
        i32::try_from(count).unwrap_or(i32::MAX),
    )
}

/// How many threads OpenImageIO will actually use.
///
/// This is the resolved count, not the stored setting: after
/// `set_threads(0)` this reports the number of cores rather than zero,
/// because zero was a request for "one per core" and OpenImageIO answers with
/// what that came to.
pub fn threads() -> u32 {
    sys::imageio::get_int_attribute("threads", 0).max(0) as u32
}

/// How many threads OpenEXR may use, which it counts separately.
///
/// Zero means one per core. OpenEXR treats a negative value as "do not start a
/// pool at all", so this takes an `i32` where [`set_threads`] takes a `u32`.
pub fn set_exr_threads(count: i32) -> Result<()> {
    set_int("set exr threads", "exr_threads", count)
}

/// How many threads OpenEXR is currently allowed.
pub fn exr_threads() -> i32 {
    sys::imageio::get_int_attribute("exr_threads", 0)
}

/// What a file is allowed to ask for before OpenImageIO refuses to read it.
///
/// These are the ceilings that turn a malformed or hostile header into an
/// error rather than an allocation. OpenImageIO applies them while reading a
/// header, before any pixel memory is reserved.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub struct Limits {
    /// The most channels a file may declare. OpenImageIO's default is 1024.
    pub channels: u32,
    /// The largest image, in megabytes, a file may declare. OpenImageIO's
    /// default is 32768, which is to say 32 GB.
    pub image_size_mb: u32,
    /// The largest width or height a file may declare, or `None` where the
    /// linked OpenImageIO has no such limit.
    ///
    /// This one arrived in OpenImageIO 3.1.19 (upstream #5297). On anything
    /// older it reads back as `None`, and setting it is an error rather than
    /// a silent no-op, because a caller tightening a ceiling against hostile
    /// input should find out that the ceiling does not exist.
    pub resolution: Option<u32>,
}

impl Limits {
    /// The ceilings currently in force.
    ///
    /// This is the usual starting point, because tightening one ceiling
    /// should not mean restating the others:
    ///
    /// ```
    /// # fn main() -> oiio::Result<()> {
    /// use oiio::config::{self, Limits};
    ///
    /// config::set_limits(&Limits::current().with_channels(64))?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn current() -> Self {
        Self {
            channels: sys::imageio::get_int_attribute("limits:channels", 0).max(0) as u32,
            image_size_mb: sys::imageio::get_int_attribute("limits:imagesize_MB", 0).max(0) as u32,
            resolution: attribute_int("limits:resolution").map(|v| v.max(0) as u32),
        }
    }

    /// The same ceilings with a different channel count.
    #[must_use]
    pub fn with_channels(mut self, channels: u32) -> Self {
        self.channels = channels;
        self
    }

    /// The same ceilings with a different image size, in megabytes.
    #[must_use]
    pub fn with_image_size_mb(mut self, image_size_mb: u32) -> Self {
        self.image_size_mb = image_size_mb;
        self
    }

    /// The same ceilings with a different maximum width or height.
    ///
    /// Setting this on an OpenImageIO older than 3.1.19 is an error; see the
    /// field.
    #[must_use]
    pub fn with_resolution(mut self, resolution: u32) -> Self {
        self.resolution = Some(resolution);
        self
    }
}

/// Read the ceilings currently in force.
pub fn limits() -> Limits {
    Limits::current()
}

/// Set the ceilings a file may not exceed.
///
/// Each field is applied separately, so a failure can leave some of them
/// changed. Read them back with [`limits`] if that matters.
pub fn set_limits(limits: &Limits) -> Result<()> {
    let clamp = |value: u32| i32::try_from(value).unwrap_or(i32::MAX);
    set_int("set limits", "limits:channels", clamp(limits.channels))?;
    set_int(
        "set limits",
        "limits:imagesize_MB",
        clamp(limits.image_size_mb),
    )?;
    match limits.resolution {
        // `None` means leave it alone, which is also what reading it back
        // from an OpenImageIO that does not have it produces.
        None => Ok(()),
        Some(resolution) => set_int("set limits", "limits:resolution", clamp(resolution)),
    }
}

/// Whether readers should reject files that break the format's rules, rather
/// than reading what they can.
///
/// Off by default, because many real files are slightly malformed and still
/// readable. Worth turning on for input that is not trusted.
pub fn set_strict_input(strict: bool) -> Result<()> {
    set_int("set strict input", "imageinput:strict", i32::from(strict))
}

/// Whether readers are currently rejecting files that break the format's
/// rules.
pub fn strict_input() -> bool {
    sys::imageio::get_int_attribute("imageinput:strict", 0) != 0
}

/// Whether a file that no reader claims should be offered to every reader in
/// turn.
///
/// On by default. Turning it off means a file is only tried by readers that
/// recognise it, which is faster and narrows what untrusted input can reach.
pub fn set_try_all_readers(try_all: bool) -> Result<()> {
    set_int("set try all readers", "try_all_readers", i32::from(try_all))
}

/// Whether a file that no reader claims is currently offered to every reader.
pub fn try_all_readers() -> bool {
    sys::imageio::get_int_attribute("try_all_readers", 1) != 0
}

/// Where OpenImageIO looks for format plugins, as a list of directories joined
/// by the platform's path separator.
///
/// An embedded host that ships its own plugins sets this before the first
/// read, since the search runs once.
pub fn set_plugin_search_path(path: &str) -> Result<()> {
    set_attribute_string("plugin_searchpath", path)
}

/// Where OpenImageIO is currently looking for format plugins.
pub fn plugin_search_path() -> String {
    sys::imageio::get_string_attribute("plugin_searchpath", "")
}

/// Where OpenImageIO looks for fonts, used by text rendering.
pub fn set_font_search_path(path: &str) -> Result<()> {
    set_attribute_string("font_searchpath", path)
}

/// Where OpenImageIO is currently looking for fonts.
pub fn font_search_path() -> String {
    sys::imageio::get_string_attribute("font_searchpath", "")
}

/// Set any integer attribute by name.
///
/// For the names with a function of their own, prefer that function: it
/// documents the units and the default.
pub fn set_attribute_int(name: &str, value: i32) -> Result<()> {
    set_int("set attribute", name, value)
}

/// Set any floating-point attribute by name.
pub fn set_attribute_float(name: &str, value: f32) -> Result<()> {
    check_allowed(name)?;
    if sys::imageio::attribute_float(name, value) {
        Ok(())
    } else {
        Err(Error::operation(
            "set attribute",
            format!("OpenImageIO does not accept {value} for {name}"),
        ))
    }
}

/// Set any string attribute by name.
pub fn set_attribute_string(name: &str, value: &str) -> Result<()> {
    check_allowed(name)?;
    if sys::imageio::attribute_string(name, value) {
        Ok(())
    } else {
        Err(Error::operation(
            "set attribute",
            format!("OpenImageIO does not accept a string for {name}"),
        ))
    }
}

/// Read any integer attribute by name, or `None` if OpenImageIO has no such
/// integer attribute.
pub fn attribute_int(name: &str) -> Option<i32> {
    let mut value = 0;
    sys::imageio::getattribute_int(name, &mut value).then_some(value)
}

/// Read any floating-point attribute by name.
pub fn attribute_float(name: &str) -> Option<f32> {
    let mut value = 0.0;
    sys::imageio::getattribute_float(name, &mut value).then_some(value)
}

/// Read any string attribute by name.
pub fn attribute_string(name: &str) -> Option<String> {
    let mut value = String::new();
    sys::imageio::getattribute_string(name, &mut value).then_some(value)
}

/// The OpenImageIO this crate is linked against, as `(major, minor, patch)`.
///
/// Note that OpenImageIO's version integer carries no tweak component, so a
/// 3.1.19.0 and a 3.1.19.2 are indistinguishable here.
pub fn version() -> (u32, u32, u32) {
    let packed = sys::imageio::openimageio_version();
    let packed = if packed < 0 { 0 } else { packed as u32 };
    (packed / 10_000, (packed / 100) % 100, packed % 100)
}

/// Every file format OpenImageIO can read or write, by name.
pub fn format_names() -> Vec<String> {
    split_list(&sys::imageio::get_string_attribute("format_list", ""))
}

/// Every file extension OpenImageIO recognises, without the leading dot.
pub fn extension_names() -> Vec<String> {
    sys::imageio::get_extension_map()
        .into_iter()
        .flat_map(|item| item.value)
        .collect()
}

/// The third-party libraries OpenImageIO was built against, with versions.
pub fn library_versions() -> Vec<String> {
    split_list(&sys::imageio::get_string_attribute("library_list", ""))
}

fn split_list(packed: &str) -> Vec<String> {
    packed
        .split(',')
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .map(str::to_owned)
        .collect()
}
