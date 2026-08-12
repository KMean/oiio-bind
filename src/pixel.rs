use crate::sys;

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

/// A scalar pixel channel type that OpenImageIO can transfer directly.
///
/// This trait is sealed so callers cannot associate an arbitrary Rust layout
/// with an unrelated OpenImageIO `TypeDesc`.
pub trait Pixel: sealed::Sealed + Copy + Default + Send + Sync + 'static {}

impl Pixel for u8 {}
impl Pixel for u16 {}
impl Pixel for half::f16 {}
impl Pixel for f32 {}

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
    // Pixel is sealed to types with initialized scalar layouts, and the byte
    // slice cannot outlive or exceed the original slice.
    unsafe { std::slice::from_raw_parts(pixels.as_ptr().cast::<u8>(), byte_len) }
}
