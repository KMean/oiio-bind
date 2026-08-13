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
    Other {
        /// The OpenImageIO type name, such as `"matrix44"` or `"float[3]"`.
        type_name: String,
        /// The value as OpenImageIO renders it for display.
        value: String,
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
    /// [`AttributeValue::Other`] values are readable but are dropped when a
    /// specification is handed to a writer, because their original
    /// OpenImageIO type is not reconstructed from the string rendering.
    pub fn is_writable(&self) -> bool {
        !matches!(self, Self::Other { .. })
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
        if scalar && type_desc.basetype == BaseType::String as u8 {
            return Self::String(sys::imageio::imagespec_get_string_attribute(spec, name, ""));
        }

        Self::Other {
            type_name: sys::typedesc::typedesc_to_str(&type_desc).to_owned(),
            value: sys::imageio::imagespec_attribute_to_string(spec, name),
        }
    }

    pub(crate) fn write(&self, spec: std::pin::Pin<&mut sys::imageio::ImageSpec>, name: &str) {
        match self {
            Self::Int(value) => sys::imageio::imagespec_attribute_int(spec, name, *value),
            Self::Float(value) => sys::imageio::imagespec_attribute_float(spec, name, *value),
            Self::String(value) => sys::imageio::imagespec_attribute_string(spec, name, value),
            // Deliberately not reconstructed; see `is_writable`.
            Self::Other { .. } => {}
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
            Self::Other { value, .. } => formatter.write_str(value),
        }
    }
}
