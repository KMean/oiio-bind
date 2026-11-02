# Publishing to crates.io

**Done: both crates were published at 0.1.0 on 2026-08-15**, then `oiio`
0.1.1, both at 0.2.0, and `oiio` 0.2.1. The checklist further down is the
0.1.0 one, kept for its reasoning; the current release is below.

Remember that publishing is irreversible: a published version can be yanked,
but never replaced or deleted.

## Ready to publish: `oiio` 0.2.3 and `oiio-sys` 0.3.0

Prepared 2026-11-02. **`oiio-sys` takes a MINOR bump, not a patch.** Three
shims gained a required parameter when the exception boundary landed, so
0.2.1 -> 0.2.2 would promise a compatibility this release does not keep and
would break anyone pinned to `oiio-sys = "0.2"`. `oiio` itself is purely
additive, so it is a patch: 0.2.2 -> 0.2.3.

This was decided by reading the diff, because `cargo-semver-checks` is not
installed. **Installing it in CI is the gate this project still lacks** and
is the cheapest remaining release-engineering fix: nothing today can tell a
maintainer a change is breaking except a human reading a 103 KB `algo.rs`.

Gates, all run on 2026-11-02 at these versions:

- [x] `cargo fmt --all --check`
- [x] `cargo clippy --workspace --all-targets -- -D warnings`
- [x] `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps`
- [x] `DOCS_RS=1 cargo doc --workspace --no-deps` -- the one that proves
      docs.rs can build without linking OpenImageIO
- [x] `cargo test --workspace --all-targets` AND `cargo test --workspace
      --doc`. **`--all-targets` does NOT include doctests**, so running
      only that leaves every documented example unverified. CI's
      `cargo test --workspace` does include them; a local gate using
      `--all-targets` is weaker than CI, which is the wrong way round.
      At 0.2.3 this was **400 pass**, debug and
      release, against the vcpkg 3.1.12.0 on this machine, with
      `OIIO_BIND_TEST_IMAGES` and `OIIO_BIND_TEST_EXR_IMAGES` set so the
      corpus suites actually run rather than skipping
- [x] `cargo package -p oiio-sys`: packages **and verifies**, 38 files,
      84.0 KiB compressed, compiling from its own directory
- [x] README checked for staleness before publishing, since crates.io
      renders the packaged copy immutably. Its CI paragraph claimed 3.1.14
      on all three platforms; corrected to say Linux and macOS run both
      lines and Windows runs 3.1.14 only.
- [ ] `cargo package -p oiio` cannot verify until `oiio-sys` 0.3.0 is on the
      index. Confirmed failing with the expected "location searched:
      crates.io index". Expected, not a fault -- see **Order** below.

Then, in order:

```bash
cargo publish -p oiio-sys
```

Wait for the index, then:

```bash
cargo publish -p oiio
```

Then tag `v0.2.3` and push the tag.

**If `cargo publish` returns 403**, the token has expired. Run `cargo login`
and paste a new one from crates.io with the `publish-update` scope covering
both crates. Never paste a token into a chat.
## A recurring cost: `TextureLookupOptions`

`TextureLookupOptions` is a shared struct in the cxx bridge, so it is public
and constructible, and cxx generates it rather than letting it be marked
`#[non_exhaustive]`. Every texture option this crate adds therefore breaks
`oiio-sys` and costs it a minor bump, even when the `oiio` change is purely
additive. Adding anisotropy and conservative filtering on 2026-11-02 is the
first instance: `oiio-sys` needs 0.4.0 while `oiio` needs nothing.

`cargo-semver-checks` does catch this one, reporting
`constructible_struct_adds_field`, so it does not need watching for by hand.
Worth knowing when planning a release, not worth restructuring the bridge
over.
## What the semver gate does not catch

`cargo-semver-checks` runs in CI as of 2026-11-02 and is a floor, not a
proof. Measured against the 0.3.0 batch: it caught the `#[non_exhaustive]`
additions, and it did NOT catch `channel_count` changing from `i32` and
`usize` to `u32`.

Of its 261 lints at v0.51.0, the return-type ones are
`inherent_method_now_returns_unit`, `function_now_returns_unit` and the
`*_return_value_added` family: they cover `-> T` becoming `-> ()` and the
reverse, and nothing else. **A return type changing from one concrete type
to another passes silently.** So does a parameter type changing, as opposed
to the parameter count, which `function_parameter_count_changed` does catch.

Read the diff for signature changes. The gate is good at structural breaks:
items removed, arity changed, a struct closed, a trait impl dropped.
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

## Publishing gate (agreed with Kim, 2026-08-14)

Hold 0.1.0 until the runs come back boring: several consecutive CI runs
across a few days with zero new findings and zero API movement. Each CI run
re-rolls the property tests against the 3.1.14 builds this machine cannot
reproduce, so quiet runs are the evidence that the IBAprep
uninitialised-destination class is exhausted. The known members are refused
by construction as of this date.

**Correction, 2026-10-31.** The reasoning in that last sentence does not
hold, and should not be reused for a future release. Two faults. The class
was misdiagnosed: it is not uninitialised destination memory but an unchecked
read past a narrower *source*, traced in `upstream-issues.md` under the open
observation. And quiet runs cannot evidence exhaustion, because the binding
now refuses those shapes and `check_result` returns early on `Err`, so the
property test never reaches them — and its criterion is plausibility
(`is_finite() && abs() < 1e30`), not correctness, so an over-read landing on
real neighbouring data passes silently either way. The refusals stand; the
argument for them is the source, not the quiet runs.
