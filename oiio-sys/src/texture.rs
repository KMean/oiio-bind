pub use ffi::*;

// missing_safety_doc is allowed because the cxx macro does not carry doc
// comments onto the wrappers it generates; the safety contracts are written
// on the declarations below regardless, where a reader will look for them.
#[allow(clippy::too_many_arguments, clippy::missing_safety_doc)]
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
        pub type TextureHandle;

        pub fn texturesystem_create(shared: bool) -> SharedPtr<TextureSystem>;

        /// # Safety
        /// The returned pointer is owned by the texture system and is valid
        /// only until that system is destroyed or the file invalidated.
        pub unsafe fn texturesystem_get_texture_handle(
            texturesystem: Pin<&mut TextureSystem>,
            filename: &str,
        ) -> *mut TextureHandle;

        /// # Safety
        /// `handle` must be null or a handle this same texture system
        /// returned and has not invalidated.
        pub unsafe fn texturesystem_handle_good(
            texturesystem: Pin<&mut TextureSystem>,
            handle: *mut TextureHandle,
        ) -> bool;

        /// # Safety
        /// As in `texturesystem_handle_good`.
        pub unsafe fn texturesystem_handle_exists(
            texturesystem: Pin<&mut TextureSystem>,
            handle: *mut TextureHandle,
        ) -> bool;

        /// # Safety
        /// As in `texturesystem_handle_good`.
        pub unsafe fn texturesystem_handle_filename(
            texturesystem: Pin<&mut TextureSystem>,
            handle: *mut TextureHandle,
        ) -> String;

        /// # Safety
        /// As in `texturesystem_handle_good`.
        #[allow(clippy::too_many_arguments)]
        pub unsafe fn texturesystem_texture_by_handle(
            texturesystem: Pin<&mut TextureSystem>,
            handle: *mut TextureHandle,
            options: &TextureLookupOptions,
            missing_color: &[f32],
            s: f32,
            t: f32,
            dsdx: f32,
            dtdx: f32,
            dsdy: f32,
            dtdy: f32,
            result: &mut [f32],
            error: &mut String,
        ) -> bool;

        /// # Safety
        /// As in `texturesystem_handle_good`.
        #[allow(clippy::too_many_arguments)]
        pub unsafe fn texturesystem_environment_by_handle(
            texturesystem: Pin<&mut TextureSystem>,
            handle: *mut TextureHandle,
            options: &TextureLookupOptions,
            missing_color: &[f32],
            r_x: f32,
            r_y: f32,
            r_z: f32,
            drdx_x: f32,
            drdx_y: f32,
            drdx_z: f32,
            drdy_x: f32,
            drdy_y: f32,
            drdy_z: f32,
            result: &mut [f32],
            error: &mut String,
        ) -> bool;

        #[allow(clippy::too_many_arguments)]
        pub fn texturesystem_texture(
            texturesystem: Pin<&mut TextureSystem>,
            filename: &str,
            options: &TextureLookupOptions,
            missing_color: &[f32],
            s: f32,
            t: f32,
            dsdx: f32,
            dtdx: f32,
            dsdy: f32,
            dtdy: f32,
            result: &mut [f32],
            error: &mut String,
        ) -> bool;

        #[allow(clippy::too_many_arguments)]
        pub fn texturesystem_environment(
            texturesystem: Pin<&mut TextureSystem>,
            filename: &str,
            options: &TextureLookupOptions,
            missing_color: &[f32],
            r_x: f32,
            r_y: f32,
            r_z: f32,
            drdx_x: f32,
            drdx_y: f32,
            drdx_z: f32,
            drdy_x: f32,
            drdy_y: f32,
            drdy_z: f32,
            result: &mut [f32],
            error: &mut String,
        ) -> bool;

        pub fn texturesystem_is_udim(
            texturesystem: Pin<&mut TextureSystem>,
            filename: &str,
        ) -> bool;

        pub fn texturesystem_resolve_udim(
            texturesystem: Pin<&mut TextureSystem>,
            pattern: &str,
            s: f32,
            t: f32,
        ) -> String;

        pub fn texturesystem_inventory_udim(
            texturesystem: Pin<&mut TextureSystem>,
            pattern: &str,
            filenames: &mut Vec<String>,
            nutiles: &mut i32,
            nvtiles: &mut i32,
        );

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
