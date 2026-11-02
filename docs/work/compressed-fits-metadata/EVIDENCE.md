# Compressed FITS metadata evidence

## 2026-11-02 baseline

- `cargo run --quiet --example dump_fits_metadata -- <Sh2-119 sample>`: completed; reported `Image Dimensions: 0x0 pixels`, no observational metadata, and only HDU 0 header cards.
- Repeated the same example for the Barnard 7 `.fit.fz` and Orion's Belt `.fits.fz` samples: same empty result.
- Read first two FITS headers directly without modifying files. All three: primary `NAXIS=0`; HDU 1 `XTENSION='BINTABLE'`, `ZIMAGE=T`, `ZCMPTYPE='GZIP_2'`, `ZBITPIX=16`, `ZNAXIS1=8288`, `ZNAXIS2=5648`. HDU 1 has `IMAGETYP=LIGHT`, `INSTRUME=Artemis-M PRO`, `GAIN=120`, `OFFSET=5`, `CCD-TEMP=-10`. Sh2-119 has `EXPTIME=120`, `FILTER=B`, `OBJECT=Sh2-119`; Barnard 7 has `EXPTIME=120`, `FILTER=R`, `OBJECT=Barnard_7`; Orion's Belt has `EXPTIME=300`, `FILTER=Ha`, `OBJECT=OrionsBelt`.
- At baseline, physical-header inspection alone did not establish payload
  integrity; later full-validation results are recorded below.

## Test-first and implementation results

- `cargo test -p astro-metadata --test compressed_fits -- --nocapture` initially failed for the expected defect: GZIP_2 metadata returned `(0, 0)` and an image-less primary was accepted. `fitsio` reported HDU 1 as `ImageInfo { shape: [2, 2], image_type: Short }`.
- After the shared change, the generated GZIP_2, ordinary primary image, later image after an auxiliary table, conflicting headers, missing optional metadata, and no-image tests pass. The loader reads the compact compressed image pixels correctly. Existing malformed compressed-layout/stream tests in `astro-io/tests/validation.rs` and `validation/compressed.rs` remain passing.
- AstroMuninn suffix and preview tests initially failed for `.fits.fz` and zero discovered files, then passed after full-path classification. Monitor discovery accepts `.fits.fz` and mixed-case `.fit.fz` and excludes `.fits.gz`.
- `ASTRO_NINA_FITS_FIXTURES=/Users/dean/projects-dev/RavenSky/ravensky-astro/tests/data/nina-compressed-fits cargo test -p astro-metadata --test compressed_fits --quiet`: 5 passed, including all three real files. Each yielded 8288×5648, expected camera/gain/offset/temperature, and its header-derived object/filter/exposure. Structural and full validation each reported one image. The full test took 8.96 seconds.
- `cargo run --quiet --example dump_fits_metadata -- tests/data/nina-compressed-fits/2026-11-01_18-04-05_Sh2-119_LIGHT_B_EXP_120.00s_0010.fits`: after the fix, printed Artemis-M PRO, 8288×5648, Sh2-119, 120 seconds, LIGHT, B, physical cards for HDU 0 and HDU 1, and selected logical image information.
- With temporary `patch.crates-io.astro-io.path="../ravensky-astro/astro-io"` and `patch.crates-io.astro-metadata.path="../ravensky-astro/astro-metadata"`, the AstroMuninn real-file preview/image-type test passed for all three samples. The original `Cargo.lock` was backed up and restored byte-for-byte afterward.

## Final checks

- Shared repo: `cargo fmt --all --check`, `cargo check --workspace --all-targets`, and `cargo test --workspace --quiet` passed. The workspace test run included 52 astro-io unit tests, 45 validation tests, 19 astro-metadata unit tests, 5 compressed-FITS integration tests (without the optional real-file environment), and other crate/example tests; two existing tests were ignored.
- Shared strict `cargo clippy --workspace --all-targets --all-features -- -D warnings` failed on existing `chunks_exact_to_as_chunks` warnings in `astro-bench/tests/workloads.rs` and `unnecessary_sort_by` warnings in unchanged example code. The same command with only those two lint categories allowed passed. A narrower package Clippy invocation hit an existing `fitsio-sys` autotools sandbox permission error; the full-workspace invocation completed and checked the changed crates.
- AstroMuninn with temporary local shared-crate patches: `cargo check --workspace --all-targets`, `cargo test --workspace --quiet`, and offline `cargo clippy --workspace --all-targets --all-features -- -D warnings` passed. The final test run set `ASTRO_NINA_FITS_FIXTURES` and included real-file product assertions; core had 153 passing tests and one pre-existing ignored test. CLI and Lite targets compiled and passed their available tests. The original dirty lockfile was restored after each patched run.
- Both repositories: `cargo fmt --all --check` and `git diff --check` passed. `git check-ignore` confirms all three 23–28 MB capture files remain ignored. No Kani harness was added: the change uses safe HDU enumeration and existing CFITSIO APIs; validation geometry and codec code were not changed.

## Remaining limits

- The three supplied captures cover one N.I.N.A. GZIP_2 layout (empty primary, image in HDU 1). Compact tests cover a later image at HDU 2 and precedence. No same-session uncompressed N.I.N.A. control was supplied; an ordinary generated primary-image regression test passed.
- Routine metadata extraction reads headers but does not verify checksums or decode image payloads. Full validation was separately run for the supplied captures.
- A standalone AstroMuninn build still resolves published 0.6.1 shared crates until a coordinated release and dependency update; the local patched build verifies the intended integrated behavior.
- No before/after metadata latency or peak-memory benchmark was run. The
  selection path enumerates HDUs and reads headers only; full pixel loading and
  validation retain their separate resource behavior. No throughput or RSS
  improvement is claimed.
