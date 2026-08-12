use std::mem::{align_of, size_of};

#[test]
fn trivial_bridge_types_match_oiio_layouts() {
    use oiio_sys::{imagebuf, imageio, typedesc};

    assert_eq!(size_of::<typedesc::BaseType>(), 1);
    assert_eq!(size_of::<typedesc::Aggregate>(), 1);
    assert_eq!(size_of::<typedesc::VecSemantics>(), 1);
    assert_eq!(size_of::<typedesc::TypeDesc>(), 8);
    assert_eq!(align_of::<typedesc::TypeDesc>(), 4);

    assert_eq!(size_of::<imageio::ROI>(), 32);
    assert_eq!(align_of::<imageio::ROI>(), 4);
    assert_eq!(size_of::<imageio::OpenMode>(), 4);
    assert_eq!(size_of::<imagebuf::IBStorage>(), 4);
    assert_eq!(size_of::<imagebuf::WrapMode>(), 4);
    assert_eq!(size_of::<imagebuf::InitializePixels>(), 4);
}

#[test]
fn oiio_3_1_typedesc_variants_round_trip() {
    use oiio_sys::typedesc::{self, Aggregate, BaseType, VecSemantics};

    let hash = typedesc::typedesc_new(
        BaseType::UStringHash,
        Aggregate::Scalar,
        VecSemantics::NoSemantics,
        0,
    );
    assert_eq!(
        typedesc::typedesc_basetype(&hash) as u8,
        BaseType::UStringHash as u8
    );

    let bounds = typedesc::typedesc_new(BaseType::Float32, Aggregate::Vec2, VecSemantics::Box, 0);
    assert_eq!(
        typedesc::typedesc_vecsemantics(&bounds) as u8,
        VecSemantics::Box as u8
    );
}
