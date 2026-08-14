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

    /// Everything `ImageBufAlgo::computePixelStats` measured, one entry per
    /// channel, flattened for the bridge. `ok` is false when nothing was
    /// measured, and `error` says why.
    #[derive(Debug, Clone, Default)]
    struct PixelStatistics {
        min: Vec<f32>,
        max: Vec<f32>,
        average: Vec<f32>,
        standard_deviation: Vec<f32>,
        nan_count: Vec<u64>,
        infinite_count: Vec<u64>,
        finite_count: Vec<u64>,
        ok: bool,
        error: String,
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
            error: &mut String,
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

        pub fn imagebufalgo_colormatrixtransform(
            dst: Pin<&mut ImageBuf>,
            src: &ImageBuf,
            matrix: &[f32],
            unpremult: bool,
            roi: &ROI,
            nthreads: i32,
        ) -> bool;

        pub fn imagebufalgo_ociolook(
            dst: Pin<&mut ImageBuf>,
            src: &ImageBuf,
            looks: &str,
            fromspace: &str,
            tospace: &str,
            unpremult: bool,
            inverse: bool,
            context_key: &str,
            context_value: &str,
            roi: &ROI,
            nthreads: i32,
        ) -> bool;

        pub fn imagebufalgo_ociodisplay(
            dst: Pin<&mut ImageBuf>,
            src: &ImageBuf,
            display: &str,
            view: &str,
            fromspace: &str,
            looks: &str,
            unpremult: bool,
            inverse: bool,
            context_key: &str,
            context_value: &str,
            roi: &ROI,
            nthreads: i32,
        ) -> bool;

        pub fn imagebufalgo_ociofiletransform(
            dst: Pin<&mut ImageBuf>,
            src: &ImageBuf,
            name: &str,
            unpremult: bool,
            inverse: bool,
            roi: &ROI,
            nthreads: i32,
        ) -> bool;

        pub fn imagebufalgo_ocionamedtransform(
            dst: Pin<&mut ImageBuf>,
            src: &ImageBuf,
            name: &str,
            unpremult: bool,
            inverse: bool,
            context_key: &str,
            context_value: &str,
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

        pub fn imagebufalgo_pixel_stats(
            src: &ImageBuf,
            roi: &ROI,
            nthreads: i32,
        ) -> PixelStatistics;

        pub fn imagebufalgo_histogram(
            src: &ImageBuf,
            channel: i32,
            bins: i32,
            min: f32,
            max: f32,
            ignore_empty: bool,
            roi: &ROI,
            nthreads: i32,
            error: &mut String,
        ) -> Vec<u64>;

        pub fn imagebufalgo_is_constant_color(
            src: &ImageBuf,
            threshold: f32,
            color: &mut [f32],
            roi: &ROI,
            nthreads: i32,
            error: &mut String,
        ) -> bool;

        pub fn imagebufalgo_is_constant_channel(
            src: &ImageBuf,
            channel: i32,
            value: f32,
            threshold: f32,
            roi: &ROI,
            nthreads: i32,
            error: &mut String,
        ) -> bool;

        pub fn imagebufalgo_is_monochrome(
            src: &ImageBuf,
            threshold: f32,
            roi: &ROI,
            nthreads: i32,
            error: &mut String,
        ) -> bool;

        pub fn imagebufalgo_nonzero_region(
            src: &ImageBuf,
            roi: &ROI,
            nthreads: i32,
            error: &mut String,
        ) -> ROI;

        pub fn imagebufalgo_pixel_hash_sha1(
            src: &ImageBuf,
            extrainfo: &str,
            roi: &ROI,
            nthreads: i32,
            error: &mut String,
        ) -> String;

        pub fn imagebufalgo_fill_vertical(
            dst: Pin<&mut ImageBuf>,
            top: &[f32],
            bottom: &[f32],
            roi: &ROI,
            nthreads: i32,
        ) -> bool;

        pub fn imagebufalgo_fill_corners(
            dst: Pin<&mut ImageBuf>,
            topleft: &[f32],
            topright: &[f32],
            bottomleft: &[f32],
            bottomright: &[f32],
            roi: &ROI,
            nthreads: i32,
        ) -> bool;

        pub fn imagebufalgo_checker(
            dst: Pin<&mut ImageBuf>,
            width: i32,
            height: i32,
            depth: i32,
            color1: &[f32],
            color2: &[f32],
            xoffset: i32,
            yoffset: i32,
            zoffset: i32,
            roi: &ROI,
            nthreads: i32,
        ) -> bool;

        pub fn imagebufalgo_noise(
            dst: Pin<&mut ImageBuf>,
            noisetype: &str,
            a: f32,
            b: f32,
            mono: bool,
            seed: i32,
            roi: &ROI,
            nthreads: i32,
        ) -> bool;

        pub fn imagebufalgo_render_point(
            dst: Pin<&mut ImageBuf>,
            x: i32,
            y: i32,
            color: &[f32],
            roi: &ROI,
            nthreads: i32,
        ) -> bool;

        pub fn imagebufalgo_render_line(
            dst: Pin<&mut ImageBuf>,
            x1: i32,
            y1: i32,
            x2: i32,
            y2: i32,
            color: &[f32],
            skip_first_point: bool,
            roi: &ROI,
            nthreads: i32,
        ) -> bool;

        pub fn imagebufalgo_render_box(
            dst: Pin<&mut ImageBuf>,
            x1: i32,
            y1: i32,
            x2: i32,
            y2: i32,
            color: &[f32],
            fill: bool,
            roi: &ROI,
            nthreads: i32,
        ) -> bool;

        pub fn imagebufalgo_render_text(
            dst: Pin<&mut ImageBuf>,
            x: i32,
            y: i32,
            text: &str,
            fontsize: i32,
            fontname: &str,
            color: &[f32],
            alignx: i32,
            aligny: i32,
            shadow: i32,
            roi: &ROI,
            nthreads: i32,
        ) -> bool;

        pub fn imagebufalgo_text_size(text: &str, fontsize: i32, fontname: &str) -> ROI;

        pub fn imagebufalgo_flatten(
            dst: Pin<&mut ImageBuf>,
            src: &ImageBuf,
            roi: &ROI,
            nthreads: i32,
        ) -> bool;

        pub fn imagebufalgo_deepen(
            dst: Pin<&mut ImageBuf>,
            src: &ImageBuf,
            zvalue: f32,
            roi: &ROI,
            nthreads: i32,
        ) -> bool;

        pub fn imagebufalgo_deep_merge(
            dst: Pin<&mut ImageBuf>,
            a: &ImageBuf,
            b: &ImageBuf,
            occlusion_cull: bool,
            roi: &ROI,
            nthreads: i32,
        ) -> bool;

        pub fn imagebufalgo_deep_holdout(
            dst: Pin<&mut ImageBuf>,
            src: &ImageBuf,
            holdout: &ImageBuf,
            roi: &ROI,
            nthreads: i32,
        ) -> bool;

        pub fn imagebufalgo_make_kernel(
            dst: Pin<&mut ImageBuf>,
            name: &str,
            width: f32,
            height: f32,
            depth: f32,
            normalize: bool,
        ) -> bool;

        pub fn imagebufalgo_convolve(
            dst: Pin<&mut ImageBuf>,
            src: &ImageBuf,
            kernel: &ImageBuf,
            normalize: bool,
            roi: &ROI,
            nthreads: i32,
        ) -> bool;

        pub fn imagebufalgo_laplacian(
            dst: Pin<&mut ImageBuf>,
            src: &ImageBuf,
            roi: &ROI,
            nthreads: i32,
        ) -> bool;

        pub fn imagebufalgo_unsharp_mask(
            dst: Pin<&mut ImageBuf>,
            src: &ImageBuf,
            kernel: &str,
            width: f32,
            contrast: f32,
            threshold: f32,
            roi: &ROI,
            nthreads: i32,
        ) -> bool;

        pub fn imagebufalgo_median_filter(
            dst: Pin<&mut ImageBuf>,
            src: &ImageBuf,
            width: i32,
            height: i32,
            roi: &ROI,
            nthreads: i32,
        ) -> bool;

        pub fn imagebufalgo_dilate(
            dst: Pin<&mut ImageBuf>,
            src: &ImageBuf,
            width: i32,
            height: i32,
            roi: &ROI,
            nthreads: i32,
        ) -> bool;

        pub fn imagebufalgo_erode(
            dst: Pin<&mut ImageBuf>,
            src: &ImageBuf,
            width: i32,
            height: i32,
            roi: &ROI,
            nthreads: i32,
        ) -> bool;

        pub fn imagebufalgo_fft(
            dst: Pin<&mut ImageBuf>,
            src: &ImageBuf,
            roi: &ROI,
            nthreads: i32,
        ) -> bool;

        pub fn imagebufalgo_ifft(
            dst: Pin<&mut ImageBuf>,
            src: &ImageBuf,
            roi: &ROI,
            nthreads: i32,
        ) -> bool;

        pub fn imagebufalgo_polar_to_complex(
            dst: Pin<&mut ImageBuf>,
            src: &ImageBuf,
            roi: &ROI,
            nthreads: i32,
        ) -> bool;

        pub fn imagebufalgo_complex_to_polar(
            dst: Pin<&mut ImageBuf>,
            src: &ImageBuf,
            roi: &ROI,
            nthreads: i32,
        ) -> bool;

        pub fn imagebufalgo_rotate90(
            dst: Pin<&mut ImageBuf>,
            src: &ImageBuf,
            roi: &ROI,
            nthreads: i32,
        ) -> bool;

        pub fn imagebufalgo_rotate180(
            dst: Pin<&mut ImageBuf>,
            src: &ImageBuf,
            roi: &ROI,
            nthreads: i32,
        ) -> bool;

        pub fn imagebufalgo_rotate270(
            dst: Pin<&mut ImageBuf>,
            src: &ImageBuf,
            roi: &ROI,
            nthreads: i32,
        ) -> bool;

        pub fn imagebufalgo_reorient(
            dst: Pin<&mut ImageBuf>,
            src: &ImageBuf,
            nthreads: i32,
        ) -> bool;

        pub fn imagebufalgo_rotate(
            dst: Pin<&mut ImageBuf>,
            src: &ImageBuf,
            angle: f32,
            has_center: bool,
            center_x: f32,
            center_y: f32,
            filtername: &str,
            filterwidth: f32,
            recompute_roi: bool,
            roi: &ROI,
            nthreads: i32,
        ) -> bool;

        pub fn imagebufalgo_warp(
            dst: Pin<&mut ImageBuf>,
            src: &ImageBuf,
            matrix: &[f32],
            filtername: &str,
            filterwidth: f32,
            wrap: &str,
            edgeclamp: bool,
            recompute_roi: bool,
            roi: &ROI,
            nthreads: i32,
        ) -> bool;

        pub fn imagebufalgo_st_warp(
            dst: Pin<&mut ImageBuf>,
            src: &ImageBuf,
            stbuf: &ImageBuf,
            filtername: &str,
            filterwidth: f32,
            chan_s: i32,
            chan_t: i32,
            flip_s: bool,
            flip_t: bool,
            roi: &ROI,
            nthreads: i32,
        ) -> bool;

        pub fn imagebufalgo_mad_iii(
            dst: Pin<&mut ImageBuf>,
            a: &ImageBuf,
            b: &ImageBuf,
            c: &ImageBuf,
            roi: &ROI,
            nthreads: i32,
        ) -> bool;

        pub fn imagebufalgo_mad_iic(
            dst: Pin<&mut ImageBuf>,
            a: &ImageBuf,
            b: &ImageBuf,
            c: &[f32],
            roi: &ROI,
            nthreads: i32,
        ) -> bool;

        pub fn imagebufalgo_mad_ici(
            dst: Pin<&mut ImageBuf>,
            a: &ImageBuf,
            b: &[f32],
            c: &ImageBuf,
            roi: &ROI,
            nthreads: i32,
        ) -> bool;

        pub fn imagebufalgo_mad_icc(
            dst: Pin<&mut ImageBuf>,
            a: &ImageBuf,
            b: &[f32],
            c: &[f32],
            roi: &ROI,
            nthreads: i32,
        ) -> bool;

        pub fn imagebufalgo_invert(
            dst: Pin<&mut ImageBuf>,
            a: &ImageBuf,
            roi: &ROI,
            nthreads: i32,
        ) -> bool;

        pub fn imagebufalgo_pow(
            dst: Pin<&mut ImageBuf>,
            a: &ImageBuf,
            b: &[f32],
            roi: &ROI,
            nthreads: i32,
        ) -> bool;

        pub fn imagebufalgo_clamp(
            dst: Pin<&mut ImageBuf>,
            src: &ImageBuf,
            min: &[f32],
            max: &[f32],
            clampalpha01: bool,
            roi: &ROI,
            nthreads: i32,
        ) -> bool;

        pub fn imagebufalgo_min_images(
            dst: Pin<&mut ImageBuf>,
            a: &ImageBuf,
            b: &ImageBuf,
            roi: &ROI,
            nthreads: i32,
        ) -> bool;

        pub fn imagebufalgo_min_constant(
            dst: Pin<&mut ImageBuf>,
            a: &ImageBuf,
            values: &[f32],
            roi: &ROI,
            nthreads: i32,
        ) -> bool;

        pub fn imagebufalgo_max_images(
            dst: Pin<&mut ImageBuf>,
            a: &ImageBuf,
            b: &ImageBuf,
            roi: &ROI,
            nthreads: i32,
        ) -> bool;

        pub fn imagebufalgo_max_constant(
            dst: Pin<&mut ImageBuf>,
            a: &ImageBuf,
            values: &[f32],
            roi: &ROI,
            nthreads: i32,
        ) -> bool;

        pub fn imagebufalgo_contrast_remap(
            dst: Pin<&mut ImageBuf>,
            src: &ImageBuf,
            black: &[f32],
            white: &[f32],
            min: &[f32],
            max: &[f32],
            scontrast: &[f32],
            sthresh: &[f32],
            roi: &ROI,
            nthreads: i32,
        ) -> bool;

        pub fn imagebufalgo_saturate(
            dst: Pin<&mut ImageBuf>,
            src: &ImageBuf,
            scale: f32,
            firstchannel: i32,
            roi: &ROI,
            nthreads: i32,
        ) -> bool;

        pub fn imagebufalgo_paste(
            dst: Pin<&mut ImageBuf>,
            xbegin: i32,
            ybegin: i32,
            zbegin: i32,
            chbegin: i32,
            src: &ImageBuf,
            srcroi: &ROI,
            nthreads: i32,
        ) -> bool;

        pub fn imagebufalgo_cut(
            dst: Pin<&mut ImageBuf>,
            src: &ImageBuf,
            roi: &ROI,
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
