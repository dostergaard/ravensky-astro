# Releasing RavenSky Astro

The four public 0.7.0 crates were published to crates.io on 2026-11-20 in
dependency order. Their distinct source commits, archive hashes, registry
verification and docs.rs results are in
[`docs/work/image-conversion-compression-v1-p7/EVIDENCE.md`](docs/work/image-conversion-compression-v1-p7/EVIDENCE.md).
The [0.7.0 release notes](docs/releases/RavenSkyAstro-0.7.0.md) describe the
published APIs and support boundaries. `astro-bench` remains unpublished.
The `v0.7.0` tag and any GitHub Release require separate approval and have
not been created. Historical 0.6.0–0.6.2 release details remain in the
corresponding `docs/work/` records and the changelog.

## 0.7.0 qualification checklist

Review the complete source and package diff against `master`. Verify the
version and API compatibility assessment, all public `Cargo.toml` manifests,
the Rust 1.94 minimum, licenses, README examples, third-party notices and
archive members. Keep `astro-bench` unpublished. The package should not
redistribute the official XISF specification or include investigation
artifacts and developer configuration.

For reproduction, start from a clean reviewed commit and run the relevant
workspace checks below. The exact backend features, commands and observed
results for 0.7.0 are recorded in the P7-A evidence:

```sh
cargo +1.94.0 fmt --all --check
cargo +1.94.0 clippy --locked --workspace --all-targets --all-features -- -D warnings
cargo +1.94.0 test --locked --workspace --all-targets --all-features
cargo +1.94.0 test --locked --workspace --all-features --doc
cargo +1.94.0 build --locked --release --workspace --all-features --examples
RUSTDOCFLAGS='-D warnings' cargo +1.94.0 doc --locked --workspace --all-features --no-deps
cargo +1.94.0 package --locked --workspace --exclude astro-bench --target-dir target/package-070
```

Use a fresh target directory for each changed package candidate; an earlier
Cargo version left trailing bytes when an archive was overwritten after its
contents shrank. Inspect the generated `.crate` archives directly. The
`--workspace` package check uses a temporary local registry to verify the
four coordinated packages without uploading them. An ordinary crates.io
dry-run for downstream crates may remain blocked until their new dependency
versions are actually published. Record that distinction.

`astro-io`'s local-capture test is intentionally ignored in standard runs
because its source file is outside Git. CI covers macOS ARM64 and Linux
x86-64 GNU; Windows GNU coverage excludes `astro-metrics` and the facade
because `sep-sys` uses POSIX `rand_r`. A local macOS check does not replace a
green CI run on the candidate commit. AstroMuninn's vendored `fitsio-sys`
backend needs separate product qualification after the registry transition.

## Completed 0.7.0 publication and remaining gates

The four packages were approved and uploaded separately in this order:

1. `astro-io 0.7.0`
2. `astro-metadata 0.7.0`
3. `astro-metrics 0.7.0`
4. `ravensky-astro 0.7.0`

Each crates.io archive matched its approved package. All four docs.rs builds
succeeded, and an independent Rust 1.94 consumer compiled the complete
registry-only 0.7.0 graph. The coordinated tag should identify the final
reviewed release-closure commit on `master`; it cannot be the source commit
of every crate, because the archives were published from successive commits.
Obtain separate approval before creating or pushing `v0.7.0` or a GitHub
Release. AstroMuninn's pinned Git overrides may be replaced only in its
dedicated P7-B task. Retain its independent vendored `fitsio-sys` patch.
