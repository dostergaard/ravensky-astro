# Compressed FITS metadata and filename support

Status: implementation complete in the two local bugfix branches; validation
results and release coordination are recorded in EVIDENCE.md and HANDOFF.md.

## Objective

Read canonical metadata from the logical image HDU in ordinary and CFITSIO tiled-image FITS files. Make AstroMuninn CLI and Lite discover `.fits.fz` and `.fit.fz` files while retaining existing extensions and file safety.

## Constraints and non-goals

- Preserve ordinary primary-image behavior, public metadata fields, raw card fidelity, and existing validation semantics.
- Do not change third-party CFITSIO/fitsio code, add a parser/dependency, or make metadata reads decompress full images.
- Keep large N.I.N.A. captures ignored under `tests/data/nina-compressed-fits/`; commit compact generated fixtures/tests only.
- Keep pre-existing uncommitted AstroMuninn changes intact. Do not publish, release, or merge.

## Current state and confirmed defect

All three N.I.N.A. GZIP_2 samples reproduce the empty `dump_fits_metadata` result: 0×0 and only HDU 0 cards. Each has `NAXIS=0` in HDU 0 and a `ZIMAGE=T` binary-table extension with `ZNAXIS1=8288`, `ZNAXIS2=5648`, observational keywords, and `ZBITPIX=16`. `astro-metadata::fits_parser::extract_metadata` selects only the primary HDU. `astro-io::fits::load_fits` also selects only the primary. `astro-io` validation independently scans HDUs and recognizes compressed images. AstroMuninn core checks only the final path extension, so `.fz` paths are omitted by discovery/monitor and rejected by metadata dispatch.

## Proposed design and ownership

1. In `astro-io`, add a small reusable image-HDU selection primitive that walks HDUs in order and identifies a usable logical image by CFITSIO image facts, including tile-compressed tables. Return the selected index and logical geometry/pixel type without reading pixels. Prefer an existing public `fitsio` API; use a narrowly isolated CFITSIO call only if necessary. Define deterministic first-usable-image behavior and report no usable image distinctly. Keep raw physical header access separate.
2. In `astro-metadata`, use the selected image HDU and its logical facts. Parse observational fields from that HDU, with an explicit primary-header fallback only for absent fields if fixtures/tests show it is needed. Selected image values take precedence; never mix physical `NAXIS1/2` from a compressed table into logical dimensions. Preserve raw cards with HDU identities and map compatibility contract, documenting precedence for conflicting cards.
3. In `astromuninn-core`, classify full path suffixes case-insensitively in one place, including `.fits.fz`, `.fit.fz`, and `.fts.fz`; reuse it for discovery, monitor, and metadata dispatch. Retain full source filename when routing. Make preview file type identify FITS rather than FZ.
4. The metadata dump shows cards from every physical HDU and prints the
   selected logical image HDU, dimensions, and pixel type after the report.

The existing validator remains separate because it has bounded resource and integrity contracts. Metadata extraction requires header-only access. Image loading may share selection but remains a separate pixel-heavy operation. No application-specific policy moves into shared crates.

## Affected areas and compatibility

- `ravensky-astro/astro-io/src/fits.rs`, `astro-metadata/src/fits_parser.rs`, focused tests and crate docs/example diagnostics.
- `astromuninn/crates/astromuninn-core/src/organizer.rs`, `workflow.rs`, `monitor/coordinator.rs`, and tests. CLI and Lite consume the same core; review their UI labels/docs.
- Public API additions should be minimal. Existing public functions and `AstroMetadata` fields remain. Changing results for empty-primary files is the intended defect correction; multiple-image precedence must be documented.

## Execution and test-first sequence

1. Add focused failing tests for compact native GZIP_2, primary image, empty primary plus later image, multiple images, absent metadata, and no usable image. Confirm the failures match the observed defect.
2. Implement shared HDU selection and canonical extraction, then image loading if the test confirms it is affected. Verify raw-header and logical-dimension contracts.
3. Add suffix/path tests before changing product classification. Test preview, image-type scan, monitor discovery, and destination filename preservation through shared core.
4. Add optional integration assertions against all ignored real captures, with header-derived values independent of filenames. Test structural/full validation and malformed/truncated input separately.
5. Update docs, run focused tests, then format/check/Clippy/all relevant tests in both repos. Review the final diff and update evidence/handoff.

## Resource model and validation

HDU selection reads header metadata only, with allocations proportional to header cards/HDU count; it must not allocate an 8288×5648 pixel buffer. Existing CFITSIO admission covers handle lifetime. Pixel loading retains its existing full-image allocation and native backend gate. Validator retains its own bounded streaming model and distinction between structural and full integrity checks. Compare metadata latency and peak memory on the three real captures against the current header-only path if implementation introduces additional native calls. No new worker pool or queue is planned.

## Risks and open questions

- `fitsio` 0.21.9 reports the compressed extension as `ImageInfo` with logical
  axes and pixel type. Raw-card calls retain physical `BINTABLE` and `Z*` cards.
- All supplied N.I.N.A. observational fields occur in HDU 1; the compact
  fixture defines primary fallback only for missing keys and selected-image
  precedence when values conflict. A later table/image cannot override
  canonical fields.
- Ensure `.fz` is not treated as external `.gz`; suffix recognition never replaces signature validation.
- AstroMuninn has substantial pre-existing uncommitted work. Restrict edits and stage only task files.
- AstroMuninn uses published `astro-io`/`astro-metadata` 0.6.1 by default.
  Local integration checks patch Cargo to these branch crates temporarily;
  release builds need a coordinated shared-crate publication and dependency
  update, which this task explicitly excludes.
