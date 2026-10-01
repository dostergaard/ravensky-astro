# Format Architecture Plan

## Context

RavenSky currently uses `fitsio`/CFITSIO for FITS access in two places:

1. `astro-metadata` FITS metadata extraction.
2. `astro-io` FITS image loading.

Windows testing showed a repeatable FITS-open boundary at full path length `260` (`259` succeeds, `260` fails) in the current stack.

At the same time, XISF handling is already native Rust and structurally separate from FITS behavior.

The crate naming alignment completed in version 0.3.0. The published meta crate,
its documentation, and its Rust imports now use the RavenSky identity. Historical
rename details remain in the changelog.

This document proposes a format-centric architecture so path handling fixes and future format support are implemented once, then reused across tools.

---

## Goals

1. Keep current top-level crate APIs stable (`astro-io`, `astro-metadata`).
2. Isolate format-specific behavior and dependencies per format.
3. Solve Windows FITS path behavior once at backend level.
4. Enable incremental addition of new formats without touching all crates.
5. Avoid a big-bang rewrite.
6. Align internal and external naming on `ravensky-astro` to reduce contributor friction and ambiguity.

---

## Naming Alignment (complete)

1. `ravensky-astro` is the canonical meta-crate name in package metadata and
   documentation; Rust consumers import it as `ravensky_astro`.
2. Documentation, examples, and AstroMuninn references use canonical naming.
3. The 0.3.0 breaking import rename was made directly because the library had
   zero known usage; no temporary compatibility alias was needed.

---

## Proposed Crate Model

### New format crates

1. `astro-format-fits`
2. `astro-format-xisf`
3. Optional later: `astro-format-<newformat>` (TIFF, JPEG XL, etc.)

### Existing facade crates remain

1. `astro-io` remains the primary image I/O facade.
2. `astro-metadata` remains the primary metadata facade.
3. `ravensky-astro` remains the canonical meta crate and re-export surface.

### Optional shared contract crate (recommended)

1. `astro-format-core` for shared decoding traits and error contracts.
2. Keep this crate small and dependency-light.

---

## Interface Strategy

Define internal contracts first, then route existing facade APIs to them.

FITS and XISF are container formats that may store multiple images with different numeric types.
The core interfaces should model this explicitly instead of assuming one `f32` image.

Suggested internal operations:

1. `sniff_format(path, bytes) -> Option<ImageFormat>`
2. `list_images(path) -> Result<Vec<ImageRef>>`
3. `describe_image(path, image_ref) -> Result<ImageDescriptor>`
4. `read_headers(path, scope) -> Result<RawHeaderMap>`
5. `read_image(path, image_ref, read_options) -> Result<ImageBuffer>`
6. `read_image_f32(path, image_ref, read_options) -> Result<(Vec<f32>, ImageDescriptor)>` (compat/convenience conversion layer)

Where:

1. `ImageRef` identifies one image inside a container (FITS HDU index/name, XISF image id/index).
2. `ImageDescriptor` includes dimensions, channel/plane count, sample type, bit depth, and relevant scale metadata.
3. `HeaderScope` differentiates container/global headers vs image-local headers.
4. `ImageBuffer` is a typed enum (for example: `U8`, `U16`, `U32`, `I16`, `I32`, `F32`, `F64`) with shape metadata.

`astro-metadata` continues mapping raw headers to `AstroMetadata` types (with explicit image selection support).
`astro-io` continues exposing convenient image-loading APIs, but routes through the typed backend operations.

No immediate public API break is required.

---

## FITS Backend Strategy

Use one crate (`astro-format-fits`) with backend abstraction:

1. `native` backend (new Rust FITS parser/reader).
2. `cfitsio` backend (existing `fitsio` behavior, optional during transition).

### Why this approach

1. Enables phased migration and parity testing.
2. Preserves a fallback while native implementation matures.
3. Lets callers stay stable while backend evolves.

---

## XISF Strategy

Move existing XISF logic into `astro-format-xisf` early:

1. Low risk (already native Rust).
2. Establishes architecture pattern.
3. Makes future format additions consistent.

`astro-io` and `astro-metadata` then call XISF format crate APIs instead of internal module code directly.

---

## Migration Phases

## Phase 0 - Baseline and Contracts

1. Define shared internal interfaces and error taxonomy.
2. Add integration tests for current behavior (golden files).
3. Preserve current public APIs.

## Phase 0.5 - Naming Alignment (complete)

1. Completed a repository-wide audit of code, documentation, examples, CI, and
   release scripts.
2. Switched documentation and examples to canonical package and import names.
3. Added CI checks that prevent retired naming from reappearing outside
   historical release notes.
4. Skipped temporary compatibility because there were zero known consumers at
   the time of the breaking 0.3.0 release.

## Phase 1 - Extract XISF

1. Create `astro-format-xisf`.
2. Move parser and image decode logic from current crates.
3. Wire `astro-io`/`astro-metadata` facades to new crate.
4. Verify no API changes for downstream users.

## Phase 2 - FITS crate skeleton

1. Create `astro-format-fits` with backend trait.
2. Implement initial `cfitsio` backend adapter to current behavior.
3. Route `astro-io` and `astro-metadata` FITS entry points through this crate.

## Phase 3 - Native FITS implementation

1. Implement native header reader first (metadata-critical path).
2. Then implement image data decode path.
3. Add parity tests against current backend outputs.

## Phase 4 - Backend default switch

1. Enable native backend by default.
2. Keep `cfitsio` backend as optional fallback feature for one or more releases.
3. Document backend feature flags and support matrix.

## Phase 5 - Cleanup

1. Remove deprecated internal adapters.
2. Keep compatibility shims only where needed.
3. Reassess whether `cfitsio` fallback remains necessary.

---

## Compatibility Plan

1. Keep `astro-io::fits::load_fits` signature unchanged.
2. Keep `astro_metadata::fits_parser::extract_metadata_from_path` signature unchanged.
3. Define and document default image-selection behavior for compatibility (for example, primary/first image unless explicitly selected).
4. Add new explicit APIs for selecting an image/HDU without removing existing convenience APIs.
5. Route implementation under the hood to format crates.
6. Document backend selection (feature flags) without forcing downstream rewrites.
7. Keep naming migration history in release notes; do not retain temporary
   compatibility shims after the migration has closed.

---

## Test and Acceptance Criteria

## Functional parity

1. Header extraction parity on representative FITS corpus.
2. Image dimension and pixel decode parity.
3. Multi-image/container parity:
   FITS primary + extensions, XISF with multiple `<Image>` entries.
4. Numeric-type parity:
   integer and floating-point sample types used in real capture/processing pipelines.
5. Error parity for malformed files where practical.

## Windows path behavior

1. Use the path probe harness as regression test input.
2. Native FITS path handling must no longer fail at full path `260` in the same environment.
3. Validate both file-heavy and dir-heavy path growth modes.

## Performance

1. Benchmark metadata extraction throughput.
2. Benchmark image decode throughput and memory behavior.
3. Ensure no major regressions compared to current release baseline.

---

## Risks and Mitigations

1. FITS standard edge cases:
   Mitigation: incremental scope and golden corpus tests.
2. Backend divergence during transition:
   Mitigation: parity tests and explicit backend toggles.
3. Accidental API drift in facade crates:
   Mitigation: compile-time compatibility tests and semver discipline.
4. Implicit assumptions about single-image files:
   Mitigation: require explicit image-selection model in internal APIs and test both default and explicit selection paths.
5. Lossy type conversion to `f32` in analysis pipelines:
   Mitigation: keep typed `ImageBuffer` path as primary backend contract and treat `f32` conversion as an opt-in convenience.

---

## Recommended Next Step

Start with Phase 0 and Phase 1 immediately:

1. Define interfaces.
2. Extract XISF to `astro-format-xisf`.
3. Wire facades without API break.

This establishes the architecture quickly and reduces risk before FITS native work begins.
