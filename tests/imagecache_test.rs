/// The OIIO 3.x bridge must keep the cache alive through ImageBuf's
/// std::shared_ptr even after Rust drops its original cache handle.
#[test]
fn imagebuf_retains_its_imagecache() {
    let cache = oiio_sys::imagecache::imagecache_create(false);
    assert!(!cache.is_null());

    let image_path =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/images/test16.png");
    let imagebuf = unsafe {
        oiio_sys::imagebuf::imagebuf_new_from_file(
            image_path.to_str().expect("test path must be UTF-8"),
            0,
            0,
            cache.clone(),
            std::ptr::null(),
            std::ptr::null_mut(),
        )
    };
    assert!(!imagebuf.is_null());

    drop(cache);

    let retained_cache = oiio_sys::imagebuf::imagebuf_imagecache(
        imagebuf.as_ref().expect("ImageBuf must remain alive"),
    );
    assert!(!retained_cache.is_null());
}
