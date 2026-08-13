use std::{
    env, fs,
    path::{Path, PathBuf},
};

use anyhow::{bail, Context, Result};

const NAMES: &[&str] = &[
    "color",
    "deepdata",
    "filesystem",
    "imagebuf",
    "imagebufalgo",
    "imagecache",
    "texture",
    "imageio",
    "typedesc",
];

fn main() -> Result<()> {
    // Skip linking on docs.rs: https://docs.rs/about/builds#detecting-docsrs
    let building_docs = std::env::var("DOCS_RS").is_ok();
    if building_docs {
        println!("cargo:rustc-cfg=docsrs");
        return Ok(());
    }

    emit_discovery_rerun_directives();

    let include_paths = discover_openimageio()?;
    emit_oiio_header_rerun_directives(&include_paths);

    let mut build = cxx_build::bridges(NAMES.iter().map(|s| format!("src/{s}.rs")));
    build
        .files(NAMES.iter().map(|s| format!("src/ffi_{s}.cpp")))
        .includes(&include_paths);

    build.std("c++17");

    if env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc") {
        build.flag_if_supported("/utf-8").flag_if_supported("/EHsc");
    }

    build.compile("oiio-sys");

    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=src/lib.rs");
    println!("cargo:rerun-if-changed=src/ffi_pixel.h");

    for name in NAMES {
        println!("cargo:rerun-if-changed=src/{name}.rs");
        println!("cargo:rerun-if-changed=src/ffi_{name}.cpp");
        println!("cargo:rerun-if-changed=src/ffi_{name}.h");
    }

    Ok(())
}

fn emit_discovery_rerun_directives() {
    for name in [
        "OIIO_ROOT",
        "OIIO_INCLUDE_DIR",
        "OIIO_LIBRARY_DIR",
        "OIIO_DLL_DIR",
        "OIIO_VCPKG_TRIPLET",
        "VCPKG_ROOT",
        "VCPKGRS_TRIPLET",
        "VCPKG_TARGET_TRIPLET",
        "VCPKG_DEFAULT_TRIPLET",
        "VCPKGRS_DYNAMIC",
    ] {
        println!("cargo:rerun-if-env-changed={name}");
    }
}

fn emit_oiio_header_rerun_directives(include_paths: &[PathBuf]) {
    for include_path in include_paths {
        let version_header = include_path.join("OpenImageIO/oiioversion.h");
        if version_header.is_file() {
            println!("cargo:rerun-if-changed={}", version_header.display());
        }
    }
}

fn discover_openimageio() -> Result<Vec<PathBuf>> {
    if let Some(paths) = discover_from_overrides()? {
        return Ok(paths);
    }

    if env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows")
        && env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc")
    {
        return discover_with_vcpkg();
    }

    let library = pkg_config::Config::new().probe("OpenImageIO").context(
        "OpenImageIO was not found with pkg-config; set OIIO_ROOT or the \
             OIIO_INCLUDE_DIR and OIIO_LIBRARY_DIR overrides for a custom install",
    )?;
    Ok(library.include_paths)
}

fn discover_from_overrides() -> Result<Option<Vec<PathBuf>>> {
    let root = env::var_os("OIIO_ROOT").map(PathBuf::from);
    let include_dir = env::var_os("OIIO_INCLUDE_DIR")
        .map(PathBuf::from)
        .or_else(|| root.as_ref().map(|path| path.join("include")));
    let library_dir = env::var_os("OIIO_LIBRARY_DIR")
        .map(PathBuf::from)
        .or_else(|| root.as_ref().map(|path| path.join("lib")));

    if include_dir.is_none() && library_dir.is_none() {
        return Ok(None);
    }

    let (Some(include_dir), Some(library_dir)) = (include_dir, library_dir) else {
        bail!(
            "custom OpenImageIO discovery requires both OIIO_INCLUDE_DIR and \
             OIIO_LIBRARY_DIR (or OIIO_ROOT containing include/ and lib/)"
        );
    };

    if !include_dir.join("OpenImageIO/imageio.h").is_file() {
        bail!(
            "OIIO include directory does not contain OpenImageIO/imageio.h: {}",
            include_dir.display()
        );
    }
    if !library_dir.is_dir() {
        bail!(
            "OIIO library directory does not exist: {}",
            library_dir.display()
        );
    }

    println!("cargo:rustc-link-search=native={}", library_dir.display());
    println!("cargo:rustc-link-lib=OpenImageIO");
    println!("cargo:rustc-link-lib=OpenImageIO_Util");

    if env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let dll_dir = env::var_os("OIIO_DLL_DIR")
            .map(PathBuf::from)
            .or_else(|| root.map(|path| path.join("bin")));
        if let Some(dll_dir) = dll_dir.filter(|path| path.is_dir()) {
            stage_windows_dlls(&dll_dir)?;
        }
    }

    Ok(Some(vec![include_dir]))
}

fn stage_windows_dlls(dll_dir: &Path) -> Result<()> {
    let out_dir = PathBuf::from(env::var_os("OUT_DIR").context("OUT_DIR is not set")?);

    for entry in fs::read_dir(dll_dir)
        .with_context(|| format!("failed to read OIIO DLL directory {}", dll_dir.display()))?
    {
        let source = entry?.path();
        let is_dll = source
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| extension.eq_ignore_ascii_case("dll"));
        if !is_dll {
            continue;
        }

        let destination = out_dir.join(
            source
                .file_name()
                .context("an OIIO DLL path has no file name")?,
        );
        fs::copy(&source, &destination).with_context(|| {
            format!(
                "failed to stage OIIO runtime DLL {} to {}",
                source.display(),
                destination.display()
            )
        })?;
        println!("cargo:rerun-if-changed={}", source.display());
    }

    println!("cargo:rustc-link-search=native={}", out_dir.display());
    Ok(())
}

fn discover_with_vcpkg() -> Result<Vec<PathBuf>> {
    let triplet = env::var("OIIO_VCPKG_TRIPLET")
        .or_else(|_| env::var("VCPKGRS_TRIPLET"))
        .or_else(|_| env::var("VCPKG_TARGET_TRIPLET"))
        .or_else(|_| env::var("VCPKG_DEFAULT_TRIPLET"))
        .unwrap_or_else(|_| default_windows_triplet());

    // vcpkg-rs requires an explicit opt-in for DLL triplets. Selecting a
    // dynamic OIIO triplet here is itself that opt-in, and is the practical
    // default for OIIO's plugin-based image format support on Windows.
    if !triplet.contains("-static") && env::var_os("VCPKGRS_DYNAMIC").is_none() {
        env::set_var("VCPKGRS_DYNAMIC", "1");
    }

    let mut config = vcpkg::Config::new();
    config.emit_includes(true).target_triplet(&triplet);
    let library = config.find_package("openimageio").with_context(|| {
        format!(
            "OpenImageIO was not found in vcpkg for triplet {triplet}; install \
             openimageio:{triplet}, set OIIO_VCPKG_TRIPLET, or use OIIO_ROOT"
        )
    })?;

    Ok(library.include_paths)
}

fn default_windows_triplet() -> String {
    let target_arch = env::var("CARGO_CFG_TARGET_ARCH");
    let architecture = match target_arch.as_deref() {
        Ok("x86_64") => "x64",
        Ok("x86") => "x86",
        Ok("aarch64") => "arm64",
        Ok(other) => other,
        Err(_) => "x64",
    };
    format!("{architecture}-windows")
}
