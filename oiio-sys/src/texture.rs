pub use ffi::*;

#[allow(clippy::too_many_arguments)]
#[cxx::bridge(namespace = oiio)]
mod ffi {
    /// The subset of OpenImageIO's `TextureOpt` this crate exposes, flattened
    /// so it can cross the bridge as a value. Enum-valued fields travel as
    /// integers and are clamped back onto their enums in C++.
    #[derive(Debug, Clone, Copy, PartialEq)]
    struct TextureLookupOptions {
        first_channel: i32,
        subimage: i32,
        s_wrap: i32,
        t_wrap: i32,
        mip_mode: i32,
        interp_mode: i32,
        s_blur: f32,
        t_blur: f32,
        s_width: f32,
        t_width: f32,
        fill: f32,
    }

    unsafe extern "C++" {
        include!("oiio-sys/src/ffi_texture.h");

        pub type TextureSystem;

        pub fn texturesystem_create(shared: bool) -> SharedPtr<TextureSystem>;

        pub fn texturesystem_texture(
            texturesystem: Pin<&mut TextureSystem>,
            filename: &str,
            options: &TextureLookupOptions,
            s: f32,
            t: f32,
            dsdx: f32,
            dtdx: f32,
            dsdy: f32,
            dtdy: f32,
            result: &mut [f32],
            error: &mut String,
        ) -> bool;

        pub fn texturesystem_geterror(texturesystem: Pin<&mut TextureSystem>) -> String;
        pub fn texturesystem_attribute_int(
            texturesystem: Pin<&mut TextureSystem>,
            name: &str,
            value: i32,
        ) -> bool;
        pub fn texturesystem_attribute_float(
            texturesystem: Pin<&mut TextureSystem>,
            name: &str,
            value: f32,
        ) -> bool;
        pub fn texturesystem_getstats(texturesystem: &TextureSystem, level: i32) -> String;
        pub fn texturesystem_invalidate(
            texturesystem: Pin<&mut TextureSystem>,
            filename: &str,
            force: bool,
        );
        pub fn texturesystem_invalidate_all(texturesystem: Pin<&mut TextureSystem>, force: bool);
        pub fn texturesystem_resolution(
            texturesystem: Pin<&mut TextureSystem>,
            filename: &str,
            resolution: &mut [i32],
        ) -> bool;
    }
}
