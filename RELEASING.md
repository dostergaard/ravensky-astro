# Releasing RavenSky Astro

The next coordinated release is **0.6.0**, currently prepared for review on
`feature/file-validation`. The version introduces the additive file-validation
and resource-control APIs. Existing image-loader contracts remain unchanged.
`astro-bench` is a repository library/CLI with `publish = false`; it is not one
of the crates uploaded to crates.io. Its standalone usage is documented in the
[benchmark guide](docs/BenchmarkGuide.md).

## Review and verification

Keep implementation, benchmark evidence and release preparation as separate
commits. Review the complete PR against `master` before merging. Do not publish
from an unreviewed feature branch. The user has reserved this review step.

Verify the declared Rust 1.94 minimum with the committed lockfile:

```sh
cargo fmt --all --check
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
cargo test --locked --workspace --all-targets --all-features
cargo test --locked --workspace --all-features --doc
cargo build --locked --release --workspace --all-features --examples
RUSTDOCFLAGS='-D warnings' cargo doc --locked --workspace --all-features --no-deps
cargo package --locked --workspace --exclude astro-bench
```

The package command verifies all four archives with a temporary registry for the
unpublished local dependencies; it does not upload them. Registry access is needed:
Cargo 1.94's offline multi-package verification failed with `no hash listed` in
the temporary registry here; online verification passed. Use `--allow-dirty` only
for precommit inspection, then repeat from a clean reviewed checkout. Inspect
archive contents, including the adapted decoder's third-party notices. Raw
benchmark reports stay in Git and are excluded from the root crate archive.

The local-capture XISF loader test is explicitly ignored in normal runs because
its input is intentionally outside Git. Where that capture is available, run
`cargo test -p astro-io test_load_xisf_real_sample -- --ignored` separately.
Generated XISF loader and validator tests remain part of normal CI.

CI covers Linux x86-64 GNU, macOS ARM64 and Windows x86-64 GNU. This is build/test
coverage, not benchmark evidence for those machines. Windows MSVC still requires
the consuming application's patched backend build; GNU CI is not MSVC evidence.
AstroMuninn's vendored `fitsio-sys 0.5.5` must be checked separately from this
workspace's normal registry `0.5.7` dependency before application integration.

## Merge and publication after approval

1. Confirm all PR checks pass and the user approves the final diff. Merge into
   `master`, fetch and fast-forward the local branch. Keep rollback commits and
   evidence; delete the feature branch only after the merged result is verified.
2. Set the 0.6.0 changelog date to the actual release date, verify all workspace
   and internal dependency versions, and commit that release metadata. Repeat
   the clean package check above.
3. Publish in dependency order, allowing each version to appear in the registry
   before starting its dependents:

   ```sh
   cargo publish --locked -p astro-io
   cargo publish --locked -p astro-metadata
   cargo publish --locked -p astro-metrics
   cargo publish --locked -p ravensky-astro
   ```

4. Verify the four registry versions and docs.rs documentation. Tag the reviewed
   release commit `v0.6.0`, push the tag and publish release notes linking the
   public API, support limitations and benchmark guide. Do not move an existing
   release tag or attempt to republish an existing crate version.
5. Update AstroMuninn's registry dependency requirements and lockfile to 0.6.0;
   add its direct `astro-io` dependency when integrating validation. Test core,
   CLI and Lite with the vendored FITS backend and the actual release targets.
   Release jobs must resolve published versions without workspace path patches.
6. Implement the shared AstroMuninn monitor/resource scheduler under its approved
   design. Configuration remains manually editable until an editor is developed.

## Evidence for this release candidate

Local 0.6.0 checks passed: 131 workspace tests, four doctests, formatting, Clippy
with warnings denied, release builds and rustdoc. Two tests are explicitly ignored
in the standard suite (the local XISF capture and the existing SEP test); the XISF
capture test also passed when invoked explicitly. Nine Python investigation tests
passed. An isolated copy using AstroMuninn's unchanged vendored `fitsio-sys 0.5.5`
passed all 108 applicable `astro-io`, `astro-metadata` and `astro-bench` tests.
The normal registry dependency remains `fitsio-sys 0.5.7`. All four 0.6.0 packages
passed online Cargo verification before final archive-content cleanup; the clean
candidate check and CI results must be recorded before merge approval.

- [Managed compressed-FITS implementation](docs/CompressedFitsResourceImplementation.md)
- [Managed codec memory/scaling/cancellation](docs/benchmarks/2026-09-08-managed-fits/README.md)
- [Earlier synthetic, real-capture and storage investigation](docs/benchmarks/2026-09-07-m4-max-completion/README.md)

Unknown format extensions remain explicit `Unsupported` outcomes. Validation is
read-only and does not prevent a producer from changing a file afterward. Working
reservations are not an OS-enforced process memory limit. Library completion does
not establish adaptive application defaults or foreground responsiveness under
all workloads.
