//! A binomial kernel is refused unless its extents are odd whole numbers.
//!
//! OpenImageIO's binomial branch sizes its tap arrays from the extent it is
//! given and then indexes them over the next odd size
//! (`imagebufalgo.cpp:936`), so an even or fractional extent reads past the
//! allocation and a fractional one writes past it. The value that comes back
//! is whatever the stack held: measured here over six runs of a 4x4, three
//! returned a kernel of NaN, one was numerically zero and two were correct.
//!
//! A NaN kernel poisons every pixel of whatever it convolves and OpenImageIO
//! reports no error, so this is refused rather than passed on.

use oiio::{algo, ImageBuf, ImageSpec, PixelFormat, Roi};

fn kernel_values(kernel: &ImageBuf) -> oiio::Result<Vec<f32>> {
    let spec = kernel.spec()?;
    let roi = spec.data_window()?;
    let mut values = vec![0.0_f32; roi.element_count()?];
    kernel.get_pixels_into(roi, &mut values)?;
    Ok(values)
}

#[test]
fn an_odd_binomial_kernel_is_built_and_is_sane() {
    for extent in [1.0_f32, 3.0, 5.0, 7.0] {
        let kernel = algo::make_kernel("binomial", extent, extent, true)
            .unwrap_or_else(|error| panic!("binomial {extent} should build: {error}"));

        let spec = kernel.spec().unwrap();
        assert_eq!(
            spec.dimensions()[0],
            extent as u32,
            "an odd extent should not be rounded"
        );

        let values = kernel_values(&kernel).unwrap();
        assert!(
            values.iter().all(|v| v.is_finite()),
            "binomial {extent} has a non-finite tap: {values:?}"
        );
        let sum: f32 = values.iter().sum();
        assert!(
            (sum - 1.0).abs() < 1e-5,
            "a normalised kernel should sum to one, got {sum}"
        );
    }
}

#[test]
fn the_3x3_binomial_is_the_values_it_should_be() {
    // [1 2 1] outer [1 2 1], over 16. Pinned so the guard cannot be
    // "correct" by refusing everything.
    let kernel = algo::make_kernel("binomial", 3.0, 3.0, true).unwrap();
    let values = kernel_values(&kernel).unwrap();
    let expected = [
        0.0625, 0.125, 0.0625, 0.125, 0.25, 0.125, 0.0625, 0.125, 0.0625_f32,
    ];
    assert_eq!(values.len(), expected.len());
    for (got, want) in values.iter().zip(expected) {
        assert!((got - want).abs() < 1e-6, "{values:?} != {expected:?}");
    }
}

#[test]
fn an_even_binomial_extent_is_refused() {
    for extent in [2.0_f32, 4.0, 6.0, 8.0] {
        let error = algo::make_kernel("binomial", extent, extent, true)
            .expect_err("an even binomial extent must be refused");
        let text = error.to_string();
        assert!(
            text.contains("odd whole"),
            "the error should say what is wanted: {text}"
        );
        assert!(
            text.contains("NaN"),
            "the error should say what goes wrong: {text}"
        );
    }
}

#[test]
fn a_fractional_binomial_extent_is_refused() {
    // The fractional case is the worse one: the fill itself writes past the
    // allocation rather than only reading past it.
    for extent in [2.5_f32, 4.5, 0.5] {
        assert!(
            algo::make_kernel("binomial", extent, extent, true).is_err(),
            "binomial {extent} must be refused"
        );
    }
}

#[test]
fn one_odd_and_one_even_extent_is_still_refused() {
    // Both extents get their own tap array, so either one being wrong is
    // enough.
    assert!(algo::make_kernel("binomial", 3.0, 4.0, true).is_err());
    assert!(algo::make_kernel("binomial", 4.0, 3.0, true).is_err());
}

#[test]
fn other_kernels_are_untouched_at_even_widths() {
    // The defect is the binomial branch alone: every other name goes through
    // a Filter2D, which is sized from the rounded image rather than the
    // extent. Refusing those too would be a gratuitous restriction.
    for name in ["gaussian", "box", "triangle", "blackman-harris", "lanczos3"] {
        let kernel = algo::make_kernel(name, 4.0, 4.0, true)
            .unwrap_or_else(|error| panic!("{name} at an even width should build: {error}"));
        let values = kernel_values(&kernel).unwrap();
        assert!(
            values.iter().all(|v| v.is_finite()),
            "{name} at an even width has a non-finite tap"
        );
    }
}

#[test]
fn unsharp_mask_refuses_an_even_binomial_width_too() {
    // unsharp_mask builds its blur with make_kernel(kernel, 1, width) above a
    // width of three and make_kernel(kernel, width, width) at or below it, so
    // the same extent reaches the same branch.
    let spec = ImageSpec::new(16, 16, 3, PixelFormat::F32).unwrap();
    let roi = spec.data_window().unwrap();
    let mut src = ImageBuf::new(&spec).unwrap();
    src.set_pixels(roi, &vec![0.5_f32; roi.element_count().unwrap()])
        .unwrap();

    let mut dst = ImageBuf::empty().unwrap();
    let error = algo::unsharp_mask(&mut dst, &src, "binomial", 4.0, 1.0, 0.0, None)
        .expect_err("an even binomial width must be refused here as well");
    assert!(
        error.to_string().contains("odd whole width"),
        "unexpected message: {error}"
    );

    // And an odd one still works, so the guard has not simply disabled it.
    let mut dst = ImageBuf::empty().unwrap();
    algo::unsharp_mask(&mut dst, &src, "binomial", 3.0, 1.0, 0.0, None).unwrap();
    let out_roi = Roi::new(0..16, 0..16, 0..1, 0..3).unwrap();
    let mut values = vec![0.0_f32; out_roi.element_count().unwrap()];
    dst.get_pixels_into(out_roi, &mut values).unwrap();
    assert!(
        values.iter().all(|v| v.is_finite()),
        "an odd binomial unsharp should not produce NaN"
    );
}
