//! Colour spaces, and the configuration that names them.
//!
//! Which names are valid depends on the active OpenColorIO configuration:
//! whatever `$OCIO` points at, or OpenImageIO's built-in one when that is
//! unset. Rather than guess, ask:
//!
//! ```no_run
//! use oiio::ColorConfig;
//!
//! # fn main() -> oiio::Result<()> {
//! let config = ColorConfig::new()?;
//! println!("configuration: {}", config.name());
//! for space in config.color_space_names() {
//!     println!("  {space}");
//! }
//! // Roles are stable names for whichever space a config uses for a purpose.
//! println!("scene_linear is {:?}", config.color_space_for_role("scene_linear"));
//! # Ok(())
//! # }
//! ```

use std::path::Path;

use crate::{path_to_utf8, sys, Error, Result};

/// The colour spaces available to [`color_convert`](crate::algo::color_convert).
pub struct ColorConfig {
    inner: cxx::UniquePtr<sys::color::ColorConfig>,
}

impl ColorConfig {
    /// The active configuration: `$OCIO`, or OpenImageIO's built-in one.
    pub fn new() -> Result<Self> {
        Self::from_inner(sys::color::colorconfig_default())
    }

    /// A configuration read from a file.
    pub fn from_path(config_path: &Path) -> Result<Self> {
        let filename = path_to_utf8(config_path)?;
        Self::from_inner(sys::color::colorconfig_from_file(filename))
    }

    /// Whether this OpenImageIO was built with OpenColorIO at all.
    ///
    /// Without it only a handful of built-in spaces exist, so a conversion
    /// naming anything else will fail.
    pub fn supports_opencolorio() -> bool {
        sys::color::colorconfig_supports_opencolorio()
    }

    /// The configuration's name, which is its file path when it came from one.
    pub fn name(&self) -> String {
        sys::color::colorconfig_name(self.inner())
    }

    /// Every colour space this configuration defines.
    pub fn color_space_names(&self) -> Vec<String> {
        sys::color::colorconfig_color_space_names(self.inner())
    }

    /// Every role this configuration defines.
    ///
    /// A role is a stable name — `"scene_linear"`, `"default"`, `"data"` —
    /// for whichever space a configuration uses for that purpose, so code can
    /// refer to it without knowing the configuration.
    pub fn role_names(&self) -> Vec<String> {
        sys::color::colorconfig_role_names(self.inner())
    }

    /// The colour space a role resolves to, if the role is defined.
    pub fn color_space_for_role(&self, role: &str) -> Option<String> {
        let name = sys::color::colorconfig_color_space_for_role(self.inner(), role);
        (!name.is_empty()).then_some(name)
    }

    /// Whether a name is one this configuration knows.
    pub fn has_color_space(&self, name: &str) -> bool {
        self.color_space_names().iter().any(|space| space == name)
    }

    fn from_inner(inner: cxx::UniquePtr<sys::color::ColorConfig>) -> Result<Self> {
        if inner.is_null() {
            return Err(Error::operation(
                "open colour configuration",
                "OpenImageIO returned no configuration".to_owned(),
            ));
        }
        let config = Self { inner };
        // A configuration that failed to load still hands back an object, so
        // ask it whether anything went wrong.
        let error = sys::color::colorconfig_geterror(config.inner());
        if !error.is_empty() {
            return Err(Error::operation("open colour configuration", error));
        }
        Ok(config)
    }

    fn inner(&self) -> &sys::color::ColorConfig {
        self.inner
            .as_ref()
            .expect("ColorConfig invariant violated: null native pointer")
    }
}

impl std::fmt::Debug for ColorConfig {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ColorConfig")
            .field("name", &self.name())
            .field("color_spaces", &self.color_space_names().len())
            .field("roles", &self.role_names().len())
            .finish()
    }
}
