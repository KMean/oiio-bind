//! The shims that take a caller-controlled message into a C++ string API.
//!
//! `oiio-sys` is published alongside `oiio` and these three are declared as
//! plain safe `fn`, so they have to hold up on their own. `oiio` itself never
//! calls them.

/// A `rust::Str` is a pointer and a length over a `&str`. It carries no NUL.
///
/// Passing `.data()` to `errorfmt(const char*)` made OpenImageIO scan from
/// there to the first NUL anywhere in the process, so any `&str` that is a
/// prefix of a larger allocation was read past its end and the extra bytes came
/// back in the message. `errorfmt` with no arguments is also a runtime format
/// call, so a `{}` in the caller's text threw `fmt::format_error` out of a shim
/// cxx declares `noexcept`, which is `std::terminate`.
#[test]
fn a_message_is_taken_by_length_and_never_used_as_a_format_string() {
    let path = std::env::temp_dir().join("oiio-sys-seterror-probe.exr");
    // The writer is only a place to hang a message; nothing is written.
    // SAFETY: a null ioproxy and an empty search path request the defaults.
    let mut output = unsafe {
        oiio_sys::imageio::imageoutput_create(path.to_str().unwrap(), std::ptr::null_mut(), "")
    };
    let owned = "marker-AAAA-BBBB-CCCC-DDDD-EEEE".to_owned();

    {
        let Some(writer) = output.as_mut() else {
            panic!("no EXR writer available");
        };
        // A format placeholder used to be std::terminate.
        oiio_sys::imageio::imageoutput_seterror(writer, "{}");
    }
    let message = oiio_sys::imageio::imageoutput_geterror(output.as_ref().unwrap(), true);
    assert_eq!(message.trim_end(), "{}");

    {
        // A prefix of a larger allocation used to read past its end.
        let writer = output.as_mut().unwrap();
        oiio_sys::imageio::imageoutput_seterror(writer, &owned[..6]);
    }
    let message = oiio_sys::imageio::imageoutput_geterror(output.as_ref().unwrap(), true);
    assert_eq!(message.trim_end(), "marker");

    // And the free function, which has no object to report through.
    oiio_sys::imageio::debug("{}");
    oiio_sys::imageio::debug(&owned[..6]);
}
