use crate::{sys, PixelFormat};

mod sealed {
    use crate::sys::typedesc::BaseType;

    pub trait Sealed {
        const BASE_TYPE: BaseType;
    }

    impl Sealed for u8 {
        const BASE_TYPE: BaseType = BaseType::UInt8;
    }

    impl Sealed for u16 {
        const BASE_TYPE: BaseType = BaseType::Uint16;
    }

    impl Sealed for half::f16 {
        const BASE_TYPE: BaseType = BaseType::Float16;
    }

    impl Sealed for f32 {
        const BASE_TYPE: BaseType = BaseType::Float32;
    }
}

/// A scalar pixel channel type that OpenImageIO can read and write directly.
///
/// This trait is sealed so callers cannot associate an arbitrary Rust layout
/// with an unrelated OpenImageIO `TypeDesc`.
pub trait Pixel: sealed::Sealed + Copy + Default + Send + Sync + 'static {
    /// The OpenImageIO pixel format this Rust type represents.
    const FORMAT: PixelFormat;
}

impl Pixel for u8 {
    const FORMAT: PixelFormat = PixelFormat::U8;
}

impl Pixel for u16 {
    const FORMAT: PixelFormat = PixelFormat::U16;
}

impl Pixel for half::f16 {
    const FORMAT: PixelFormat = PixelFormat::F16;
}

impl Pixel for f32 {
    const FORMAT: PixelFormat = PixelFormat::F32;
}

pub(crate) fn type_desc<T: Pixel>() -> sys::typedesc::TypeDesc {
    sys::typedesc::typedesc_from_basetype_arraylen(T::BASE_TYPE, 0)
}

pub(crate) fn as_bytes_mut<T: Pixel>(pixels: &mut [T]) -> &mut [u8] {
    let byte_len = std::mem::size_of_val(pixels);
    // Pixel is sealed to types for which every bit pattern is valid, and the
    // byte slice cannot outlive or exceed the original mutable slice.
    unsafe { std::slice::from_raw_parts_mut(pixels.as_mut_ptr().cast::<u8>(), byte_len) }
}

pub(crate) fn as_bytes<T: Pixel>(pixels: &[T]) -> &[u8] {
    let byte_len = std::mem::size_of_val(pixels);
    // Pixel is sealed to initialized scalar layouts with no padding, and the
    // byte slice cannot outlive or exceed the original slice.
    unsafe { std::slice::from_raw_parts(pixels.as_ptr().cast::<u8>(), byte_len) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pixel_formats_agree_with_openimageio_type_descriptors() {
        fn check<T: Pixel>() {
            assert_eq!(PixelFormat::from_sys(&type_desc::<T>()), T::FORMAT);
            assert_eq!(T::FORMAT.byte_size(), Some(std::mem::size_of::<T>()));
            assert!(T::FORMAT.is_supported_buffer_format());
        }

        check::<u8>();
        check::<u16>();
        check::<half::f16>();
        check::<f32>();
    }

    #[test]
    fn borrows_whole_buffers_as_bytes() {
        let pixels = [1.0_f32, 2.0, 3.0];
        assert_eq!(as_bytes(&pixels).len(), 12);
        assert!(as_bytes::<f32>(&[]).is_empty());
    }
}
