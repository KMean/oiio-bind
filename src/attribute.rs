use crate::sys::{self, typedesc::BaseType};

/// A metadata value attached to an [`ImageSpec`](crate::ImageSpec).
///
/// OpenImageIO metadata is dynamically typed. The three variants this crate
/// models directly cover the overwhelming majority of file metadata and can be
/// written back out. Everything else is preserved for inspection as
/// [`AttributeValue::Other`], holding OpenImageIO's own type name and string
/// rendering of the value.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum AttributeValue {
    Int(i32),
    Float(f32),
    String(String),
    /// Several strings, such as OpenEXR's `multiView`.
    Strings(Vec<String>),
    /// Any other OpenImageIO type, carried verbatim.
    ///
    /// The stored bytes are the value exactly as OpenImageIO holds it, so an
    /// attribute this crate does not model — `float2`, `uint16`, `timecode`,
    /// an ICC profile — still survives being read from one image and written
    /// to another. `value` is only for display, and rounds.
    Other {
        /// The OpenImageIO type name, such as `"float2"` or `"uint8[3144]"`.
        type_name: String,
        /// The value as OpenImageIO renders it for display.
        value: String,
        /// The value as OpenImageIO stores it.
        bytes: Vec<u8>,
    },
}

impl AttributeValue {
    /// The integer value, when this attribute holds one.
    pub fn as_int(&self) -> Option<i32> {
        match self {
            Self::Int(value) => Some(*value),
            _ => None,
        }
    }

    /// The float value, when this attribute holds one.
    pub fn as_float(&self) -> Option<f32> {
        match self {
            Self::Float(value) => Some(*value),
            _ => None,
        }
    }

    /// The string value, when this attribute holds one.
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Self::String(value) => Some(value),
            _ => None,
        }
    }

    /// Whether this crate can write the attribute back to a file.
    ///
    /// Every variant can, including [`AttributeValue::Other`], which carries
    /// the value's original bytes rather than only its printed form. The one
    /// exception is an `Other` whose bytes did not come from OpenImageIO,
    /// which is only possible if it was built by hand.
    pub fn is_writable(&self) -> bool {
        match self {
            Self::Other { bytes, .. } => !bytes.is_empty(),
            _ => true,
        }
    }

    pub(crate) fn read(spec: &sys::imageio::ImageSpec, name: &str) -> Self {
        let type_desc = sys::imageio::imagespec_attribute_type(spec, name);
        let scalar = type_desc.arraylen == 0
            && type_desc.aggregate == sys::typedesc::Aggregate::Scalar as u8;

        if scalar && type_desc.basetype == BaseType::Int32 as u8 {
            return Self::Int(sys::imageio::imagespec_get_int_attribute(spec, name, 0));
        }
        if scalar && type_desc.basetype == BaseType::Float32 as u8 {
            return Self::Float(sys::imageio::imagespec_get_float_attribute(spec, name, 0.0));
        }
        if type_desc.basetype == BaseType::String as u8 {
            if scalar {
                return Self::String(sys::imageio::imagespec_get_string_attribute(spec, name, ""));
            }
            // A string array holds pointers, not characters, so it is read as
            // strings rather than as bytes.
            return Self::Strings(sys::imageio::imagespec_attribute_strings(spec, name));
        }

        Self::Other {
            type_name: sys::typedesc::typedesc_to_str(&type_desc).to_owned(),
            value: sys::imageio::imagespec_attribute_to_string(spec, name),
            bytes: sys::imageio::imagespec_attribute_bytes(spec, name),
        }
    }

    pub(crate) fn write(&self, mut spec: std::pin::Pin<&mut sys::imageio::ImageSpec>, name: &str) {
        match self {
            Self::Int(value) => sys::imageio::imagespec_attribute_int(spec, name, *value),
            Self::Float(value) => sys::imageio::imagespec_attribute_float(spec, name, *value),
            Self::String(value) => sys::imageio::imagespec_attribute_string(spec, name, value),
            Self::Strings(values) => {
                let type_name = format!("string[{}]", values.len());
                sys::imageio::imagespec_attribute_set_strings(
                    spec.as_mut(),
                    name,
                    &type_name,
                    values,
                );
            }
            Self::Other {
                type_name, bytes, ..
            } => {
                // Re-emitted from the stored bytes, so nothing is lost to the
                // rounding in the printed form. A mismatched length is
                // refused by the shim rather than read past.
                sys::imageio::imagespec_attribute_set_bytes(spec.as_mut(), name, type_name, bytes);
            }
        }
    }
}

impl From<i32> for AttributeValue {
    fn from(value: i32) -> Self {
        Self::Int(value)
    }
}

impl From<f32> for AttributeValue {
    fn from(value: f32) -> Self {
        Self::Float(value)
    }
}

impl From<&str> for AttributeValue {
    fn from(value: &str) -> Self {
        Self::String(value.to_owned())
    }
}

impl From<String> for AttributeValue {
    fn from(value: String) -> Self {
        Self::String(value)
    }
}

impl std::fmt::Display for AttributeValue {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Int(value) => write!(formatter, "{value}"),
            Self::Float(value) => write!(formatter, "{value}"),
            Self::String(value) => formatter.write_str(value),
            Self::Strings(values) => formatter.write_str(&values.join(", ")),
            Self::Other { value, .. } => formatter.write_str(value),
        }
    }
}
