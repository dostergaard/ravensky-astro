# Changelog

All notable changes to the Astro Core project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

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
