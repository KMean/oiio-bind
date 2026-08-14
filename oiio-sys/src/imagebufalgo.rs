pub use ffi::*;

// These signatures mirror OpenImageIO's, argument for argument, so the
// operation-oriented wrappers live in `oiio` rather than being invented here.
//
// # Safety
//
// Every `imagebufalgo_*` declaration below is `unsafe fn`, and they share one
// contract, which is why it is stated here rather than repeated eighty-seven
// times.
//
// The caller must ensure the region's channel range exists in the image the
// operation writes into. `ROI` is a plain repr(C) struct with public fields, so
// any range at all can be built, and OpenImageIO does not check it:
// `ImageBufAlgo::IBAprep` opens with
// `roi = roi_intersection(roi, get_roi(dst->spec()))`, and `roi_intersection`
// takes the larger begin and the smaller end. A channel range starting past the
// destination's last channel therefore comes back inverted -- 5..8 against 0..3
// gives chbegin 5, chend 3 -- and `ROI::nchannels()` is then negative. The
// kernels use it as an unsigned length: `zero` reaches `memcpy` with
// `(size_t)-8` from an address already past the end of the pixel.
//
// A region that is undefined (`roi_default`) means the whole image and is
// always sound. Where a destination is not yet allocated, `IBAprep` builds it
// out of the region and then intersects against what it built, so the range
// must begin at channel zero.
//
// `oiio::algo::region_in` is the implementation of that contract.
#[allow(clippy::too_many_arguments)]
#[allow(clippy::missing_safety_doc)]
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

        pub unsafe fn imagebufalgo_zero(dst: Pin<&mut ImageBuf>, roi: &ROI, nthreads: i32) -> bool;

        pub unsafe fn imagebufalgo_fill(
            dst: Pin<&mut ImageBuf>,
            values: &[f32],
            roi: &ROI,
            nthreads: i32,
        ) -> bool;

        pub unsafe fn imagebufalgo_add_images(
            dst: Pin<&mut ImageBuf>,
            a: &ImageBuf,
            b: &ImageBuf,
            roi: &ROI,
            nthreads: i32,
        ) -> bool;

        pub unsafe fn imagebufalgo_add_constant(
            dst: Pin<&mut ImageBuf>,
            a: &ImageBuf,
            values: &[f32],
            roi: &ROI,
            nthreads: i32,
        ) -> bool;

        pub unsafe fn imagebufalgo_sub_images(
            dst: Pin<&mut ImageBuf>,
            a: &ImageBuf,
            b: &ImageBuf,
            roi: &ROI,
            nthreads: i32,
        ) -> bool;

        pub unsafe fn imagebufalgo_sub_constant(
            dst: Pin<&mut ImageBuf>,
            a: &ImageBuf,
            values: &[f32],
            roi: &ROI,
            nthreads: i32,
        ) -> bool;

        pub unsafe fn imagebufalgo_mul_images(
            dst: Pin<&mut ImageBuf>,
            a: &ImageBuf,
            b: &ImageBuf,
            roi: &ROI,
            nthreads: i32,
        ) -> bool;

        pub unsafe fn imagebufalgo_mul_constant(
            dst: Pin<&mut ImageBuf>,
            a: &ImageBuf,
            values: &[f32],
            roi: &ROI,
            nthreads: i32,
        ) -> bool;

        pub unsafe fn imagebufalgo_div_images(
            dst: Pin<&mut ImageBuf>,
            a: &ImageBuf,
            b: &ImageBuf,
            roi: &ROI,
            nthreads: i32,
        ) -> bool;

        pub unsafe fn imagebufalgo_div_constant(
            dst: Pin<&mut ImageBuf>,
            a: &ImageBuf,
            values: &[f32],
            roi: &ROI,
            nthreads: i32,
        ) -> bool;

        pub unsafe fn imagebufalgo_abs(
            dst: Pin<&mut ImageBuf>,
            a: &ImageBuf,
            roi: &ROI,
            nthreads: i32,
        ) -> bool;

        pub unsafe fn imagebufalgo_absdiff_images(
            dst: Pin<&mut ImageBuf>,
            a: &ImageBuf,
            b: &ImageBuf,
            roi: &ROI,
            nthreads: i32,
        ) -> bool;

        pub unsafe fn imagebufalgo_copy(
            dst: Pin<&mut ImageBuf>,
            src: &ImageBuf,
            convert: TypeDesc,
            roi: &ROI,
            nthreads: i32,
        ) -> bool;

        pub unsafe fn imagebufalgo_crop(
            dst: Pin<&mut ImageBuf>,
            src: &ImageBuf,
            roi: &ROI,
            nthreads: i32,
        ) -> bool;

        pub unsafe fn imagebufalgo_flip(
            dst: Pin<&mut ImageBuf>,
            src: &ImageBuf,
            roi: &ROI,
            nthreads: i32,
        ) -> bool;

        pub unsafe fn imagebufalgo_flop(
            dst: Pin<&mut ImageBuf>,
            src: &ImageBuf,
            roi: &ROI,
            nthreads: i32,
        ) -> bool;

        pub unsafe fn imagebufalgo_transpose(
            dst: Pin<&mut ImageBuf>,
            src: &ImageBuf,
            roi: &ROI,
            nthreads: i32,
        ) -> bool;

        pub unsafe fn imagebufalgo_compare(
            a: &ImageBuf,
            b: &ImageBuf,
            failthresh: f32,
            warnthresh: f32,
            roi: &ROI,
            nthreads: i32,
            error: &mut String,
        ) -> CompareSummary;

        pub unsafe fn imagebufalgo_colorconvert(
            dst: Pin<&mut ImageBuf>,
            src: &ImageBuf,
            fromspace: &str,
            tospace: &str,
            unpremult: bool,
            roi: &ROI,
            nthreads: i32,
        ) -> bool;

        pub unsafe fn imagebufalgo_colormatrixtransform(
            dst: Pin<&mut ImageBuf>,
            src: &ImageBuf,
            matrix: &[f32],
            unpremult: bool,
            roi: &ROI,
            nthreads: i32,
        ) -> bool;

        pub unsafe fn imagebufalgo_ociolook(
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

        pub unsafe fn imagebufalgo_ociodisplay(
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

        pub unsafe fn imagebufalgo_ociofiletransform(
            dst: Pin<&mut ImageBuf>,
            src: &ImageBuf,
            name: &str,
            unpremult: bool,
            inverse: bool,
            roi: &ROI,
            nthreads: i32,
        ) -> bool;

        pub unsafe fn imagebufalgo_ocionamedtransform(
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

        pub unsafe fn imagebufalgo_resize(
            dst: Pin<&mut ImageBuf>,
            src: &ImageBuf,
            filtername: &str,
            filterwidth: f32,
            roi: &ROI,
            nthreads: i32,
        ) -> bool;

        pub unsafe fn imagebufalgo_fit(
            dst: Pin<&mut ImageBuf>,
            src: &ImageBuf,
            filtername: &str,
            filterwidth: f32,
            fillmode: &str,
            exact: bool,
            roi: &ROI,
            nthreads: i32,
        ) -> bool;

        pub unsafe fn imagebufalgo_resample(
            dst: Pin<&mut ImageBuf>,
            src: &ImageBuf,
            interpolate: bool,
            roi: &ROI,
            nthreads: i32,
        ) -> bool;

        pub unsafe fn imagebufalgo_over(
            dst: Pin<&mut ImageBuf>,
            a: &ImageBuf,
            b: &ImageBuf,
            roi: &ROI,
            nthreads: i32,
        ) -> bool;

        pub unsafe fn imagebufalgo_premult(
            dst: Pin<&mut ImageBuf>,
            src: &ImageBuf,
            roi: &ROI,
            nthreads: i32,
        ) -> bool;

        pub unsafe fn imagebufalgo_unpremult(
            dst: Pin<&mut ImageBuf>,
            src: &ImageBuf,
            roi: &ROI,
            nthreads: i32,
        ) -> bool;

        pub unsafe fn imagebufalgo_repremult(
            dst: Pin<&mut ImageBuf>,
            src: &ImageBuf,
            roi: &ROI,
            nthreads: i32,
        ) -> bool;

        pub unsafe fn imagebufalgo_zover(
            dst: Pin<&mut ImageBuf>,
            a: &ImageBuf,
            b: &ImageBuf,
            z_zeroisinf: bool,
            roi: &ROI,
            nthreads: i32,
        ) -> bool;

        pub unsafe fn imagebufalgo_scale(
            dst: Pin<&mut ImageBuf>,
            a: &ImageBuf,
            b: &ImageBuf,
            roi: &ROI,
            nthreads: i32,
        ) -> bool;

        pub unsafe fn imagebufalgo_fix_non_finite(
            dst: Pin<&mut ImageBuf>,
            src: &ImageBuf,
            mode: i32,
            pixels_fixed: &mut i64,
            roi: &ROI,
            nthreads: i32,
        ) -> bool;

        pub unsafe fn imagebufalgo_rangecompress(
            dst: Pin<&mut ImageBuf>,
            src: &ImageBuf,
            use_luma: bool,
            roi: &ROI,
            nthreads: i32,
        ) -> bool;

        pub unsafe fn imagebufalgo_rangeexpand(
            dst: Pin<&mut ImageBuf>,
            src: &ImageBuf,
            use_luma: bool,
            roi: &ROI,
            nthreads: i32,
        ) -> bool;

        pub unsafe fn imagebufalgo_channel_sum(
            dst: Pin<&mut ImageBuf>,
            src: &ImageBuf,
            weights: &[f32],
            roi: &ROI,
            nthreads: i32,
        ) -> bool;

        pub unsafe fn imagebufalgo_channels(
            dst: Pin<&mut ImageBuf>,
            src: &ImageBuf,
            nchannels: i32,
            channelorder: &[i32],
            channelvalues: &[f32],
            newchannelnames: &Vec<String>,
            shuffle_channel_names: bool,
            nthreads: i32,
        ) -> bool;

        pub unsafe fn imagebufalgo_pixel_stats(
            src: &ImageBuf,
            roi: &ROI,
            nthreads: i32,
        ) -> PixelStatistics;

        pub unsafe fn imagebufalgo_histogram(
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

        pub unsafe fn imagebufalgo_is_constant_color(
            src: &ImageBuf,
            threshold: f32,
            color: &mut [f32],
            roi: &ROI,
            nthreads: i32,
            error: &mut String,
        ) -> bool;

        pub unsafe fn imagebufalgo_is_constant_channel(
            src: &ImageBuf,
            channel: i32,
            value: f32,
            threshold: f32,
            roi: &ROI,
            nthreads: i32,
            error: &mut String,
        ) -> bool;

        pub unsafe fn imagebufalgo_is_monochrome(
            src: &ImageBuf,
            threshold: f32,
            roi: &ROI,
            nthreads: i32,
            error: &mut String,
        ) -> bool;

        pub unsafe fn imagebufalgo_nonzero_region(
            src: &ImageBuf,
            roi: &ROI,
            nthreads: i32,
            error: &mut String,
        ) -> ROI;

        pub unsafe fn imagebufalgo_pixel_hash_sha1(
            src: &ImageBuf,
            extrainfo: &str,
            roi: &ROI,
            nthreads: i32,
            error: &mut String,
        ) -> String;

        pub unsafe fn imagebufalgo_fill_vertical(
            dst: Pin<&mut ImageBuf>,
            top: &[f32],
            bottom: &[f32],
            roi: &ROI,
            nthreads: i32,
        ) -> bool;

        pub unsafe fn imagebufalgo_fill_corners(
            dst: Pin<&mut ImageBuf>,
            topleft: &[f32],
            topright: &[f32],
            bottomleft: &[f32],
            bottomright: &[f32],
            roi: &ROI,
            nthreads: i32,
        ) -> bool;

        pub unsafe fn imagebufalgo_checker(
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

        pub unsafe fn imagebufalgo_noise(
            dst: Pin<&mut ImageBuf>,
            noisetype: &str,
            a: f32,
            b: f32,
            mono: bool,
            seed: i32,
            roi: &ROI,
            nthreads: i32,
        ) -> bool;

        pub unsafe fn imagebufalgo_render_point(
            dst: Pin<&mut ImageBuf>,
            x: i32,
            y: i32,
            color: &[f32],
            roi: &ROI,
            nthreads: i32,
        ) -> bool;

        pub unsafe fn imagebufalgo_render_line(
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

        pub unsafe fn imagebufalgo_render_box(
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

        pub unsafe fn imagebufalgo_render_text(
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

        pub unsafe fn imagebufalgo_text_size(text: &str, fontsize: i32, fontname: &str) -> ROI;

        pub unsafe fn imagebufalgo_flatten(
            dst: Pin<&mut ImageBuf>,
            src: &ImageBuf,
            roi: &ROI,
            nthreads: i32,
        ) -> bool;

        pub unsafe fn imagebufalgo_deepen(
            dst: Pin<&mut ImageBuf>,
            src: &ImageBuf,
            zvalue: f32,
            roi: &ROI,
            nthreads: i32,
        ) -> bool;

        pub unsafe fn imagebufalgo_deep_merge(
            dst: Pin<&mut ImageBuf>,
            a: &ImageBuf,
            b: &ImageBuf,
            occlusion_cull: bool,
            roi: &ROI,
            nthreads: i32,
        ) -> bool;

        pub unsafe fn imagebufalgo_deep_holdout(
            dst: Pin<&mut ImageBuf>,
            src: &ImageBuf,
            holdout: &ImageBuf,
            roi: &ROI,
            nthreads: i32,
        ) -> bool;

        pub unsafe fn imagebufalgo_make_kernel(
            dst: Pin<&mut ImageBuf>,
            name: &str,
            width: f32,
            height: f32,
            depth: f32,
            normalize: bool,
        ) -> bool;

        pub unsafe fn imagebufalgo_convolve(
            dst: Pin<&mut ImageBuf>,
            src: &ImageBuf,
            kernel: &ImageBuf,
            normalize: bool,
            roi: &ROI,
            nthreads: i32,
        ) -> bool;

        pub unsafe fn imagebufalgo_laplacian(
            dst: Pin<&mut ImageBuf>,
            src: &ImageBuf,
            roi: &ROI,
            nthreads: i32,
        ) -> bool;

        pub unsafe fn imagebufalgo_unsharp_mask(
            dst: Pin<&mut ImageBuf>,
            src: &ImageBuf,
            kernel: &str,
            width: f32,
            contrast: f32,
            threshold: f32,
            roi: &ROI,
            nthreads: i32,
        ) -> bool;

        pub unsafe fn imagebufalgo_median_filter(
            dst: Pin<&mut ImageBuf>,
            src: &ImageBuf,
            width: i32,
            height: i32,
            roi: &ROI,
            nthreads: i32,
        ) -> bool;

        pub unsafe fn imagebufalgo_dilate(
            dst: Pin<&mut ImageBuf>,
            src: &ImageBuf,
            width: i32,
            height: i32,
            roi: &ROI,
            nthreads: i32,
        ) -> bool;

        pub unsafe fn imagebufalgo_erode(
            dst: Pin<&mut ImageBuf>,
            src: &ImageBuf,
            width: i32,
            height: i32,
            roi: &ROI,
            nthreads: i32,
        ) -> bool;

        pub unsafe fn imagebufalgo_fft(
            dst: Pin<&mut ImageBuf>,
            src: &ImageBuf,
            roi: &ROI,
            nthreads: i32,
        ) -> bool;

        pub unsafe fn imagebufalgo_ifft(
            dst: Pin<&mut ImageBuf>,
            src: &ImageBuf,
            roi: &ROI,
            nthreads: i32,
        ) -> bool;

        pub unsafe fn imagebufalgo_polar_to_complex(
            dst: Pin<&mut ImageBuf>,
            src: &ImageBuf,
            roi: &ROI,
            nthreads: i32,
        ) -> bool;

        pub unsafe fn imagebufalgo_complex_to_polar(
            dst: Pin<&mut ImageBuf>,
            src: &ImageBuf,
            roi: &ROI,
            nthreads: i32,
        ) -> bool;

        pub unsafe fn imagebufalgo_rotate90(
            dst: Pin<&mut ImageBuf>,
            src: &ImageBuf,
            roi: &ROI,
            nthreads: i32,
        ) -> bool;

        pub unsafe fn imagebufalgo_rotate180(
            dst: Pin<&mut ImageBuf>,
            src: &ImageBuf,
            roi: &ROI,
            nthreads: i32,
        ) -> bool;

        pub unsafe fn imagebufalgo_rotate270(
            dst: Pin<&mut ImageBuf>,
            src: &ImageBuf,
            roi: &ROI,
            nthreads: i32,
        ) -> bool;

        pub unsafe fn imagebufalgo_reorient(
            dst: Pin<&mut ImageBuf>,
            src: &ImageBuf,
            nthreads: i32,
        ) -> bool;

        pub unsafe fn imagebufalgo_rotate(
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

        pub unsafe fn imagebufalgo_warp(
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

        pub unsafe fn imagebufalgo_st_warp(
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

        pub unsafe fn imagebufalgo_mad_iii(
            dst: Pin<&mut ImageBuf>,
            a: &ImageBuf,
            b: &ImageBuf,
            c: &ImageBuf,
            roi: &ROI,
            nthreads: i32,
        ) -> bool;

        pub unsafe fn imagebufalgo_mad_iic(
            dst: Pin<&mut ImageBuf>,
            a: &ImageBuf,
            b: &ImageBuf,
            c: &[f32],
            roi: &ROI,
            nthreads: i32,
        ) -> bool;

        pub unsafe fn imagebufalgo_mad_ici(
            dst: Pin<&mut ImageBuf>,
            a: &ImageBuf,
            b: &[f32],
            c: &ImageBuf,
            roi: &ROI,
            nthreads: i32,
        ) -> bool;

        pub unsafe fn imagebufalgo_mad_icc(
            dst: Pin<&mut ImageBuf>,
            a: &ImageBuf,
            b: &[f32],
            c: &[f32],
            roi: &ROI,
            nthreads: i32,
        ) -> bool;

        pub unsafe fn imagebufalgo_invert(
            dst: Pin<&mut ImageBuf>,
            a: &ImageBuf,
            roi: &ROI,
            nthreads: i32,
        ) -> bool;

        pub unsafe fn imagebufalgo_pow(
            dst: Pin<&mut ImageBuf>,
            a: &ImageBuf,
            b: &[f32],
            roi: &ROI,
            nthreads: i32,
        ) -> bool;

        pub unsafe fn imagebufalgo_clamp(
            dst: Pin<&mut ImageBuf>,
            src: &ImageBuf,
            min: &[f32],
            max: &[f32],
            clampalpha01: bool,
            roi: &ROI,
            nthreads: i32,
        ) -> bool;

        pub unsafe fn imagebufalgo_min_images(
            dst: Pin<&mut ImageBuf>,
            a: &ImageBuf,
            b: &ImageBuf,
            roi: &ROI,
            nthreads: i32,
        ) -> bool;

        pub unsafe fn imagebufalgo_min_constant(
            dst: Pin<&mut ImageBuf>,
            a: &ImageBuf,
            values: &[f32],
            roi: &ROI,
            nthreads: i32,
        ) -> bool;

        pub unsafe fn imagebufalgo_max_images(
            dst: Pin<&mut ImageBuf>,
            a: &ImageBuf,
            b: &ImageBuf,
            roi: &ROI,
            nthreads: i32,
        ) -> bool;

        pub unsafe fn imagebufalgo_max_constant(
            dst: Pin<&mut ImageBuf>,
            a: &ImageBuf,
            values: &[f32],
            roi: &ROI,
            nthreads: i32,
        ) -> bool;

        pub unsafe fn imagebufalgo_contrast_remap(
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

        pub unsafe fn imagebufalgo_saturate(
            dst: Pin<&mut ImageBuf>,
            src: &ImageBuf,
            scale: f32,
            firstchannel: i32,
            roi: &ROI,
            nthreads: i32,
        ) -> bool;

        pub unsafe fn imagebufalgo_paste(
            dst: Pin<&mut ImageBuf>,
            xbegin: i32,
            ybegin: i32,
            zbegin: i32,
            chbegin: i32,
            src: &ImageBuf,
            srcroi: &ROI,
            nthreads: i32,
        ) -> bool;

        pub unsafe fn imagebufalgo_cut(
            dst: Pin<&mut ImageBuf>,
            src: &ImageBuf,
            roi: &ROI,
            nthreads: i32,
        ) -> bool;

        pub unsafe fn imagebufalgo_make_texture_from_buffer(
            mode: i32,
            input: &ImageBuf,
            outputfilename: &str,
            config: &ImageSpec,
            error: &mut String,
        ) -> bool;

        pub unsafe fn imagebufalgo_make_texture_from_file(
            mode: i32,
            filename: &str,
            outputfilename: &str,
            config: &ImageSpec,
            error: &mut String,
        ) -> bool;
    }
}
