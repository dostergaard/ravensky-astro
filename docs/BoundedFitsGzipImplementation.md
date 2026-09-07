# Bounded FITS GZIP validation

Status: implemented and locally verified after `348aceb`. Native allocation
controls for remaining layouts are not complete.

## Objective and decision

Remove CFITSIO from full validation of the common integer GZIP tile layout, so
decoded tile size and cache behavior cannot cause native allocations on that path.
Reuse flate2's existing Rust backend and shared reservations. This is the first
bounded compressed-FITS increment, not a claim that all CFITSIO allocations are
controlled. Do not enable the remaining native paths in shared-budget mode.

## Constraints and current state

CFITSIO concurrency is coordinated, but its GZIP inflation can realloc beyond a
declared tile size and its tile cache can retain several decoded tiles. Forking the
whole native backend would require allocation-failure and codec audits; a helper
process alone is not a portable memory ceiling. Direct streamed validation avoids
those allocations for a precisely defined subset while preserving concurrency.

Initial coverage: integer `ZBITPIX` 8/16/32/64, `GZIP_1`/`GZIP_2`, one
`COMPRESSED_DATA` column using a P/Q byte descriptor, all positive tiles, arbitrary
supported dimensions and short edge tiles. Additional columns, floating-point
quantization and other codecs keep the existing standalone native path and remain
unsupported through shared admission. No silent fallback after a managed decode
error. Image loading and metadata extraction retain their existing backend.

## Design and resource model

- Generalize the existing bounded XISF subblock reader into an internal input
  module reusable by FITS. Keep physical read accounting, cancellation and logical
  consumption separate from read-ahead.
- Stream a single GZIP member per tile into discarded 64 KiB output chunks.
  Require Deflate StreamEnd, exact output size, CRC32/ISIZE and exact descriptor
  consumption. GZIP_2 unshuffling is a reversible byte permutation and need not
  materialize numeric pixels for container validation.
  Additional GZIP members return `Unsupported`; other trailing data fails integrity.
- Reserve 1 MiB inflater allowance, 256 KiB optional-header allowance and at most
  64 KiB each for input/output. Limit each GZIP header to 64 KiB before flate2 can
  accumulate unbounded filename/comment data. Managed buffers allocate fallibly;
  flate2 bookkeeping remains infallible within the audited allowance.
- Compute each tile's expected byte count using checked arithmetic and the
  FITS axis ordering, including partial edges. Validate heap descriptors against
  the already-checked table extent. No decoded tile or cache is retained.
- Keep one caller-owned worker per validation, no internal threads or waiting
  queues. Shared reservations bound admitted work, not total process RSS or OS
  allocation success. Check cancellation between output chunks and input reads.

## Affected areas and sequence

1. Add failing shared-budget integer GZIP tests and oversized-output regressions.
2. Extract the bounded input reader, implement GZIP framing/streaming and tile
   layout dispatch, retaining the native gate for remaining layouts.
3. Verify native-generated GZIP_1/GZIP_2 files, P/Q offsets, short edge tiles,
   truncated/corrupt/trailing streams, limits and reservation cleanup.
4. Update support documentation and run workspace checks. Keep native resource
   controls and expanded compressed-FITS benchmark coverage visible as follow-ups.

## Verification and risks

Tests establish exactness, cancellation and admission invariants; they do not
establish RSS. Use a decoded tile larger than a 2 MiB shared allowance to prove
the algorithm does not reserve a decoded tile, and concurrent fixtures to verify
aggregate release. Run fmt, Clippy, tests/doctests, docs and release build.
Measure compressed-FITS throughput/RSS with representative native-produced fixtures
before choosing release defaults; the previous XISF benchmark is not such evidence.
Real captures, Windows/Linux and foreground contention remain release checks.

Reference: FITS tile compression convention 2.3, sections 2–3 and GZIP description:
https://fits.gsfc.nasa.gov/registry/tilecompression.html

## Remaining work

This subset does not resolve RICE, PLIO, HCOMPRESS, floating-point quantization or
fallback-column decoding. Their bounded implementations or enforceable native
allocation controls still precede full shared compressed-FITS coverage and monitor
rollout. No new user-facing configuration or dependency is required here.

## Local verification

The initial 8 MiB tile / 2 MiB shared-budget test failed at the old native gate,
then passed through the managed path. A checksum-order regression also failed
before the fix and now confirms that stored FITS checksums precede decode;
decoded-size preflight still precedes checksum I/O.

Coverage includes native-generated GZIP_1/GZIP_2, integer widths 8/16/32/64,
P/Q descriptors, nonzero heap gaps, partial edge tiles in three dimensions,
optional GZIP metadata, truncated/non-final streams, corrupt CRC/ISIZE, trailing
data, oversized output/header rejection, shared contention/cleanup, four concurrent
calls, buffered-input cancellation and explicit exclusion of other layouts.

Passed: 110 workspace all-target tests, four doctests, fmt, Clippy with warnings
denied, documentation with warnings denied and release build. One existing SEP
test remains ignored. No native allocation ceiling or new RSS/throughput result
is claimed by these tests. The earlier XISF measurements describe their recorded
revision. A subsequent [compressed-FITS baseline](benchmarks/2026-09-06-m4-max-fits-gzip/README.md)
now records synthetic GZIP_1/GZIP_2 row/image-tile RSS and worker scaling; it does
not compare against the historical native decoder. Real-capture measurements
remain outstanding.
AstroMuninn changes only update its design documents; runtime integration is pending.
