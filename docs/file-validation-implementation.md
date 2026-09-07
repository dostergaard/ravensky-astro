# File validation implementation strategy

## Objective
Add a read-only `astro_io::validation` API for structural and full FITS/XISF validation, independent of consuming applications.

## Constraints and current state
Use the existing FITS backend and XML/deflate dependencies. Existing loaders and metadata APIs remain unchanged. Base this branch on the published 0.5.0 revision, retaining the coordinate metadata work it contains. No release or consumer integration is included in this change.

## Design and affected areas
Introduce a validation module with typed outcomes, source observations, cancellation, and explicit resource limits. Separate FITS layout traversal from XISF XML/block traversal and common bounded I/O. Full validation reads payloads, decodes supported compression, and verifies available checksums. Unsupported layouts and algorithms fail explicitly rather than claiming partial success. Document exact supported coverage and limits in astro-io's README.

## Execution
1. Add failing integration tests for defaults, truncation, cancellation, and structural/full differences.
2. Implement common API and FITS structural/full paths.
3. Add fixtures for XISF layout, encodings, compression, checksums, and auxiliary blocks; implement each path.
4. Add regression cases for limits, invalid descriptors, source changes, and backend coverage.
5. Run formatting, Clippy, tests, docs, and builds; review the diff and consumer compatibility.

## Verification
Use generated temporary files and independent codec/checksum fixtures. Test all HDUs/blocks, not only primary images. Confirm read-only behavior and resource/cancellation outcomes. Run the complete shared workspace test suite after focused astro-io checks.

## Risks and remaining details
A stable, valid file can still be modified by its producer after validation. Blocking filesystem/native calls cannot be cancelled instantly. Native decoder allocation limits must be handled explicitly. Unsupported features must never be mislabeled as corruption or silently accepted in full mode. Real capture fixtures and other-platform validation remain release evidence requirements.

## Implementation and verification result

Implemented on `feature/file-validation`, based on `dbf4bba` (published 0.5.0).
The additive API, support matrix, unsupported cases and resource/native limits
are documented in `astro-io/README.md`. This change does not integrate the
validator into AstroMuninn or publish a crate version.

Verified on macOS:

- `cargo fmt --all -- --check`
- `cargo clippy --offline --workspace --all-targets --all-features -- -D warnings`
- `cargo test --offline --workspace --all-targets --all-features`: 83 passed;
  one pre-existing SEP test remains ignored.
- `cargo test --offline --workspace --all-features --doc`: two passed.
- `RUSTDOCFLAGS='-D warnings' cargo doc --offline --workspace --all-features --no-deps`
- `cargo build --offline --workspace --all-features --release`
- `git diff --check`

The validator contributes 25 integration tests and four unit tests. Fixture
coverage includes all five supported FITS compression families, every supported
XISF codec with/without shuffling, the specification's embedded zlib example,
published SHA vectors, local references, source changes and cancellation.
CFITSIO's first native release build required running outside the filesystem
sandbox; the approved retry and subsequent final build succeeded.

Implementation is locally verified. Real capture-file compatibility and
Windows/Linux execution remain rollout gates. The consumer must still add its
monitor/config integration and readiness/execution contract tests separately.

## Resource-aware concurrency follow-up (2026-09-06)

Status: initial shared reservations and zlib/Zstandard streaming implemented and
locally verified; see [resource implementation](ValidationResourceImplementation.md)
and the [comparison measurements](benchmarks/2026-09-06-m4-max-streaming/README.md).
The baseline gaps and approved direction below describe the pre-change state;
native controls and consumer integration remain follow-up work.
The functional verification above applies to the first validator implementation;
it does not establish peak-memory bounds or adaptive scheduling performance.

### Objective and constraints

Support efficient concurrent callers with explicit shared reservations, streamed
validation and verified backend constraints. Preserve the existing standalone
API and validation guarantees. Product scheduling, OS telemetry/QoS, quiet periods
and retries stay in AstroMuninn. Serial mode is a baseline/diagnostic option;
release concurrency is bounded by measured workload and runtime resources.

### Baseline gaps and approved design

`validation/mod.rs` estimates memory without lifetime-owned reservations;
`validation/xisf.rs` retains entire decoded subblocks and builds an allocated XML
tree; `validation/fits.rs` estimates CFITSIO memory without an enforced native
quota. Audit these paths and dependencies before promising OOM recovery.

Add a caller-owned resource-control path with bounded preparation, per-stage
requirements and reservations released with their buffers on every exit. Keep
capacity contention distinct from a file exceeding configured limits; waiting
belongs outside workers. Cover parser/inline buffers, capacities/reallocation
peaks, backend history, native overhead and nested threads. Stream zlib/Zstandard
attachments/output; explicitly bound LZ4 blocks and CFITSIO tiles. Preserve
checksum-before-decode, exact output/extents and source-change checks. Reduce
redundant I/O only when whole-file coverage remains demonstrable.

### Execution and affected areas

1. Establish the shared `astro-bench` synthetic fixture/runner foundation and
   record serial/fixed-concurrency baselines using the existing validator; see
   [Benchmark and Calibration Design](BenchmarkAndCalibrationDesign.md).
   Document capabilities and missing native/allocation evidence.
2. Specify the additive resource API in `astro-io`, with deterministic tests for
   simultaneous callers, capacity rejection, cleanup, cancellation and deadlocks.
3. Implement managed reservations and streaming in the validation modules;
   reduce header/inline duplication and audit native allocations/threading.
4. Verify capability/exclusivity handling across existing CFITSIO loader and
   metadata callers before relaxing platform restrictions. Coordinate any required
   consumer changes explicitly; never protect only the new validation entry point.
5. Extend the benchmark matrix to remaining codecs/real captures, compare
   before/after with the same versioned recipes, and update
   public guarantees. Integrate the product scheduler separately once controls
   are usable; no new release manifest/version is implied by this design update.

### Verification and rollout

Measure release builds on representative small/large/mixed FITS/XISF workloads,
all supported codecs, checksums, large single blocks and many subblocks. Compare
serial/fixed concurrency and later consumer automatic mode on local/shared/remote
storage, lower-memory and larger systems, under idle and competing application
loads. Record fixture/build provenance, cache state and repeated-run variance;
measure peak RSS, managed/native allocations, bytes read, throughput, file latency,
cancellation and foreground responsiveness. Keep performance benchmarks separate
from deterministic resource-accounting tests. Run existing format/lint/test/build
checks after implementation changes.

Set numerical memory/latency targets and regression tolerance from baselines
before accepting release defaults. Exact API shape, telemetry fallback values,
backend allocation evidence and platform/capture coverage remain engineering work.
The first synthetic serial/fixed-concurrency measurements are recorded in the
[initial benchmark baseline](benchmarks/2026-09-06-m4-max/README.md). The later
[completion investigation](BenchmarkCompletion.md) adds real captures on three
volumes, extended synthetic scaling, profiles, CLI cancellation and an automated
responsiveness proxy. Native exclusions, broader producer/platform coverage and
actual foreground/pressure testing remain open. Full product
admission and benchmark requirements are recorded in AstroMuninn's monitor and
image-validation designs; general policy is in the workspace
`docs/PerformanceAndResourceDesign.md` when working in the multi-repo checkout.
