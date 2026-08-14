//! insert_samples / erase_samples must bound the pixel index in the shim.
//!
//! Unlike samples()/capacity()/data_ptr()/deep_value(), DeepData::insert_samples
//! and erase_samples index m_nsamples[pixel] / m_capacity[pixel] with no check
//! of their own, so an out-of-range pixel used to be a heap read and write past
//! those vectors. The shims now reject it, matching the guarded siblings. This
//! test reaches those two raw shims directly (the safe `oiio` crate never calls
//! them) and confirms an out-of-range pixel is a no-op, not a crash.

use oiio_sys::deepdata;
use oiio_sys::imageio;
use oiio_sys::typedesc::{self, BaseType};

#[test]
fn out_of_range_pixel_is_a_noop_not_a_heap_write() {
    let float = typedesc::typedesc_from_basetype_arraylen(BaseType::Float32, 0);
    let mut spec = imageio::imagespec_new(4, 4, 2, float);
    imageio::imagespec_set_deep(spec.pin_mut(), true);

    let mut deep = deepdata::deepdata_default();
    let mut error = String::new();
    assert!(deepdata::deepdata_init_from_spec(
        deep.pin_mut(),
        spec.as_ref().unwrap(),
        &mut error
    ));
    assert_eq!(deepdata::deepdata_pixels(deep.as_ref().unwrap()), 16);

    // A valid pixel: set two samples so there is state to protect.
    deepdata::deepdata_set_samples(deep.pin_mut(), 3, 2);
    assert_eq!(deepdata::deepdata_samples(deep.as_ref().unwrap(), 3), 2);

    // Out-of-range pixels, on both operations. If the guard were absent these
    // would index far past the 16-entry vectors; with it they are no-ops and
    // the process survives.
    deepdata::deepdata_insert_samples(deep.pin_mut(), 1_000_000, 0, 4);
    deepdata::deepdata_insert_samples(deep.pin_mut(), -1, 0, 4);
    deepdata::deepdata_erase_samples(deep.pin_mut(), 1_000_000, 0, 1);
    deepdata::deepdata_erase_samples(deep.pin_mut(), -5, 0, 1);

    // Survived, and the valid pixel is untouched.
    assert_eq!(deepdata::deepdata_pixels(deep.as_ref().unwrap()), 16);
    assert_eq!(deepdata::deepdata_samples(deep.as_ref().unwrap(), 3), 2);

    // An in-range insert still works.
    deepdata::deepdata_insert_samples(deep.pin_mut(), 3, 2, 3);
    assert_eq!(deepdata::deepdata_samples(deep.as_ref().unwrap(), 3), 5);
}
