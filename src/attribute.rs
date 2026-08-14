use crate::sys::{self, typedesc::BaseType};
use crate::{Error, Result};

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
    /// A single 32-bit signed integer.
    Int(i32),
    /// A single 32-bit float.
    Float(f32),
    /// A single string.
    String(String),
    /// Several strings, such as OpenEXR's `multiView`.
    Strings(Vec<String>),
    /// Any other OpenImageIO type, carried verbatim.
    ///
    /// The stored bytes are the value exactly as OpenImageIO holds it, so an
    /// attribute this crate does not model — `float2`, `uint16`, `timecode`,
    /// an ICC profile — still survives being read from one image and written
    /// to another. `value` is only for display, and rounds.
    ///
    /// A hand-built `Other` is only writable if its bytes match the size its
    /// `type_name` describes and the type is one that means something outside
    /// this process; see [`AttributeValue::is_writable`]. One that is not
    /// makes the write fail rather than vanish. Everything read from a real
    /// file satisfies this, so a round trip never trips it.
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
    /// The scalar variants always can. [`AttributeValue::Strings`] can when
    /// it holds at least one string — an empty array has no OpenImageIO type
    /// to be written as, and its write fails. An [`AttributeValue::Other`],
    /// which carries the value's original bytes rather than only its printed
    /// form, is writable when its type name parses, its length is exactly
    /// what that type measures, and the type is one that means anything
    /// outside this process: a string is carried by the `String` variant
    /// instead, and a pointer or a hashed string is a raw process address.
    /// Anything read out of a real file satisfies all of that; a hand-built
    /// value may not.
    pub fn is_writable(&self) -> bool {
        match self {
            // An empty array would be "string[0]", which parses to a scalar
            // string; the write arm refuses it, so this must say so too.
            Self::Strings(values) => !values.is_empty(),
            Self::Other {
                type_name, bytes, ..
            } => {
                !bytes.is_empty()
                    && sys::imageio::attribute_bytes_are_writable(type_name, bytes.len())
            }
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

    pub(crate) fn write(
        &self,
        mut spec: std::pin::Pin<&mut sys::imageio::ImageSpec>,
        name: &str,
    ) -> Result<()> {
        match self {
            Self::Int(value) => sys::imageio::imagespec_attribute_int(spec, name, *value),
            Self::Float(value) => sys::imageio::imagespec_attribute_float(spec, name, *value),
            Self::String(value) => sys::imageio::imagespec_attribute_string(spec, name, value),
            Self::Strings(values) => {
                let type_name = format!("string[{}]", values.len());
                // The shim reports whether it stored the array, and an empty
                // one it cannot: "string[0]" parses to a scalar string whose
                // element count is one, not zero, so the length check fails.
                // Discarding the bool, as the Other arm used to, would drop the
                // attribute with nothing said.
                if !sys::imageio::imagespec_attribute_set_strings(
                    spec.as_mut(),
                    name,
                    &type_name,
                    values,
                ) {
                    return Err(Error::InvalidImageSpec(format!(
                        "attribute {name:?} could not be written as {type_name:?}; \
                         a string attribute needs at least one value"
                    )));
                }
            }
            Self::Other {
                type_name, bytes, ..
            } => {
                // Re-emitted from the stored bytes, so nothing is lost to the
                // rounding in the printed form. The shim refuses a payload
                // that does not measure what the type says, an array with no
                // concrete length, and types that carry a process address.
                // That refusal used to be discarded here, which left the
                // attribute absent from the file with nothing said about it.
                if !sys::imageio::imagespec_attribute_set_bytes(
                    spec.as_mut(),
                    name,
                    type_name,
                    bytes,
                ) {
                    return Err(Error::InvalidImageSpec(format!(
                        "attribute {name:?} declares type {type_name:?} and carries {} bytes, \
                         which that type cannot hold",
                        bytes.len()
                    )));
                }
            }
        }
        Ok(())
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
