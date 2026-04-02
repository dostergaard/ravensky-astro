# Canonical Metadata Model Strategy

Status: planned, not yet in execution

This strategy is intentionally written ahead of implementation.

Execution should not begin until an initial representative FITS/XISF sample corpus exists, because the shape of the canonical model should be validated against real file families rather than only against current repo examples.

## 1. Objective

Define a canonical in-memory metadata and image-document model that:

- gives applications one stable format-independent contract
- separates raw format extraction from semantic normalization
- supports future cleanup/canonicalization workflows
- keeps `astro-io`, `astro-metadata`, and downstream consumers like AstroMuninn aligned without breaking current metadata-only behavior

The desired outcome is a staged architecture where FITS and XISF both map into the same canonical document layer, while preserving format-specific raw access and compatibility with existing published APIs during migration.

## 2. Constraints and Non-Goals

Constraints:

- `ravensky-astro` crates are already published, so public API churn should be minimized and staged.
- AstroMuninn currently depends on `astro_metadata::fits_parser` and `astro_metadata::xisf_parser` for metadata extraction.
- Metadata-only consumers must continue to work even when image decoding support is narrower than metadata parsing support.
- The current coarse-grained crate split remains the intended structure:
  - `astro-io` owns raw format access
  - `astro-metadata` owns structured parsing and normalization
  - `astro-metrics` remains separate

Non-goals for the first implementation phase:

- replacing CFITSIO
- broad public API redesign in one step
- writing FITS/XISF round-trip output support immediately
- solving every vendor quirk before the base model exists
- merging all XISF and FITS logic into a new crate hierarchy right away

## 3. Current State

Today the repository is only partly aligned with the intended boundary:

- FITS metadata parsing in `astro-metadata` already leans on `astro-io` for raw FITS header-card access.
- XISF metadata parsing in `astro-metadata` still performs its own XISF header/XML parsing instead of consuming a raw XISF layer from `astro-io`.
- `astro-metadata` publicly depends on FITS-owned raw-header types from `astro-io`, which is an ownership smell.
- `astro-io` XISF image loading has been hardened and is now explicit about its supported decode subset.
- There is not yet a canonical document model that both FITS and XISF map into.
- The current metadata structs are useful but tightly tied to today’s parser organization and may not be the right full raw-to-canonical boundary.

The repo also lacks the broad sample corpus needed to confidently shape the model across:

- amateur capture FITS/XISF
- mission-grade FITS
- richer multi-extension or custom-key FITS
- richer XISF variants

## 4. Proposed Design

Use a three-layer model.

### Raw Format Layer

This layer lives primarily in `astro-io`.

It should expose loss-minimizing format-specific representations such as:

- `fits::RawHeaderRecord`
- `fits::RawHeaderSet`
- `fits::RawImageUnit`
- `xisf::RawProperty`
- `xisf::RawImageBlock`
- `xisf::RawAttachment`
- `xisf::RawDocument`

Properties of this layer:

- preserves order where relevant
- preserves duplicates where relevant
- avoids semantic normalization
- records unsupported or unknown fields without discarding them
- does not apply product-specific fallback policy

### Canonical Document Layer

This layer should likely live in `astro-metadata`, because it is the semantic contract rather than the raw I/O contract.

Suggested top-level types:

- `ImageDocument`
- `ImagePlane`
- `MetadataEnvelope`
- `AcquisitionMetadata`
- `EquipmentMetadata`
- `DetectorMetadata`
- `FilterMetadata`
- `TargetMetadata`
- `SiteMetadata`
- `EnvironmentMetadata`
- `WcsMetadata`
- `CalibrationMetadata`
- `ProcessingMetadata`
- `ProvenanceMetadata`

The canonical layer should distinguish:

- normalized semantic fields
- raw-source evidence or provenance
- derived values
- uncertainty, ambiguity, and conflicts

### Projection / Mapping Layer

This layer maps raw representations into canonical ones.

Examples:

- `FromFitsRaw for ImageDocument`
- `FromXisfRaw for ImageDocument`
- `FitsNormalizationReport`
- `XisfNormalizationReport`

This layer should live in `astro-metadata`, not in `astro-io`, because it is semantic interpretation.

### Why this design fits better

It gives us:

- one stable contract for downstream apps
- clean separation between reading and interpretation
- compatibility with metadata-only consumers
- room for future cleanup/export workflows
- a path to unify XISF/FITS behavior without forcing image-decoding and metadata-parsing support to be identical

It is better than directly “moving everything into one parser” because that would entangle raw format access, semantic rules, and compatibility concerns even more tightly.

## 5. Affected Areas

Primary affected areas:

- `docs/`
- `astro-io/src/fits.rs`
- `astro-io/src/xisf.rs`
- `astro-metadata/src/types.rs`
- `astro-metadata/src/fits_parser.rs`
- `astro-metadata/src/xisf_parser.rs`

Likely future additions:

- new raw-format modules in `astro-io`
- new canonical-model modules in `astro-metadata`
- new normalization-report or mapping modules in `astro-metadata`
- cross-crate regression tests in `tests/`

Downstream compatibility-sensitive areas:

- AstroMuninn metadata ingestion paths
- examples using `astro_metadata::*_parser`
- public docs that currently present the existing metadata structs as the whole contract

## 6. Execution Steps

### Step 0: Build the initial sample corpus

Before implementation begins, gather a representative starter corpus.

Minimum useful coverage:

- amateur FITS from NINA and at least one other capture tool
- amateur XISF from PixInsight and at least one additional XISF-producing workflow if available
- HST FITS
- JWST FITS
- at least one multi-extension or mission-style FITS family
- at least one XISF family that stresses richer metadata than the current sample set

This step is a gate for implementation, not optional polish.

### Step 1: Write the canonical model design note

Turn this strategy into a more concrete type-level design note once the corpus exists.

That note should:

- inventory recurring metadata families across the corpus
- identify fields that are common, optional, conflicting, or format-specific
- define what belongs in canonical metadata versus raw provenance

### Step 2: Define raw-lossless representations

Add internal raw representations in `astro-io` for FITS and XISF.

Goal:

- no normalization
- no metadata policy
- enough fidelity to preserve order, duplicates, source spelling, and raw context where practical

This should begin with headers/properties and image descriptors, not full write support.

### Step 3: Define the canonical document types

Add initial canonical types in `astro-metadata` as internal or narrowly exposed types first.

Start with:

- document identity
- acquisition and detector/equipment metadata
- filter/target/session fields
- WCS
- raw provenance attachment points

Defer broad public stabilization until the mappings have been tested against the corpus.

### Step 4: Build format-to-canonical mapping adapters

Implement adapters from raw FITS and raw XISF structures into the canonical model.

These adapters should:

- preserve provenance
- distinguish absent vs invalid vs conflicting information
- avoid hiding ambiguity

### Step 5: Add normalization reporting

Introduce a report type that records:

- recognized keys/properties
- ignored keys/properties
- rewritten or aliased fields
- conflicts
- unsupported cases

This is important for future cleanup tooling and for trust in the normalization process.

### Step 6: Migrate existing metadata parsers onto the new internals

Reimplement:

- `astro_metadata::fits_parser`
- `astro_metadata::xisf_parser`

on top of the raw + canonical path, while preserving their existing public signatures as long as possible.

This is the key compatibility step for AstroMuninn.

### Step 7: Add cross-format regression tests

Add corpus-backed tests that verify:

- equivalent metadata is normalized consistently from FITS and XISF when appropriate
- metadata-only parsing remains broader than image decoding
- unsupported image payloads do not prevent metadata extraction where metadata is still parseable
- raw provenance is preserved where intended

### Step 8: Reassess public API stabilization

Only after the above is in place should we decide:

- what canonical types become public
- which current public fields should remain
- what can be deprecated or migrated
- whether a broader API cleanup is worth doing while still in `0.x`

## 7. Verification Strategy

Verification should be phased.

Pre-implementation verification:

- corpus inventory document exists
- corpus families are categorized by source and notable traits

Implementation verification:

- focused unit tests for raw FITS/XISF representations
- focused unit tests for canonical type behavior and derived fields
- regression tests for malformed/partial/quirky metadata cases
- corpus-backed tests for HST/JWST/amateur file families
- cross-format equality tests where equivalent files exist

Repository validation commands once implementation begins:

- `cargo fmt --all --check`
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`
- `cargo test --workspace --all-targets`

Additional manual verification:

- verify AstroMuninn can still extract metadata from representative XISF and FITS files
- verify metadata-only consumers are not blocked by unsupported image decode cases

## 8. Risks and Mitigations

Risk: the canonical model is shaped too early from a narrow amateur-only corpus.

Mitigation:

- gate execution on a broader starter corpus
- include at least one mission-grade FITS family before finalizing type boundaries

Risk: raw and canonical concepts get mixed again during implementation.

Mitigation:

- require raw types to remain loss-minimizing and semantically “dumb”
- keep normalization adapters in `astro-metadata`

Risk: metadata-only behavior is accidentally narrowed to match image-decoding support.

Mitigation:

- add explicit tests proving metadata parsing can still succeed on files whose image decode path is unsupported

Risk: public API churn becomes larger than intended.

Mitigation:

- stage new internals behind existing public parser entry points first
- defer public canonical-model stabilization until after corpus-backed validation

Risk: the effort turns into a FITS backend rewrite before the model is stable.

Mitigation:

- treat canonical-model work as independent from CFITSIO replacement
- do not begin backend replacement as part of this phase

## 9. Open Questions

- Should the first public-facing canonical type be additive alongside `AstroMetadata`, or should `AstroMetadata` evolve into the canonical envelope over time?
- How much raw provenance should be embedded directly in canonical types versus attached through sidecar report structures?
- Do we want one generic `HeaderRecord` concept shared across FITS and XISF-derived FITSKeyword material, or separate raw types with a shared higher-level trait?
- Which mission-grade FITS families beyond HST and JWST are worth prioritizing early?
- How far should the first canonical phase go on image-document structure versus metadata-only structure?
- When the corpus arrives, which fields prove common enough to be canonical, and which should remain format- or vendor-specific extensions?
