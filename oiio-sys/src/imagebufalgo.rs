pub use ffi::*;

#[cxx::bridge(namespace = oiio)]
mod ffi {
    /// Everything `ImageBufAlgo::compare` measured, flattened for the bridge.
    #[derive(Debug, Clone, Copy, PartialEq)]
    struct CompareSummary {
        mean_error: f64,
        root_mean_square_error: f64,
        peak_signal_to_noise_ratio: f64,
        max_error: f64,
        max_x: i32,
        max_y: i32,
        max_z: i32,
        max_channel: i32,
        warnings: u64,
        failures: u64,
        failed: bool,
    }

    unsafe extern "C++" {
        include!("oiio-sys/src/ffi_imagebufalgo.h");

        type ImageBuf = crate::imagebuf::ImageBuf;
        type ROI = crate::imageio::ROI;
        type TypeDesc = crate::typedesc::TypeDesc;

        pub fn imagebufalgo_zero(dst: Pin<&mut ImageBuf>, roi: &ROI, nthreads: i32) -> bool;

        pub fn imagebufalgo_fill(
            dst: Pin<&mut ImageBuf>,
            values: &[f32],
            roi: &ROI,
            nthreads: i32,
        ) -> bool;

        pub fn imagebufalgo_add_images(
            dst: Pin<&mut ImageBuf>,
            a: &ImageBuf,
            b: &ImageBuf,
            roi: &ROI,
            nthreads: i32,
        ) -> bool;

        pub fn imagebufalgo_add_constant(
            dst: Pin<&mut ImageBuf>,
            a: &ImageBuf,
            values: &[f32],
            roi: &ROI,
            nthreads: i32,
        ) -> bool;

        pub fn imagebufalgo_sub_images(
            dst: Pin<&mut ImageBuf>,
            a: &ImageBuf,
            b: &ImageBuf,
            roi: &ROI,
            nthreads: i32,
        ) -> bool;

        pub fn imagebufalgo_sub_constant(
            dst: Pin<&mut ImageBuf>,
            a: &ImageBuf,
            values: &[f32],
            roi: &ROI,
            nthreads: i32,
        ) -> bool;

        pub fn imagebufalgo_mul_images(
            dst: Pin<&mut ImageBuf>,
            a: &ImageBuf,
            b: &ImageBuf,
            roi: &ROI,
            nthreads: i32,
        ) -> bool;

        pub fn imagebufalgo_mul_constant(
            dst: Pin<&mut ImageBuf>,
            a: &ImageBuf,
            values: &[f32],
            roi: &ROI,
            nthreads: i32,
        ) -> bool;

        pub fn imagebufalgo_div_images(
            dst: Pin<&mut ImageBuf>,
            a: &ImageBuf,
            b: &ImageBuf,
            roi: &ROI,
            nthreads: i32,
        ) -> bool;

        pub fn imagebufalgo_div_constant(
            dst: Pin<&mut ImageBuf>,
            a: &ImageBuf,
            values: &[f32],
            roi: &ROI,
            nthreads: i32,
        ) -> bool;

        pub fn imagebufalgo_abs(
            dst: Pin<&mut ImageBuf>,
            a: &ImageBuf,
            roi: &ROI,
            nthreads: i32,
        ) -> bool;

        pub fn imagebufalgo_absdiff_images(
            dst: Pin<&mut ImageBuf>,
            a: &ImageBuf,
            b: &ImageBuf,
            roi: &ROI,
            nthreads: i32,
        ) -> bool;

        pub fn imagebufalgo_copy(
            dst: Pin<&mut ImageBuf>,
            src: &ImageBuf,
            convert: TypeDesc,
            roi: &ROI,
            nthreads: i32,
        ) -> bool;

        pub fn imagebufalgo_crop(
            dst: Pin<&mut ImageBuf>,
            src: &ImageBuf,
            roi: &ROI,
            nthreads: i32,
        ) -> bool;

        pub fn imagebufalgo_flip(
            dst: Pin<&mut ImageBuf>,
            src: &ImageBuf,
            roi: &ROI,
            nthreads: i32,
        ) -> bool;

        pub fn imagebufalgo_flop(
            dst: Pin<&mut ImageBuf>,
            src: &ImageBuf,
            roi: &ROI,
            nthreads: i32,
        ) -> bool;

        pub fn imagebufalgo_transpose(
            dst: Pin<&mut ImageBuf>,
            src: &ImageBuf,
            roi: &ROI,
            nthreads: i32,
        ) -> bool;

        pub fn imagebufalgo_compare(
            a: &ImageBuf,
            b: &ImageBuf,
            failthresh: f32,
            warnthresh: f32,
            roi: &ROI,
            nthreads: i32,
        ) -> CompareSummary;
    }
}
