# Changelog

All notable changes to RavenSky Astro will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.6.0] - 2026-09-09

### Added
- Managed compressed-FITS validation for Rice, PLIO, HCOMPRESS, floating-point,
  quantized, fallback-column and null-mask layouts. Streaming or admitted tile
  buffers replace the validator's estimated CFITSIO path; both entry points now
  share the same bounded behavior. Includes native-oracle and malformed-input tests.
- Standalone benchmark guide covering setup, runnable CLI and external-project
  Rust examples, real captures, report interpretation and investigation suites.
- Extended benchmark investigation: worker-order/tile/size matrices, sampled
  profiles, bounded contention and cancellation checks, plus read-only real-capture
  measurements with preserved source hashes and explicit rejected-sample records.
  Recorded local SSD/HDD/external-SSD evidence and supplied format regression fixtures.
- Compressed FITS benchmark recipes for GZIP_1/GZIP_2 with configurable tile rows,
  bounded generation, quota checks, native pixel-parity tests and a repeatable
  isolated measurement matrix. Original fixture versions and bytes are preserved.
- Bounded full validation of integer GZIP_1/GZIP_2 FITS tiles in a single
  COMPRESSED_DATA P/Q byte column, available with shared memory admission.
  Stream decoded bytes with CRC/size/end checks and a 64 KiB optional-header cap.
- Shared CFITSIO admission across FITS loaders and metadata helpers,
  with runtime reentrancy detection and public wrappers for direct
  native callers. Reentrant builds retain parallelism; non-reentrant builds
  serialize participating callers. Managed validation does not use this gate.
- Caller-owned `MemoryBudget` and nonblocking `validate_file_with_budget`, with
  automatic reservation release, distinct `ResourceBusy` outcomes and peak
  reservation telemetry.
- Optional `astro-bench` library/CLI with bounded deterministic FITS/XISF fixtures,
  read/structural/full workloads, fixed concurrency, cancellation, isolated repeated
  samples, peak-memory/CPU telemetry and versioned JSON reports. Application
  calibration and adaptive scheduling remain separate follow-up work.
- Read-only `astro_io::validation` API for structural and full FITS/XISF checks,
  with typed failures, configurable resource limits, cooperative cancellation,
  source observations and checksum/decode coverage reports.
- Complete supported HDU/block traversal, FITS heap and tiled-image validation,
  and XISF local blocks/references, zlib/LZ4/LZ4HC/Zstandard decompression and
  SHA-1/SHA-2/SHA-3 verification. Existing image-loader contracts are unchanged.
- Generated regression fixtures for truncation, corruption, supported variants,
  limits, source mutation and cancellation. Documented support boundaries and
  remaining real-capture/platform rollout checks in the `astro-io` README.

### Changed
- Verify FITS stored-data/HDU checksums before compressed payload decoding,
  preserving declared-size preflight before checksum I/O. Reuse one bounded input
  reader across FITS GZIP and XISF streamed codecs.
- Stream XISF zlib/Zstandard input and discard decoded chunks, preserving checksum,
  frame, trailing-data and exact-output checks. Reserve parser/inline/LZ4/FITS
  working data before allocation. Truncated zlib trailers now fail explicitly.

## [0.5.0] - 2026-08-25

### Added
- `Exposure::header_coordinates`, `HeaderCoordinatePairs`, and `CoordinatePair`
  preserve `RA`/`DEC` and `OBJCTRA`/`OBJCTDEC` as independently sourced
  coordinate pairs for FITS and XISF metadata extraction
- `sensor_temperature_qa` example, which recursively evaluates FITS/XISF sensor
  temperature deviations, previews candidates, confirms moves, preserves relative
  paths, and safely handles cross-volume moves
- Regression coverage for FITS/XISF coordinate-source preservation, sexagesimal
  coordinate handling, coordinate bounds, sensor-temperature selection, and safe
  cross-volume example moves

### Changed
- `Exposure::ra` and `Exposure::dec` are now documented as legacy convenience
  fields. They continue to prefer `RA`/`DEC` and fall back to `OBJCTRA`/`OBJCTDEC`;
  new consumers should use `header_coordinates` when source identity matters
- Coordinate projection is shared by the FITS and XISF parsers, preventing format
  drift while intentionally avoiding target, pointing, or image-center inference
- Hardened `astro-io` XISF loading to return explicit errors for malformed or unsupported files instead of falling back to hardcoded offsets or placeholder pixel data
- Removed direct stdout output from the `astro-io` XISF loader and limited diagnostics to library-appropriate logging

### Fixed
- FITS numeric `RA` values expressed in degrees are no longer incorrectly
  converted from hours a second time
- FITS `OBJCTRA` and `OBJCTDEC` now support sexagesimal fallback parsing, and
  invalid right-ascension or declination ranges are rejected with warnings

### Documentation
- Documented source-preserving coordinate handling and staged migration guidance
  in the canonical metadata-model strategy and `astro-metadata` README
- Added the external FITS standard PDF to `.gitignore` because it is maintained
  outside this repository

## [0.4.0] - 2026-03-23

### Added
- FITS header-card extraction APIs in `astro-io` so callers can inspect ordered raw cards by HDU
- `raw_header_cards` support in `astro-metadata` for both FITS and XISF metadata extraction
- Expanded metadata dump and stats examples for inspecting FITS/XISF raw header content

### Changed
- `AstroMetadata` now exposes both `raw_header_cards` and `raw_headers` as part of its public metadata model
- Workspace crate version references across docs now point to `0.4.0`

## [0.3.0] - 2026-03-05

### Changed
- Breaking: root library import name changed from `astro_core` to `ravensky_astro`
- Documentation and examples now use `ravensky-astro` / `ravensky_astro` naming consistently
- Updated crate version references across workspace docs to `0.3.0`

## [0.2.0] - 2025-05-31

### Added
- Enhanced quality metrics with improved scoring algorithms
- Kron radius and AUTO flux calculations using SEP functions
- Logarithmic SNR scoring for better perceptual representation
- FWHM consistency score to detect uneven focus
- Elongation metric for more intuitive star shape assessment
- Comprehensive unit tests for all crates
- Documentation for quality metrics

### Changed
- Improved background scoring to combine uniformity with noise level
- Refactored metrics to use only data available from SEP
- Updated API to be more consistent and intuitive
- Fixed deprecated method calls in chrono library usage

### Fixed
- Type conversion issues in SEP function calls
- Removed unused imports and variables
- Fixed floating point precision issues in tests
