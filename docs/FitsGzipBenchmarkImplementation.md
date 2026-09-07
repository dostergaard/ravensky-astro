# Compressed FITS benchmark coverage

Status: implemented and locally verified; the
[216-sample baseline](benchmarks/2026-09-06-m4-max-fits-gzip/README.md) is recorded.

## Objective and scope

Measure the bounded integer FITS GZIP validator committed in `28979fc`. Add
deterministic GZIP_1/GZIP_2 fixtures with selectable full-width tile heights to
astro-bench. Establish current throughput/RSS and scaling across row-sized and
whole-image tiles before extending decoder coverage or tuning defaults.
No production validator changes, native allocation hooks, scheduler or calibration
UI are included. These measurements do not establish the other native codecs'
memory behavior or historical speedups.

## Current state and design

The harness already owns scratch quotas, deterministic UInt16 pixels, isolated
sample children, shared validator reservations, hashes and JSON reports. Extend
`Encoding` with `FitsGzip` and `FitsGzip2`; add optional `Recipe::tile_rows` and
`--tile-rows` (default: whole-image tile). Preserve existing recipes and file bytes.
New compressed-FITS fixtures use generator version 2; original encodings retain 1.

Write an empty primary HDU and a binary table with one Q byte descriptor per tile.
Stream each GZIP member directly through the quota writer, then fill the reserved
descriptors and header using bounded seeks. No compressed or decoded tile is held
in memory. Generate signed FITS samples with BZERO=32768 to preserve the logical
UInt16 values used by other recipes. GZIP_2 visits each tile's deterministic pixel
sequence twice for byte-plane order, retaining only cloned generator state and a
64 KiB chunk. This adds preparation CPU, outside timing, without an image buffer.

Preflight disk use includes per-tile descriptors and conservative GZIP expansion,
headers/trailers and HDU padding. Limit to 16,384 tiles/image and existing image,
frame and scratch bounds. Every append remains quota-controlled; backpatching
only replaces reserved bytes. No new dependency or native call is used in
generation/measurement; use existing CFITSIO in dev-only cross-reader tests.
Generation is single-threaded and checks cancellation between chunks/tiles.
Each measured worker owns one validation; the existing shared 512 MiB allowance
and bounded results queue apply. Add a 2 MiB decoder allowance to GZIP full-work
preflight, plus existing 16 MiB/worker overhead. This is admission, not an RSS cap.

## Implementation and verification

1. Test reproducibility, logical-pixel parity against CFITSIO, tile shapes and
   partial edges, validation coverage, invalid options/quotas and old manifests.
2. Extend generation, CLI, admission and format/checksum accounting; document
   metadata differences and fixture versions.
3. Run fmt, Clippy, tests/doctests, docs and release build. Run CLI coverage for
   new options and failure cleanup using existing tests/harness.
4. Record 24 configurations: two encodings × two tile heights × two patterns ×
   two sizes for full validation (16), plus read/structural for both encodings and
   tile heights at small/noise (8). Each uses workers 1/2/4 and three repetitions:
   216 isolated samples. Sizes: 8 MiB/image × 8 files and 32 MiB/image × 4 files.

## Measurement acceptance and limitations

Before running, require every sample to complete exact file accounting, release
reservations and remain within the declared shared allowance. Initial memory
target: four-worker full validation stays below 64 MiB process peak RSS for these
fixtures; investigate any higher result. No throughput improvement target exists
without an equivalent compressed-FITS baseline. Report medians, variation, worker
scaling and row/whole-image differences, including regressions or saturation.

Record source fingerprint, revision/compiler, hardware/storage, recipe hashes and
cache/load conditions. Run benchmarks without concurrent builds/tests. Warm-cache
local measurements do not determine storage speed, foreground responsiveness or
release defaults. Windows/Linux, real captures, pressure/remote-storage tests and
other native-codec resource controls remain open.

## Completed verification

New-encoding tests initially failed on unsupported recipe variants, then passed
after implementation. Coverage verifies deterministic generation, CFITSIO pixel
parity for both encodings/patterns with whole, row and partial-edge tiles, all
three workloads, invalid tile counts/options, quota exhaustion during writes,
cancellation between chunks, CLI failure cleanup and unchanged version-one FITS
bytes against a previously recorded fixture hash. Original manifests still open.

Passed: `cargo fmt --all -- --check`, workspace Clippy/all targets/all features
with warnings denied, 116 workspace all-target tests, four doctests, documentation
with warnings denied and the all-feature release build. One existing SEP test
remains ignored. Cargo checks used cached dependencies with `--offline`.

The matrix passed expected-file/byte accounting and shared-budget checks. Every
report matched an independent recomputation of the measured-source fingerprint.
Peak RSS stayed below 7.83 MiB; four-worker whole-image validation showed
2.73–3.45× median speedup over one worker. Row-tile scaling saturated or regressed
in several cases, and one timing group contained a large outlier. Preserve those
results and profile tile overhead before choosing defaults. No production
validator or downstream application change is needed for this benchmark extension.
