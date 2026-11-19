# Releasing RavenSky Astro

The last published line is 0.6.2. The next proposed line is 0.7.0; its
preparation evidence and separate publication approval gate are in
[`docs/work/image-conversion-compression-v1-p7/`](docs/work/image-conversion-compression-v1-p7/).
The [draft release notes](docs/releases/RavenSkyAstro-0.7.0-draft.md) describe
the new APIs and support boundaries. Historical 0.6.0–0.6.2 release details
remain in the corresponding `docs/work/` records and the changelog.

## Before requesting publication approval

Review the complete source and package diff against `master`. Verify the
version and API compatibility assessment, all four `Cargo.toml` manifests,
the Rust 1.94 minimum, licenses, README examples, third-party notices and
archive members. Keep `astro-bench` unpublished. The package should not
redistribute the official XISF specification or include investigation
artifacts and developer configuration.

Run the relevant full workspace checks from a clean reviewed commit:

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

## After explicit approval

Publish the reviewed 0.7.0 crates in dependency order, waiting for each
version to be available on crates.io before the next dry-run and upload:

```sh
cargo publish --locked -p astro-io
cargo publish --locked -p astro-metadata
cargo publish --locked -p astro-metrics
cargo publish --locked -p ravensky-astro
```

The real four-crate publication requires explicit user authorization;
preparing or dry-running packages does not grant it. After publication, verify registry
archives and docs.rs, then obtain approval before a public tag or GitHub
release. AstroMuninn's pinned Git overrides may be replaced only after the
published crates resolve and qualify from the registry. Do not remove its
independent vendored `fitsio-sys` patch as part of that transition.
