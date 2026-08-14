//! Convolution, morphology and the Fourier pair.

mod common;

use oiio::{algo, ImageBuf, ImageSpec, PixelFormat, Roi};

fn single_channel(width: u32, height: u32) -> ImageBuf {
    let spec = ImageSpec::new(width, height, 1, PixelFormat::F32).unwrap();
    let mut image = ImageBuf::new(&spec).unwrap();
    algo::zero(&mut image, None).unwrap();
    image
}

fn value(image: &ImageBuf, x: i32, y: i32) -> f32 {
    let roi = Roi::new(x..x + 1, y..y + 1, 0..1, 0..1).unwrap();
    let mut one = [0.0_f32; 1];
    image.get_pixels_into(roi, &mut one).unwrap();
    one[0]
}

fn set(image: &mut ImageBuf, x: i32, y: i32, v: f32) {
    let roi = Roi::new(x..x + 1, y..y + 1, 0..1, 0..1).unwrap();
    image.set_pixels(roi, &[v]).unwrap();
}

#[test]
fn a_named_kernel_is_built_and_normalised() {
    let kernel = algo::make_kernel("gaussian", 5.0, 5.0, true).unwrap();
    let spec = kernel.spec().unwrap();
    println!(
        "gaussian 5x5: {:?} at {:?}",
        spec.dimensions(),
        spec.origin()
    );
    assert!(spec.dimensions()[0] >= 5 && spec.dimensions()[1] >= 5);

    // Normalised means the values sum to one.
    let stats = algo::pixel_stats(&kernel, None).unwrap();
    let total = stats.average[0] * stats.finite_count[0] as f32;
    assert!(
        (total - 1.0).abs() < 1e-3,
        "a normalised kernel should sum to one, got {total}"
    );

    // It is centred on the origin, which is what convolve wants.
    let origin = spec.origin();
    assert!(
        origin[0] < 0 && origin[1] < 0,
        "unexpected origin {origin:?}"
    );
}

/// OpenImageIO answers an unknown filter name with a box kernel and a
/// complaint recorded on the buffer, so a caller who does not look ends up
/// convolving with something they did not ask for.
#[test]
fn an_unknown_kernel_name_is_an_error_not_a_box() {
    let error = algo::make_kernel("no-such-filter", 3.0, 3.0, true).unwrap_err();
    println!("unknown kernel name reported as: {error}");
}

#[test]
fn convolving_with_a_blur_spreads_a_single_bright_pixel() {
    let mut source = single_channel(16, 16);
    set(&mut source, 8, 8, 1.0);

    let kernel = algo::make_kernel("gaussian", 5.0, 5.0, true).unwrap();
    let mut blurred = ImageBuf::empty().unwrap();
    algo::convolve(&mut blurred, &source, &kernel, true, None).unwrap();

    assert!(
        value(&blurred, 8, 8) < 1.0,
        "the peak should have spread out"
    );
    assert!(
        value(&blurred, 9, 8) > 0.0,
        "the neighbour should have picked something up"
    );

    // A normalised blur conserves total energy.
    let before = algo::pixel_stats(&source, None).unwrap();
    let after = algo::pixel_stats(&blurred, None).unwrap();
    let sum = |s: &oiio::algo::PixelStats| s.average[0] * s.finite_count[0] as f32;
    assert!(
        (sum(&before) - sum(&after)).abs() < 1e-3,
        "a normalised kernel should conserve the total: {} then {}",
        sum(&before),
        sum(&after)
    );
}

/// An empty kernel makes OpenImageIO divide by a zero sum, fill the image with
/// NaN, and report success.
#[test]
fn convolving_with_an_empty_kernel_is_refused() {
    let source = single_channel(8, 8);
    let empty = ImageBuf::empty().unwrap();
    let mut result = ImageBuf::empty().unwrap();

    let error = algo::convolve(&mut result, &source, &empty, true, None).unwrap_err();
    println!("empty kernel reported as: {error}");
    assert!(error.to_string().contains("empty"), "{error}");
}

#[test]
fn the_laplacian_finds_an_edge() {
    let mut source = single_channel(16, 16);
    // A step: the left half dark, the right half bright.
    for y in 0..16 {
        for x in 8..16 {
            set(&mut source, x, y, 1.0);
        }
    }

    let mut edges = ImageBuf::empty().unwrap();
    algo::laplacian(&mut edges, &source, None).unwrap();

    // Flat areas stay flat; the edge does not.
    assert!(
        value(&edges, 3, 8).abs() < 1e-5,
        "the flat left should be 0"
    );
    assert!(
        value(&edges, 13, 8).abs() < 1e-5,
        "the flat right should be 0"
    );
    let at_edge = value(&edges, 8, 8);
    println!("laplacian at the step: {at_edge}");
    assert!(
        at_edge.abs() > 0.5,
        "the edge should respond, got {at_edge}"
    );
}

#[test]
fn unsharp_mask_increases_local_contrast() {
    let mut source = single_channel(32, 32);
    for y in 0..32 {
        for x in 16..32 {
            set(&mut source, x, y, 0.5);
        }
    }

    let mut sharpened = ImageBuf::empty().unwrap();
    algo::unsharp_mask(&mut sharpened, &source, "gaussian", 3.0, 1.0, 0.0, None).unwrap();

    // Just inside the bright side of the step, sharpening overshoots upward.
    let plain = value(&source, 16, 16);
    let sharp = value(&sharpened, 16, 16);
    println!("at the step: {plain} then {sharp}");
    assert!(
        sharp > plain,
        "sharpening should overshoot at an edge, got {sharp} from {plain}"
    );
}

/// OpenImageIO reads the source through an iterator of the *destination's*
/// pixel type without converting, so a mismatch reinterprets the source's
/// bytes and, for a wider destination, reads past its end.
#[test]
fn unsharp_mask_refuses_a_destination_of_another_type() {
    let source = ImageBuf::new(&ImageSpec::new(16, 16, 1, PixelFormat::U8).unwrap()).unwrap();
    let mut wider = ImageBuf::new(&ImageSpec::new(16, 16, 1, PixelFormat::F32).unwrap()).unwrap();

    let error =
        algo::unsharp_mask(&mut wider, &source, "gaussian", 3.0, 1.0, 0.0, None).unwrap_err();
    println!("mismatched types reported as: {error}");
    assert!(error.to_string().contains("type"), "{error}");

    // An empty destination is fine: it takes the source's type.
    let mut empty = ImageBuf::empty().unwrap();
    algo::unsharp_mask(&mut empty, &source, "gaussian", 3.0, 1.0, 0.0, None).unwrap();
}

#[test]
fn the_median_filter_removes_a_speck_a_blur_would_smear() {
    let mut source = single_channel(16, 16);
    algo::fill(&mut source, &[0.5], None).unwrap();
    set(&mut source, 8, 8, 1.0); // one bright speck

    let mut cleaned = ImageBuf::empty().unwrap();
    algo::median_filter(&mut cleaned, &source, 3, None, None).unwrap();

    assert!(
        (value(&cleaned, 8, 8) - 0.5).abs() < 1e-5,
        "a lone speck should be replaced by its neighbours' median, got {}",
        value(&cleaned, 8, 8)
    );
}

/// A window of one is not a no-op upstream: it reads the pixel one to the left
/// and above, so the image comes back shifted.
#[test]
fn a_window_of_one_is_refused() {
    let source = single_channel(8, 8);

    for (name, outcome) in [
        ("median_filter", {
            let mut d = ImageBuf::empty().unwrap();
            algo::median_filter(&mut d, &source, 1, None, None)
        }),
        ("dilate", {
            let mut d = ImageBuf::empty().unwrap();
            algo::dilate(&mut d, &source, 1, None, None)
        }),
        ("erode", {
            let mut d = ImageBuf::empty().unwrap();
            algo::erode(&mut d, &source, 1, None, None)
        }),
    ] {
        let error = outcome.unwrap_err();
        println!("{name} with a window of 1: {error}");
        assert!(error.to_string().contains("at least 2"), "{error}");
    }
}

#[test]
fn dilate_grows_and_erode_shrinks() {
    let mut source = single_channel(16, 16);
    // A 2x2 bright square.
    for y in 8..10 {
        for x in 8..10 {
            set(&mut source, x, y, 1.0);
        }
    }

    let mut grown = ImageBuf::empty().unwrap();
    algo::dilate(&mut grown, &source, 3, None, None).unwrap();
    assert_eq!(
        value(&grown, 7, 8),
        1.0,
        "dilating should reach the pixel outside the square"
    );

    let mut shrunk = ImageBuf::empty().unwrap();
    algo::erode(&mut shrunk, &source, 3, None, None).unwrap();
    assert_eq!(
        value(&shrunk, 8, 8),
        0.0,
        "eroding a 2x2 square with a 3x3 window should erase it"
    );
}

/// Every pixel of the result has to come from somewhere in the source.
/// OpenImageIO leaves -FLT_MAX or +FLT_MAX in pixels that had no source under
/// them, and calls that success.
///
/// The region has to reach outside the source for that to be possible at all;
/// with the default region it already equals the data window and the guard
/// that clamps it has nothing to do. So this asks for a region twice the size
/// of the image, which is the shape the guard exists for.
#[test]
fn dilate_and_erode_never_leave_a_float_extreme() {
    let mut source = single_channel(8, 8);
    algo::fill(&mut source, &[0.25], None).unwrap();
    let beyond = Roi::new(-8..16, -8..16, 0..1, 0..1).unwrap();

    let mut grown = ImageBuf::empty().unwrap();
    algo::dilate(&mut grown, &source, 3, None, Some(beyond)).unwrap();
    let stats = algo::pixel_stats(&grown, None).unwrap();
    println!("dilate range: {} to {}", stats.min[0], stats.max[0]);
    assert!(
        stats.min[0] > -1e30 && stats.max[0] < 1e30,
        "dilate left a float extreme: {} to {}",
        stats.min[0],
        stats.max[0]
    );

    let mut shrunk = ImageBuf::empty().unwrap();
    algo::erode(&mut shrunk, &source, 3, None, Some(beyond)).unwrap();
    let stats = algo::pixel_stats(&shrunk, None).unwrap();
    println!("erode range: {} to {}", stats.min[0], stats.max[0]);
    assert!(
        stats.min[0] > -1e30 && stats.max[0] < 1e30,
        "erode left a float extreme: {} to {}",
        stats.min[0],
        stats.max[0]
    );
}

#[test]
fn the_fourier_transform_round_trips() {
    let mut source = single_channel(16, 16);
    for y in 0..16 {
        for x in 0..16 {
            set(&mut source, x, y, ((x + y) % 4) as f32 / 4.0);
        }
    }

    let mut frequency = ImageBuf::empty().unwrap();
    algo::fft(&mut frequency, &source, None).unwrap();

    let spec = frequency.spec().unwrap();
    assert_eq!(
        spec.channel_count(),
        2,
        "a transform is real and imaginary, whatever the source held"
    );
    assert_eq!(spec.format(), PixelFormat::F32);

    let mut back = ImageBuf::empty().unwrap();
    algo::ifft(&mut back, &frequency, None).unwrap();

    for (x, y) in [(0, 0), (5, 7), (15, 15)] {
        let want = value(&source, x, y);
        let got = value(&back, x, y);
        assert!(
            (want - got).abs() < 1e-3,
            "a round trip should return the original at {x},{y}: {want} then {got}"
        );
    }
}

/// A buffer whose pixels are not in memory has no pixel address, and
/// OpenImageIO would dereference the null it gets back rather than report
/// anything: `hfft_` casts `src.pixeladdr(...)` to a complex pointer behind an
/// assertion that a release build compiles away.
///
/// OpenImageIO reads a small file into local storage eagerly, so a fixture of
/// a size worth writing here comes back `Local` and takes the normal path.
/// The guard covers the cache-backed case, which needs a file large enough for
/// OpenImageIO to leave in the cache; what is asserted here is that the
/// ordinary route through a file still works, and that a buffer with no pixels
/// at all is refused rather than crashing.
#[test]
fn the_inverse_transform_needs_pixels_in_memory() {
    let scratch = common::ScratchDir::new("ifft");
    let path = scratch.file("frequency.exr");

    let mut source = single_channel(8, 8);
    algo::fill(&mut source, &[0.5], None).unwrap();
    let mut frequency = ImageBuf::empty().unwrap();
    algo::fft(&mut frequency, &source, None).unwrap();
    frequency.write(&path).unwrap();

    let attached = ImageBuf::from_path(&path).unwrap();
    println!("a small file comes back as {:?}", attached.storage());
    let mut back = ImageBuf::empty().unwrap();
    algo::ifft(&mut back, &attached, None).unwrap();

    // A buffer that holds nothing at all is refused, not dereferenced.
    // `soundness_test` covers the rest of ifft's preconditions.
    let nothing = ImageBuf::empty().unwrap();
    let error = algo::ifft(&mut back, &nothing, None).unwrap_err();
    println!("a source with no pixels reported as: {error}");
    assert!(error.to_string().contains("no pixels"), "{error}");
}

#[test]
fn polar_and_complex_are_inverses() {
    let spec = ImageSpec::new(8, 8, 2, PixelFormat::F32).unwrap();
    let mut polar = ImageBuf::new(&spec).unwrap();
    // Magnitude 2, phase 1 radian, everywhere.
    algo::fill(&mut polar, &[2.0, 1.0], None).unwrap();

    let mut complex = ImageBuf::empty().unwrap();
    algo::polar_to_complex(&mut complex, &polar, None).unwrap();

    let roi = Roi::new(0..1, 0..1, 0..1, 0..2).unwrap();
    let mut values = [0.0_f32; 2];
    complex.get_pixels_into(roi, &mut values).unwrap();
    println!("2 at 1 radian is {values:?}");
    assert!((values[0] - 2.0 * 1.0_f32.cos()).abs() < 1e-5);
    assert!((values[1] - 2.0 * 1.0_f32.sin()).abs() < 1e-5);

    let mut round_trip = ImageBuf::empty().unwrap();
    algo::complex_to_polar(&mut round_trip, &complex, None).unwrap();
    let mut back = [0.0_f32; 2];
    round_trip.get_pixels_into(roi, &mut back).unwrap();
    println!("and back: {back:?}");
    assert!((back[0] - 2.0).abs() < 1e-5);
    assert!((back[1] - 1.0).abs() < 1e-5);
}

#[test]
fn the_polar_conversions_need_exactly_two_channels() {
    let three = single_channel(8, 8);
    let mut result = ImageBuf::empty().unwrap();
    assert!(algo::complex_to_polar(&mut result, &three, None).is_err());
    assert!(algo::polar_to_complex(&mut result, &three, None).is_err());
}
