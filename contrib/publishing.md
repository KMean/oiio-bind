# Publishing to crates.io

Neither `oiio` nor `oiio-sys` has ever been published; both names are free.
This is the first release, and both crates go out at 0.1.0.

Everything below has been checked except the two `cargo publish` commands
themselves, which need a crates.io token and are irreversible: a published
version can be yanked, but never replaced or deleted.

## Order

`oiio-sys` first, then `oiio`. The safe crate depends on the sys crate by
version as well as by path, so `cargo package -p oiio` fails until `oiio-sys`
is on the index:

```
no matching package named `oiio-sys` found
location searched: crates.io index
```

That is expected before the first publish, not a fault in the manifest.

## Before publishing

- [x] Both manifests carry the same version, 0.1.0.
- [x] `cargo package -p oiio-sys` packages and **verifies**: the packaged crate
      compiles from its own directory, so the C++ sources and headers are all
      included. 36 files, 54 KiB compressed.
- [x] `anyhow` moved to `[dev-dependencies]`. It was only ever used by the
      integration tests, and a published crate should not impose it.
- [x] `DOCS_RS=1 cargo doc` succeeds. `oiio-sys/build.rs` skips discovery and
      linking when `DOCS_RS` is set, which is what lets docs.rs build a crate
      it cannot link OpenImageIO for.
- [x] `cargo fmt --check`, `cargo clippy -- -D warnings` and
      `RUSTDOCFLAGS="-D warnings" cargo doc` are all clean.
- [x] The full suite passes in debug and release, and against the OpenImageIO
      and OpenEXR image corpora.

## Publishing

```bash
cargo publish -p oiio-sys
```

Wait for the index to update — usually under a minute — then:

```bash
cargo publish -p oiio
```

## Afterwards

- The README says in two places that the crates are not on crates.io yet, and
  points at the repository for dependency instructions. Both need updating once
  they are.
- Tag the release and push the tag.
- Upstream `vfx-rs/oiio-bind` has not had a code push since May 2024. A
  courtesy issue there, saying this fork is published and under these names,
  was suggested earlier and has not been done.

## What a dependent needs

Publishing does not make this crate build anywhere: it needs OpenImageIO 3.1.4
or newer installed, found through vcpkg on Windows or `pkg-config` on Unix.
The README's Usage and Troubleshooting sections cover that, and are the first
thing someone arriving from crates.io will read.
