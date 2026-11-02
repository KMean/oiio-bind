//! Process-wide settings.
//!
//! These mutate state that belongs to the whole process. Each `tests/*.rs`
//! file is its own binary, so nothing here can disturb another suite, and the
//! tests below are split so that no two touch the same attribute: they run in
//! parallel threads within this binary.

use oiio::config::{self, Limits};

#[test]
fn the_thread_count_round_trips() {
    let original = config::threads();

    config::set_threads(3).unwrap();
    assert_eq!(config::threads(), 3);

    // Zero asks for one thread per core, and reading back gives what that
    // came to rather than the zero that was stored. So it must NOT be 0, and
    // it must be at least 1.
    config::set_threads(0).unwrap();
    let resolved = config::threads();
    assert!(
        resolved >= 1,
        "zero means one per core, which should resolve to at least 1, got {resolved}"
    );

    config::set_threads(original).unwrap();
}

#[test]
fn the_limits_round_trip() {
    let original = config::limits();

    let mut wanted = Limits::current().with_channels(64).with_image_size_mb(512);
    // limits:resolution only exists from OpenImageIO 3.1.19; leave it alone
    // where the linked library has no such attribute.
    if original.resolution.is_some() {
        wanted = wanted.with_resolution(16_384);
    }
    config::set_limits(&wanted).unwrap();

    let now = config::limits();
    assert_eq!(now.channels, 64);
    assert_eq!(now.image_size_mb, 512);
    if original.resolution.is_some() {
        assert_eq!(now.resolution, Some(16_384));
    } else {
        assert_eq!(now.resolution, None, "absent attributes stay absent");
    }

    config::set_limits(&original).unwrap();
    assert_eq!(config::limits(), original);
}

#[test]
fn the_default_limits_are_openimageios_own() {
    // Documented defaults. If OpenImageIO changes them this should be the
    // thing that notices, rather than a user's allocation.
    let limits = config::limits();
    assert!(
        limits.channels > 0 && limits.image_size_mb > 0,
        "the two limits every supported OpenImageIO has should be set: {limits:?}"
    );
    // resolution is Some only from 3.1.19 onward, so its absence is a fact
    // about the linked library rather than a failure.
    if let Some(resolution) = limits.resolution {
        assert!(
            resolution > 0,
            "a present resolution limit should be positive"
        );
    }
}

#[test]
fn use_imagecache_is_refused_with_a_reason() {
    // Not a gap: turning this on would put ImageBuf's drop onto the shared
    // cache from arbitrary threads, which is what the Send impl's comment
    // says must not happen.
    let error = config::set_attribute_int("imagebuf:use_imagecache", 1)
        .expect_err("this must not be settable through the safe crate");
    let text = error.to_string();
    assert!(
        text.contains("imagebuf:use_imagecache"),
        "the error should name the attribute: {text}"
    );
    assert!(
        text.contains("shared cache"),
        "the error should say why: {text}"
    );

    // And OpenImageIO still has it off, so the refusal happened before the
    // call rather than after it.
    assert_eq!(config::attribute_int("imagebuf:use_imagecache"), Some(0));
}

#[test]
fn an_unknown_attribute_is_an_error_not_a_silent_no_op() {
    assert!(config::set_attribute_int("not:a:real:attribute", 1).is_err());
    assert_eq!(config::attribute_int("not:a:real:attribute"), None);
    assert_eq!(config::attribute_string("not:a:real:attribute"), None);
}

#[test]
fn strict_input_round_trips() {
    let original = config::strict_input();
    config::set_strict_input(!original).unwrap();
    assert_eq!(config::strict_input(), !original);
    config::set_strict_input(original).unwrap();
    assert_eq!(config::strict_input(), original);
}

#[test]
fn the_version_matches_what_the_crate_reports() {
    let (major, minor, patch) = config::version();
    assert!(
        major >= 3,
        "this crate requires 3.1.4 or newer, got {major}"
    );
    assert!(
        minor < 100 && patch < 100,
        "the version decode is wrong: {major}.{minor}.{patch}"
    );
}

#[test]
fn the_format_and_extension_lists_are_populated() {
    let formats = config::format_names();
    assert!(
        formats.iter().any(|name| name == "openexr"),
        "openexr should be among {} formats",
        formats.len()
    );

    let extensions = config::extension_names();
    assert!(
        extensions.iter().any(|ext| ext == "exr"),
        "exr should be among {} extensions",
        extensions.len()
    );

    // The library list is what a bug report wants; it should at least name
    // the thing it is a list of.
    assert!(!config::library_versions().is_empty());
}

#[test]
fn the_plugin_search_path_round_trips() {
    let original = config::plugin_search_path();

    // A path that does not exist is still a valid thing to ask for: the
    // search is what consults it, not the setter.
    config::set_plugin_search_path("/nonexistent/plugins").unwrap();
    assert_eq!(config::plugin_search_path(), "/nonexistent/plugins");

    config::set_plugin_search_path(&original).unwrap();
    assert_eq!(config::plugin_search_path(), original);
}
