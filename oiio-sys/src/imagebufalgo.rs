pub use ffi::*;

// These signatures mirror OpenImageIO's, argument for argument, so the
// operation-oriented wrappers live in `oiio` rather than being invented here.
#[allow(clippy::too_many_arguments)]
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
        type ImageSpec = crate::imageio::ImageSpec;
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

        pub fn imagebufalgo_colorconvert(
            dst: Pin<&mut ImageBuf>,
            src: &ImageBuf,
            fromspace: &str,
            tospace: &str,
            unpremult: bool,
            roi: &ROI,
            nthreads: i32,
        ) -> bool;

        pub fn imagebufalgo_resize(
            dst: Pin<&mut ImageBuf>,
            src: &ImageBuf,
            filtername: &str,
            filterwidth: f32,
            roi: &ROI,
            nthreads: i32,
        ) -> bool;

        pub fn imagebufalgo_fit(
            dst: Pin<&mut ImageBuf>,
            src: &ImageBuf,
            filtername: &str,
            filterwidth: f32,
            fillmode: &str,
            exact: bool,
            roi: &ROI,
            nthreads: i32,
        ) -> bool;

        pub fn imagebufalgo_resample(
            dst: Pin<&mut ImageBuf>,
            src: &ImageBuf,
            interpolate: bool,
            roi: &ROI,
            nthreads: i32,
        ) -> bool;

        pub fn imagebufalgo_over(
            dst: Pin<&mut ImageBuf>,
            a: &ImageBuf,
            b: &ImageBuf,
            roi: &ROI,
            nthreads: i32,
        ) -> bool;

        pub fn imagebufalgo_premult(
            dst: Pin<&mut ImageBuf>,
            src: &ImageBuf,
            roi: &ROI,
            nthreads: i32,
        ) -> bool;

        pub fn imagebufalgo_unpremult(
            dst: Pin<&mut ImageBuf>,
            src: &ImageBuf,
            roi: &ROI,
            nthreads: i32,
        ) -> bool;

        pub fn imagebufalgo_channel_sum(
            dst: Pin<&mut ImageBuf>,
            src: &ImageBuf,
            weights: &[f32],
            roi: &ROI,
            nthreads: i32,
        ) -> bool;

        pub fn imagebufalgo_channels(
            dst: Pin<&mut ImageBuf>,
            src: &ImageBuf,
            nchannels: i32,
            channelorder: &[i32],
            channelvalues: &[f32],
            newchannelnames: &Vec<String>,
            shuffle_channel_names: bool,
            nthreads: i32,
        ) -> bool;

        pub fn imagebufalgo_make_texture_from_buffer(
            mode: i32,
            input: &ImageBuf,
            outputfilename: &str,
            config: &ImageSpec,
            error: &mut String,
        ) -> bool;

        pub fn imagebufalgo_make_texture_from_file(
            mode: i32,
            filename: &str,
            outputfilename: &str,
            config: &ImageSpec,
            error: &mut String,
        ) -> bool;
    }
}
