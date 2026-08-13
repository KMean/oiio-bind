pub use ffi::*;

#[cxx::bridge(namespace = oiio)]
mod ffi {
    unsafe extern "C++" {
        include!("oiio-sys/src/ffi_color.h");

        pub type ColorConfig;

        pub fn colorconfig_default() -> UniquePtr<ColorConfig>;
        pub fn colorconfig_from_file(filename: &str) -> UniquePtr<ColorConfig>;
        pub fn colorconfig_supports_opencolorio() -> bool;
        pub fn colorconfig_name(config: &ColorConfig) -> String;
        pub fn colorconfig_geterror(config: &ColorConfig) -> String;
        pub fn colorconfig_color_space_names(config: &ColorConfig) -> Vec<String>;
        pub fn colorconfig_role_names(config: &ColorConfig) -> Vec<String>;
        pub fn colorconfig_color_space_for_role(config: &ColorConfig, role: &str) -> String;
    }
}
