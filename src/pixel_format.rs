use crate::sys::{self, typedesc::BaseType};

/// The scalar type of one channel value.
///
/// This describes what a file stores or what a writer should produce. It is
/// deliberately wider than the [`Pixel`](crate::Pixel) trait: OpenImageIO can
/// report formats such as [`PixelFormat::U32`] that the contiguous buffer API
/// does not read or write.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum PixelFormat {
    U8,
    I8,
    U16,
    I16,
    U32,
    I32,
    U64,
    I64,
    /// IEEE 754 binary16, matching [`half::f16`].
    F16,
    F32,
    F64,
    /// A format this crate does not model, including OpenImageIO's `UNKNOWN`.
    Other,
}

impl PixelFormat {
    /// Size of one channel value in bytes, when the format has a fixed size.
    pub fn byte_size(self) -> Option<usize> {
        Some(match self {
            Self::U8 | Self::I8 => 1,
            Self::U16 | Self::I16 | Self::F16 => 2,
            Self::U32 | Self::I32 | Self::F32 => 4,
            Self::U64 | Self::I64 | Self::F64 => 8,
            Self::Other => return None,
        })
    }

    /// Whether the contiguous [`Pixel`](crate::Pixel) API can read and write
    /// buffers of this format without conversion.
    pub fn is_supported_buffer_format(self) -> bool {
        matches!(self, Self::U8 | Self::U16 | Self::F16 | Self::F32)
    }

    /// The OpenImageIO name for this format, such as `"half"` or `"uint16"`.
    pub fn name(self) -> &'static str {
        match self {
            Self::U8 => "uint8",
            Self::I8 => "int8",
            Self::U16 => "uint16",
            Self::I16 => "int16",
            Self::U32 => "uint32",
            Self::I32 => "int32",
            Self::U64 => "uint64",
            Self::I64 => "int64",
            Self::F16 => "half",
            Self::F32 => "float",
            Self::F64 => "double",
            Self::Other => "unknown",
        }
    }

    pub(crate) fn base_type(self) -> BaseType {
        match self {
            Self::U8 => BaseType::UInt8,
            Self::I8 => BaseType::Int8,
            Self::U16 => BaseType::Uint16,
            Self::I16 => BaseType::Int16,
            Self::U32 => BaseType::UInt32,
            Self::I32 => BaseType::Int32,
            Self::U64 => BaseType::UInt64,
            Self::I64 => BaseType::Int64,
            Self::F16 => BaseType::Float16,
            Self::F32 => BaseType::Float32,
            Self::F64 => BaseType::Float64,
            Self::Other => BaseType::Unknown,
        }
    }

    pub(crate) fn to_sys(self) -> sys::typedesc::TypeDesc {
        sys::typedesc::typedesc_from_basetype_arraylen(self.base_type(), 0)
    }

    pub(crate) fn from_sys(type_desc: &sys::typedesc::TypeDesc) -> Self {
        // Aggregates and arrays are never plain pixel formats.
        if type_desc.arraylen != 0 || type_desc.aggregate != sys::typedesc::Aggregate::Scalar as u8
        {
            return Self::Other;
        }
        Self::from_base_type(type_desc.basetype)
    }

    fn from_base_type(basetype: u8) -> Self {
        match basetype {
            b if b == BaseType::UInt8 as u8 => Self::U8,
            b if b == BaseType::Int8 as u8 => Self::I8,
            b if b == BaseType::Uint16 as u8 => Self::U16,
            b if b == BaseType::Int16 as u8 => Self::I16,
            b if b == BaseType::UInt32 as u8 => Self::U32,
            b if b == BaseType::Int32 as u8 => Self::I32,
            b if b == BaseType::UInt64 as u8 => Self::U64,
            b if b == BaseType::Int64 as u8 => Self::I64,
            b if b == BaseType::Float16 as u8 => Self::F16,
            b if b == BaseType::Float32 as u8 => Self::F32,
            b if b == BaseType::Float64 as u8 => Self::F64,
            _ => Self::Other,
        }
    }
}

impl std::fmt::Display for PixelFormat {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.name())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_through_openimageio_type_descriptors() {
        let formats = [
            PixelFormat::U8,
            PixelFormat::I8,
            PixelFormat::U16,
            PixelFormat::I16,
            PixelFormat::U32,
            PixelFormat::I32,
            PixelFormat::U64,
            PixelFormat::I64,
            PixelFormat::F16,
            PixelFormat::F32,
            PixelFormat::F64,
        ];
        for format in formats {
            assert_eq!(PixelFormat::from_sys(&format.to_sys()), format);
        }
    }

    #[test]
    fn reports_openimageio_sizes() {
        for format in [PixelFormat::U8, PixelFormat::F16, PixelFormat::F32] {
            let expected = sys::typedesc::typedesc_size(&format.to_sys());
            assert_eq!(format.byte_size(), Some(expected));
        }
        assert_eq!(PixelFormat::Other.byte_size(), None);
    }

    #[test]
    fn treats_aggregates_and_arrays_as_other() {
        let matrix = sys::typedesc::typedesc_from_basetype_aggregate_arraylen(
            BaseType::Float32,
            sys::typedesc::Aggregate::Matrix44,
            0,
        );
        assert_eq!(PixelFormat::from_sys(&matrix), PixelFormat::Other);

        let array = sys::typedesc::typedesc_from_basetype_arraylen(BaseType::Float32, 4);
        assert_eq!(PixelFormat::from_sys(&array), PixelFormat::Other);
    }

    #[test]
    fn names_match_openimageio() {
        for format in [
            PixelFormat::U8,
            PixelFormat::U16,
            PixelFormat::F16,
            PixelFormat::F32,
            PixelFormat::F64,
        ] {
            let type_desc = format.to_sys();
            assert_eq!(sys::typedesc::typedesc_to_str(&type_desc), format.name());
        }
    }
}
