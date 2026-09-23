# FITS and XISF format-ingestion architecture assessment

- Status: read-only architecture assessment and migration plan
- Assessment date: 2026-09-22
- Production code changed: no
- Intended additions: this report and PlantUML sources only
- PlantUML rendering: unavailable locally; no SVG files generated

## 1. Executive summary

RavenSky should correct its XISF ingestion architecture as part of the first
Revision 1 implementation slice, but it does **not** need a separate flag-day
refactor before correctness work can begin. The first implementation task should
extract the correct 16-byte monolithic envelope, checked XML range, strict UTF-8,
and namespace-aware event handling into `astro-io`, then make the current XISF
reader and validator consume it. The metadata path should migrate through a
small additive raw-inspection boundary rather than build a fourth parser or
consume validation internals.

The recommended ownership is:

- `astro-io` owns FITS and XISF container syntax, safe structural
  interpretation, storage locations, checked byte/sample calculations,
  format-specific decompression and sample decoding, and low-level format
  correctness.
- Format validators consume those primitives and add conformance, integrity,
  configured resource, and diagnostic policy. Validation should not remain the
  sole owner of a more-correct parser dialect.
- `astro-metadata` owns interpretation, precedence, normalization, and derived
  astrophotographic semantics. It should consume raw cards/records/descriptors
  from `astro-io`, not understand XISF prefix or XML layout and not expose
  CFITSIO handles as the preferred path.
- `astro-metrics` should continue to consume resolved `f32` image planes plus
  dimensions. It has no reason to parse FITS/XISF syntax or metadata. A new
  general image framework is not justified now.
- AstroMuninn owns monitoring, retry/admission, organization, path, and
  presentation policy. It should not acquire format syntax.

The immediate architectural problem is XISF-specific. The narrow loader,
validator, and metadata extractor independently interpret the container. The
validator is currently the most complete implementation, but its full `Node`
tree, `Context`, budgets, and validation diagnostics are not a suitable shared
object model. Extract the syntax and safety pieces; retain validation policy.

FITS does not require immediate architectural work. Its native CFITSIO reader
and pure-Rust validator duplicate some concepts, but that is mostly justified
specialization: one renders images through a mature native library while the
other validates hostile bytes under RavenSky-controlled resource bounds. The
selective later cleanup is to let FITS metadata semantics consume cards without
opening or inspecting CFITSIO handles directly. Replacing CFITSIO, forcing FITS
through the XISF design, or building a generic FITS/XISF AST has no present
justification.

There are zero standalone migration stages required before the first XISF
correctness fix can land. Stage 1 is both the architectural extraction and the
16-byte-prefix/strict-header fix. General public parser APIs, a public generic
image type, typed XISF astrometry, and broad FITS API changes should be deferred.

## 2. Current dependency and ownership map

### 2.1 Crate dependencies

| Owner | Current dependencies relevant to ingestion | Observed role |
| --- | --- | --- |
| `astro-io` | `fitsio`, `quick-xml`, codecs/digests | FITS/XISF loading, raw FITS cards, validation, native backend gate |
| `astro-metadata` | `astro-io`, `fitsio` | Semantic metadata; FITS uses `astro-io` cards but also opens/inspects CFITSIO; XISF parses independently |
| `astro-metrics` | declares `astro-io` and `astro-metadata`, but source has no references to either | SEP-facing analysis over `&[f32]`, width, and height |
| `ravensky-astro` | all three crates | Thin re-export facade |
| `astromuninn-core` | published `astro-io` and `astro-metadata` 0.6.1 | Monitoring validation plus metadata-driven planning/organization |

The direction `astro-metadata -> astro-io` is suitable for a raw-to-semantic
pipeline. `astro-io` must not depend on `astro-metadata`; doing so would create a
cycle and put normalized semantics below format access. The declared format
dependencies in `astro-metrics` are not used by its source and should not shape
the target design.

### 2.2 Current public surfaces

- `astro_io::fits::load_fits` returns `(Vec<f32>, usize, usize)` from the primary
  HDU through CFITSIO.
- `astro_io::fits::{FitsHeaderCard, read_header_cards,
  read_all_header_cards, header_cards_to_map}` expose raw FITS cards and a
  compatibility map. `astro-metadata` re-exports `FitsHeaderCard`.
- `astro_io::xisf::load_xisf` returns the same tuple but supports only an
  uncompressed, one-channel, attachment-backed `UInt16` subset.
- `astro_io::validation::{validate_file, validate_file_with_budget, ...}`
  exposes typed format-independent validation policy and reports. Its FITS and
  XISF parsers remain private.
- `astro_metadata::{AstroMetadata, fits_parser, xisf_parser}` exposes normalized
  semantic metadata and raw FITS-style cards.
- `astro_metrics::sep_detect` accepts only resolved pixel slices and dimensions.
- The facade simply re-exports the three crates.

### 2.3 Downstream flow

AstroMuninn uses `astro_metadata::fits_parser::extract_metadata_from_path` or
`xisf_parser::extract_metadata_from_path` based on the filename extension. The
result drives destination planning, object/frame/filter/equipment display, and
raw-card presentation. Monitor validation separately calls
`validate_file_with_budget`, maps its non-exhaustive typed errors to product
policy, and retains the returned file stamp for a pre-operation recheck.

No AstroMuninn production source calls `load_fits`, `load_xisf`, or
`astro-metrics`. Pixel-loader changes therefore affect library consumers and
examples first; metadata and validator behavior affect AstroMuninn directly.

### 2.4 Relationship to existing architecture plans

`docs/FormatArchitecturePlan.md` proposed new per-format crates, a small shared
format-core crate, a typed multi-image buffer, and eventual native FITS replacement.
That plan predates the completed managed validator and the concrete XISF
Revision 1 divergence. The current evidence does not justify paying its package,
release, feature, and public-contract costs before fixing XISF. This assessment
therefore narrows the immediate direction to per-format modules inside
`astro-io`; it does not claim that a future format crate can never be useful.

`docs/CanonicalMetadataModelStrategy.md` already establishes the important
raw-versus-semantic split and gates a broad canonical document model on a real
FITS/XISF corpus. The recommendation here is compatible with that split. The
minimal XISF raw handoff needed to remove current parser duplication is not the
same as stabilizing the proposed cross-format canonical document model, which
remains deferred behind corpus evidence.

The older `docs/AstroMetadataPlan.md` is historical implementation context. Its
idea of coupling metadata directly into loader return values is not adopted;
independent metadata-only operation is an established current requirement.

See [current ingestion paths](diagrams/01-current-format-ingestion.puml) and
[current responsibilities](diagrams/02-current-responsibilities.puml).

## 3. FITS architecture findings

### 3.1 Normal image loading and native boundary

`astro-io/src/fits.rs:39-60` opens the file, selects the primary HDU, obtains
dimensions from `HduInfo::ImageInfo`, and asks CFITSIO/fitsio to read all pixels
as `f32`. `astro-io/src/fits/backend.rs:1-151` places open/operations/drop under
a reentrancy-aware gate. Independent handles may run concurrently when the
linked CFITSIO is reentrant; otherwise all participating native calls serialize.

CFITSIO should remain behind `astro-io` for image and card access. It already
owns mature image conversion and compressed-FITS rendering. Moving those native
details into metadata or metrics would invert the intended dependency direction.

The loader is intentionally smaller than the validator contract. It reads only
the primary image and has no RavenSky memory budget or cooperative cancellation.
It also indexes the first two shape entries directly, so its public 2-D contract
should eventually reject non-2-D shapes explicitly rather than depend on native
shape assumptions. That is a local reader-hardening item, not a reason to
redesign FITS ingestion now.

### 3.2 FITS metadata extraction

`astro-metadata/src/fits_parser.rs:22-72` opens a `FitsFile`, enters the
`astro-io` CFITSIO gate, reads the primary HDU, and calls the shared
`astro_io::fits::read_header_cards`. Semantic parsing then consumes the
last-value-wins compatibility map while retaining ordered duplicate cards.

Answer: `astro-metadata` does **not** independently parse raw FITS header blocks
or HDU extents. It reasonably reuses `astro-io`'s `FitsHeaderCard` extraction.
It does, however, directly depend on `fitsio`, accept `&mut FitsFile` publicly,
and inspect `HduInfo` for detector information. That is native-library leakage
across the preferred metadata boundary. It is architectural divergence, but it
is not evidence of a current correctness defect.

### 3.3 Pure-Rust validation and compressed FITS

`astro-io/src/validation/fits.rs:38-254` independently walks every HDU from
bytes. It enforces 80-byte ASCII cards, mandatory-card order, unique structural
keywords, supported `BITPIX`, checked dimensions, 2880-byte header/data padding,
HDU extents, table geometry, optional checksums, and full-file traversal.

Binary tables and compressed images are validation-specific managed paths:

- `binary_table` checks `TFORM`, row widths, P/Q descriptors, heap offsets, and
  declared heap bounds.
- `tile_layout` checks `ZNAXIS*`, `ZTILE*`, decoded size, and row count.
- `validation/fits/tiles.rs` resolves named compressed/fallback/mask columns and
  dispatches GZIP, Rice, PLIO, HCOMPRESS, or uncompressed tiles.
- GZIP, Rice, and PLIO stream bounded data; HCOMPRESS admits a bounded complete
  tile working set.
- Full validation checks stored checksums before payload decode and does not use
  a CFITSIO fallback.

This path validates storage correctness; it does not render dequantized pixels.
CFITSIO remains the normal image decoder. Keeping these paths separate is
justified while they share small arithmetic concepts where useful.

### 3.4 FITS definition/interpretation inventory

| Concept | Current implementations | Classification | Consequence |
| --- | --- | --- | --- |
| FITS cards | CFITSIO-backed `FitsHeaderCard`; private validator structural `BTreeMap` | Justified specialization | Metadata preserves full cards; validator keeps only safety-relevant keys |
| HDUs | CFITSIO handle traversal; pure byte-offset validator loop | Justified specialization | Native reading and hostile-input validation have different goals |
| Image geometry / `BITPIX` | CFITSIO `HduInfo`; validator header arithmetic; metadata compatibility map | Harmless duplication with a local reader risk | Values can be interpreted on different paths; `load_fits` should explicitly guard 2-D shape |
| Padding/extents | CFITSIO internal behavior; validator `padded` and `Context::extent` | Justified specialization | Pure arithmetic is valuable for tests/Kani but need not replace CFITSIO |
| Binary-table descriptors | Validator `binary_table`; CFITSIO internals for native access | Justified specialization | Managed validator checks exact heap bounds without rendering |
| Compressed-tile locations | Validator `tiles::payload`; CFITSIO native decoder | Justified specialization | Different resource and output contracts |
| Metadata semantics | `astro-metadata` over cards and `HduInfo` | Architectural divergence | Direct native type/API coupling can be removed later |
| Resource enforcement | Validator only | Architectural gap, not immediate FITS blocker | Normal CFITSIO loads are unbudgeted and not cooperatively cancellable |

The normal reader and validator do not share card parsing, extent, or sample-size
logic today. This is acceptable at the native/pure-validation boundary, provided
malformed-input behavior is not falsely promised to be identical. Small pure
arithmetic may be shared internally for proofability; a shared FITS AST is not
needed.

### 3.5 Malformed-input behavior

Validation distinguishes incomplete, invalid, unsupported, integrity, resource,
changed-source, cancellation, and I/O outcomes. CFITSIO-backed loading and
metadata expose `anyhow`/native failures and may accept formats outside the
managed validator's supported set. This is materially different by contract,
but not automatically a defect. A consumer that requires readiness must call
validation; successfully opening with CFITSIO is not the same guarantee.

## 4. XISF architecture findings

### 4.1 Current paths

There are three structural interpretations:

1. `astro-io/src/xisf.rs` manually scans strings for the first `Image`,
   geometry, sample format, compression, and attachment location. It reads XML
   after 12 bytes and always decodes `UInt16` as little-endian.
2. `astro-io/src/validation/xisf.rs` reads the correct 16-byte prefix, builds a
   namespace-aware private event/tree representation, checks block descriptors,
   ranges, compression, byte-order spelling, checksums, and resource limits,
   and can decode zlib/LZ4/Zstandard for validation.
3. `astro-metadata/src/xisf_parser.rs` reads XML after 12 bytes, uses lossy UTF-8,
   swallows header-extraction failure, and scans strings for `FITSKeyword`,
   `Property`, image, color, and attachment metadata.

The Revision 1 assessment remains the factual baseline: the two 12-byte paths
are wrong, their fixtures encode the same error, big-endian pixel data is
silently misdecoded, and the validator's extension/Zstandard frame policies
need targeted corrections.

### 4.2 Duplicated responsibilities and best existing basis

| Responsibility | Duplicated in | Most complete current basis | Shared-primitive suitability |
| --- | --- | --- | --- |
| 16-byte monolithic prefix/reserved bytes | loader, validator, metadata | Validator | Yes: envelope/range primitive |
| XML byte range and truncation | all three | Validator | Yes: checked borrowed/owned header range |
| Strict UTF-8 | loader is strict after wrong range; metadata is lossy; validator decodes events | Validator event path plus explicit whole-header UTF-8 check | Yes |
| Namespace and root handling | validator only structurally; other paths use text | Validator | Yes as event handling; extension policy stays separate |
| Image descriptors | loader, validator, metadata | Validator for geometry/sample/location/compression; loader for returned pixels | Yes as format-specific descriptor, not generic AST |
| Byte order | validator validates spelling; loader ignores | Validator descriptor | Yes; sample decoder consumes it |
| Sample format / geometry | all three | Validator | Yes: checked typed calculation |
| Block locations / ranges | loader and validator; metadata records attachment text | Validator | Yes |
| Compression/subblocks | loader rejects; validator parses/decodes; metadata reports strings | Validator | Yes, split descriptor parsing from policy/decode |
| Checksums | validator | Validator | Keep validator/integrity policy; share stored-range access |
| `FITSKeyword` | metadata string scanner; validator event tree retains attributes | Neither is complete alone | Build from shared events; preserve name/value/comment |
| `Property` | validator knows typed storage size; metadata exact-string scans selected values | Validator structure plus metadata semantics | Raw records may cross boundary; typing/normalization stays metadata |
| Extension handling | validator | Validator, after root-only policy correction | Event parser shares namespace/depth; acceptance is validation policy |
| Resource limits/cancellation | validator only | Validator | Share bounded I/O/reservation primitives where decoding needs them |

The validator must not simply become a public document model. Its `Node` owns
all XML strings, parent/child indices, validation-only text, and a `Context`
that combines file state, diagnostics, budgets, checksum counters, and policy.
That shape is too heavy for metadata-only reads and would couple ordinary reads
to validation policy.

### 4.3 Recommended extraction seam

Extract three XISF-specific layers inside `astro-io`:

1. A monolithic envelope reader: signature, 16-byte prefix, reserved bytes,
   checked XML range, strict UTF-8, and source extent.
2. A namespace-aware event/descriptor layer: root scope, `Image`,
   `FITSKeyword`, `Property`, storage location, sample, byte order, geometry,
   compression, and raw attributes/text required by consumers.
3. Format-specific block/sample decoding: exact stored/decoded lengths,
   subblocks, codecs, unshuffle, byte order, and normalized output.

The validator consumes the same descriptors and adds required-element rules,
uniqueness/placement, checksum policy, malformed-versus-unsupported categories,
configured limits, and full traversal. Metadata consumes only raw records and
descriptors needed for semantics. The narrow loader consumes image/block/sample
descriptors and the decoder.

## 5. Parsing-versus-validation analysis

Checks necessary to avoid overflow, out-of-bounds reads, ambiguous byte ranges,
or unsafe allocation are part of parsing even when they reject input.

| Format | Safe structural interpretation | Validation policy layered above it |
| --- | --- | --- |
| FITS | checked 80-byte reads, numeric decoding, `BITPIX` width, checked axis products, 2880-byte extent construction, table/heap descriptor bounds | mandatory-card ordering, allowed HDU kinds, uniqueness, checksum requirements, codec/profile conformance, configured structure/decoded limits |
| XISF | 16-byte envelope, strict UTF-8, XML well-formedness, safe namespace/depth events, checked attachment/inline ranges, geometry/sample byte counts, parsed compression/subblock sizes, bounded block access | mandatory/unique elements/properties, legal child placement, root-only extension acceptance, ID uniqueness, exact codec-frame rules, checksum enforcement, unsupported-object classification, configured aggregate limits |

Some rules straddle the boundary. An unknown sample format can be structurally
represented without decoding, but a pixel loader must report it unsupported.
An attachment outside the observed file is unsafe/incomplete for every
consumer. A checksum descriptor can be parsed structurally while verification
remains full-validation policy. Exact Zstandard framing is XISF storage
conformance and must be applied by both full validation and any pixel decoder
claiming to decode that block.

Ordinary readers must never repeat unchecked offset arithmetic merely to avoid
the validator. Metadata-only extraction need not decode payloads, hash blocks,
or enforce every schema invariant once its header range and event stream are
trustworthy.

## 6. Metadata boundary analysis

### 6.1 What `astro-metadata` should receive

For FITS, the existing ordered `FitsHeaderCard` representation is adequate.
Metadata semantics should consume cards and optional format-neutral geometry
facts, not a `FitsFile` or `HduInfo`. The path-based public API can remain while
its implementation delegates native access to `astro-io`.

For XISF, `astro-metadata` needs a lightweight raw inspection result containing
only facts already required by current behavior:

- ordered `FITSKeyword` name/value/comment records;
- raw `Property` identifier/type/value or storage reference records;
- image identifier and the raw geometry/sample/byte-order/color facts used by
  existing metadata and attachment reporting;
- selected document metadata such as version and creator records.

This should be an owned or borrow-scoped record view with private fields and
accessors or a bounded iterator/visitor. It must not expose `quick-xml` event
types, the validator `Node`, validation `Context`, CFITSIO handles, or codec
state. A large public XISF AST is not justified.

Because `astro-metadata` is a separate crate, complete consolidation cannot be
achieved with `pub(crate)` items alone. One small additive `astro-io` boundary is
unavoidable unless duplicate XISF parsing is retained. Exact naming should be
chosen during implementation, but the contract should be non-exhaustive or
accessor-based and limited to raw records/descriptors. Existing public metadata
extraction signatures need not change.

### 6.2 Public versus internal

| Concept | Initial visibility |
| --- | --- |
| Checked range/extent and byte-count helpers | private / `pub(crate)` in `astro-io` |
| XISF envelope and XML event machinery | private / `pub(crate)` |
| XISF image/block/compression descriptors | private unless a field is required by the raw inspection boundary |
| Sample format and byte-order enums | private initially |
| Codec, checksum, reservation, validation context | private |
| Existing `FitsHeaderCard` | remain public |
| Minimal raw XISF inspection result | small additive public cross-crate contract during migration |
| General XISF Property public model | defer until raw-preservation consumers are defined |
| Typed `AstrometricSolution` | defer |

### 6.3 Raw preservation versus semantics

Raw records and normalized semantics must remain distinct. Padding, comments,
duplicate/order information, producer-specific spellings, and unknown XISF
properties are evidence. `AstroMetadata` is the semantic view with precedence,
numeric/date parsing, coordinate-source distinctions, and derived session date.
Normalization must not overwrite the preserved raw value.

## 7. Pixel and metrics boundary analysis

`astro-metrics` currently consumes `&[f32]`, width, and height. It contains no
FITS/XISF parsing and AstroMuninn does not use it. That boundary is directionally
correct.

Before metrics receives pixels, the format layer should have resolved:

- attachment/tile ranges and exact decoded length;
- compression and shuffle;
- byte order and sample type;
- channel/plane selection and layout;
- any documented bounds/offset normalization policy; and
- `width * height == sample_count` for the supplied plane.

The existing tuple is a format-neutral representation by convention, but does
not encode these invariants. A small validated plane view could become useful if
two or more real consumers need it. Introducing one now would widen public APIs
without solving the immediate XISF parser divergence. Keep metrics unchanged,
add format-layer invariant tests, and revisit a type only with concrete consumer
pressure.

The declared `astro-metrics -> astro-io/astro-metadata` manifest edges are not
used by source. Removing them is useful later cleanup; it is not a prerequisite
for Revision 1.

## 8. Resource and cancellation analysis

### 8.1 Current state

The validator has the strongest resource contract:

- checked `u64` add/multiply and file extents;
- maximum header, working, structure, and decoded-byte limits;
- caller-shared nonblocking `MemoryBudget` reservations;
- 64 KiB file/stream chunks;
- cooperative cancellation checkpoints;
- bounded zlib/Zstandard history/output and exact decoded size;
- admitted whole blocks only where the codec requires them; and
- source-stamp consistency checks.

The ordinary XISF loader allocates header and payload vectors from declared
sizes without a configurable budget or cancellation, and it reads the complete
payload before decoding. The XISF metadata reader similarly allocates the full
header. FITS loading/metadata rely on CFITSIO allocations and blocking native
calls; the gate controls reentrancy, not memory.

### 8.2 Target split

- Share checked sizes, bounded input, exact-length decoding, Zstandard window
  preflight, and cancellation checkpoints where readers and validators perform
  the same low-level work.
- Keep aggregate conformance limits and diagnostic counters in validation.
- Allow reader-specific limits to be supplied through a small reader option or
  bounded entry point when compressed pixel loading is added. Do not pass the
  entire validation context into ordinary readers.
- Applications coordinate aggregate admission, worker count, storage pressure,
  and retry/fairness. Libraries expose costs and controllable primitives.
- Treat native codec/CFITSIO memory as separate from managed Rust buffers and
  state what is bounded versus estimated.

For XISF Revision 1, compressed pixel decoding must have an explicit decoded
size and native-window ceiling before Zstandard support is called complete.
For FITS, adding a new bounded loader API can wait until a production pixel
consumer requires it; changing mature CFITSIO behavior for symmetry would add
risk without current benefit.

## 9. Architecture options considered

### Option A — per-format structural kernels in `astro-io` (recommended)

- **Ownership:** private FITS and XISF kernels; common checked arithmetic/bounded
  input only where semantics truly match.
- **Dependencies:** `astro-metadata -> astro-io`; metrics remains format-neutral;
  facade/applications depend downward.
- **APIs:** existing loaders/metadata/validation stay; add only a minimal raw
  XISF inspection contract needed across the crate boundary.
- **FITS:** preserve CFITSIO reader and pure validator specialization; cards are
  the metadata handoff.
- **XISF:** one envelope/event/descriptor implementation feeds loader,
  validator, and metadata.
- **Resources:** low-level decoding uses explicit bounds/reservations;
  validation keeps aggregate policy.
- **Cost/risk:** moderate extraction and cross-crate contract design; lowest
  long-term divergence and no generic AST.

### Option B — share arithmetic/envelope helpers only

- **Ownership:** loader, validator, and metadata keep separate element parsers;
  only prefix/range/size functions are shared.
- **Dependencies/APIs:** few public additions; current semantic code changes
  less.
- **Benefit:** fastest correction of the 12/16-byte and overflow defects.
- **Risk:** `Image`, `FITSKeyword`, `Property`, namespace, extension, and future
  Revision 1 behavior can still diverge. Metadata would continue to understand
  XISF container syntax. This does not adequately address the forcing problem.

### Option C — new per-format/shared format crates

- **Ownership:** as proposed by the existing `FormatArchitecturePlan`, separate
  FITS/XISF crates (optionally plus a shared core) would serve `astro-io` and
  `astro-metadata`.
- **Benefit:** cross-crate internals could be isolated from the `astro-io`
  public module surface and could later support multiple facades.
- **Risk:** new package/version/release complexity, backend/feature design,
  another public dependency layer, premature typed image contracts, and
  uncertain semver ownership. The current repository has two formats and a
  correct coarse crate split; no evidence requires package extraction to fix
  the present divergence. Defer rather than reject permanently.

Option A best fits the existing dependency direction and the repository's early
but published API state. It shares actual syntax, not a generic parser framework.

## 10. Recommended architecture

The target is shown in
[03-recommended-format-architecture.puml](diagrams/03-recommended-format-architecture.puml).

1. **Who owns FITS syntax?** `astro-io`. CFITSIO owns native decoding behind
   its adapter; the pure validator owns RavenSky's hostile-byte interpretation.
2. **Who owns XISF syntax?** `astro-io`, through one monolithic envelope and
   namespace-aware event/descriptor implementation.
3. **Who owns safe structural interpretation?** `astro-io`, with per-format
   kernels plus small shared checked range/size/bounded-input primitives.
4. **Who owns conformance validation?** `astro_io::validation`, layered over
   descriptors and retaining typed error/resource policy.
5. **Who owns metadata semantics?** `astro-metadata`.
6. **Who owns decoded sample/image semantics?** `astro-io` resolves format
   storage, sample type, byte order, layout, and normalization contract.
7. **What should metrics receive?** A validated `f32` plane/slice and dimensions;
   no format syntax.
8. **What low-level primitives should be shared?** Checked add/multiply/ranges,
   FITS padding where used, XISF envelope/ranges, image byte count, compression
   subblock accounting, bounded payload input, byte unshuffle, and Zstandard
   frame-window checks.
9. **What remains specialized?** FITS versus XISF structure, CFITSIO rendering,
   FITS tile codecs, XISF XML/event rules, validation policy, metadata semantics,
   and application scheduling.
10. **What remains private initially?** Parser events/trees, descriptors beyond
    the minimal metadata handoff, sample/byte-order enums, codec state,
    reservations, validation context, native handles, and proof helpers.
11. **What should not be refactored?** Do not replace CFITSIO, unify FITS/XISF
    ASTs, redesign metrics, move workflow policy into libraries, or add typed
    astrometry/color/distributed XISF features without a consumer.

## 11. PlantUML diagram index

| Diagram | Purpose | Render status |
| --- | --- | --- |
| [01-current-format-ingestion.puml](diagrams/01-current-format-ingestion.puml) | Current FITS/XISF call paths and duplication | Source only |
| [02-current-responsibilities.puml](diagrams/02-current-responsibilities.puml) | Responsibility-stage mapping | Source only |
| [03-recommended-format-architecture.puml](diagrams/03-recommended-format-architecture.puml) | Recommended ownership and visibility | Source only |
| [04-incremental-migration.puml](diagrams/04-incremental-migration.puml) | Stages, dependencies, removal points, tests/Kani | Source only |

`command -v plantuml` returned no executable, and no repository PlantUML
renderer/configuration was found. Per task constraints, no software was
installed and no alternate diagram language or generated SVG was substituted.

## 12. Kani implications

The architecture improves proofability by separating pure arithmetic and cursor
logic from XML, filesystem, native codecs, and validation orchestration. Proposed
bounds are proof domains, not runtime limits.

| Production function/property | Symbolic domain | Essential assumptions | Expected postcondition |
| --- | --- | --- | --- |
| Shared checked range/extent | arbitrary `u64` offset, length, file length | none | success implies no overflow and `offset + length <= file_len`; otherwise typed error |
| FITS `padded` | arbitrary `u64` byte count | none | success is the minimal multiple of 2880 not below input; overflow rejects |
| Image byte-count calculation | dimensions/channels/sample width across full integer range | accepted sample width; separately bound allocations | success equals a wider mathematical product; zero/overflow rejects |
| FITS `type_size` and heap span | arbitrary type code/count/offset/heap size | descriptor bytes are available | accepted span matches `u128` oracle and lies in heap |
| XISF compression subblocks | bounded list of stored/decoded part pairs | list length is the recorded proof bound | sums do not overflow and exactly match descriptors on success |
| Byte unshuffle | bounded byte slice, item width/count, incomplete tail | output allocation matches input length | no out-of-bounds access; complete groups permute exactly once; tail unchanged; round trip identity |
| Zstandard `frame_window` / exact-frame cursor | 0–18 arbitrary header bytes plus bounded remainder cursor | do not assume valid magic/flags | no panic/OOB; valid header yields correct bounded window; truncated/reserved/multiple/skippable XISF cases reject as specified |
| Bounded payload `Input` | bounded inline bytes and operation sequence | `BufRead::consume` contract or explicit clamping | logical consumption never exceeds declared length or leaks next frame |

Keep harnesses beside private production functions under `#[cfg(kani)]`; do not
make helpers public solely for proof access. Follow the existing adoption plan:
pilot `frame_window`, then FITS padding/extent arithmetic. When XISF extraction
creates shared range/subblock/unshuffle helpers, they become strong later
candidates. Normal tests/fixtures remain authoritative for XML event structure,
CFITSIO, native decompression, filesystems, concurrency, and floating-point
semantics.

## 13. Impact on the XISF Revision 1 implementation plan

The prior phase ordering should be adjusted so behavior is implemented once:

- **Proceed immediately:** corrected 16-byte envelope, reserved bytes, strict
  UTF-8, checked XML range, and actionable errors. This is the structural seam.
- **Same migration:** byte-order-aware exact pixel decoding and event-based
  `FITSKeyword` extraction. Both consume shared descriptors/events.
- **Wait for shared structural extraction:** Zstandard pixel loading,
  `zstd+sh`, root extension handling, and expanded schema rules. Implementing
  these first in the narrow loader or metadata scanner would duplicate logic.
- **Later Revision 1 work:** exact one-frame-per-subblock enforcement and bounded
  codecs belong in the shared XISF storage/decoder layer plus validation policy.
- **After the primary Revision 1 compatibility slice:** public raw `Property`
  preservation, detached signature structure, broader schema invariants, and
  storage regression corpus.
- **Defer:** typed astrometry, color transforms, distributed units/block indexes,
  external locations, generic image framework, and broad public parser API.

The original Phase 1 remains correct in substance, but “shared” now means a
format-owned structural path in `astro-io`, not a helper copied between the
loader and metadata crate.

### 13.1 Authoritative migration classification

Every proposed action in this report maps to exactly one ID/category here.

| ID | Classification | Affected area | Reason / dependencies | Test strategy | Public API impact | Kani suitability |
| --- | --- | --- | --- | --- | --- | --- |
| M1 | **Required before XISF Revision 1 implementation** | `astro-io` XISF envelope/event internals and fixtures | Establish one 16-byte, strict, checked structural source before adding features | corrected positive/negative prefix, truncation, UTF-8, root tests | none initially | high: ranges/byte counts |
| M2 | **Do during XISF Revision 1 implementation** | XISF loader, validator, metadata handoff | Migrate all consumers to M1; cross-crate metadata needs minimal raw view | parity tests for descriptors/records and old-path removal | small additive `astro-io` raw-inspection boundary; existing APIs unchanged | medium: descriptor sizes |
| M3 | **Do during XISF Revision 1 implementation** | XISF sample decoder and `FITSKeyword` mapping | Fix big endian, exact length, escaped/padded/comment records on M1/M2 | LE/BE golden pixels; truncated/suffix; keyword/event and AstroMuninn path-plan goldens | no breaking change | high: byte counts/unshuffle |
| M4 | **Do during XISF Revision 1 implementation** | XISF storage decoder and validator policy | Exact Zstandard frames, bounded zstd/zstd+sh, root extensions; depends on M1/M2 | codec/subblock/window/budget/cancel and extension-placement fixtures | bounded reader option may be additive; avoid changing current signature | high: frame cursor/subblock sums |
| M5 | **Do during XISF Revision 1 implementation** | `astro-io` proof modules | Add only focused Kani pilots alongside extracted production functions | named harnesses plus normal regression tests | none | purpose of item |
| M6 | **Do during XISF Revision 1 implementation** | release plus AstroMuninn dependency update | Product consumes published 0.6.1, so shared-crate fixes require coordinated release | shared full checks; monitor error mapping; metadata/path-plan goldens | version update only | no: integration behavior |
| M7 | **Do after Revision 1** | XISF Property/signature/schema/storage corpus | Preserve information and harden conformance after core compatibility | raw-property, mandatory metadata, unique ID, signed-envelope, multi-storage fixtures | additive raw-property access requires API review | selective size/range helpers |
| M8 | **Do after Revision 1** | FITS metadata boundary | Move preferred semantics input to cards/geometry facts, reduce direct `FitsFile`/`HduInfo` coupling | card-to-semantic parity and duplicate/order tests | keep path API; later deprecation only after evidence | low/medium |
| M9 | **Do after Revision 1** | FITS private validator helpers | Extract only proofable arithmetic reused by active code | unchanged validator fixtures plus Kani padding/extent/type-size | none | high |
| M10 | **Do after Revision 1** | `astro-metrics/Cargo.toml` | Remove unused format dependencies after confirming package/docs behavior | cargo metadata/tree, check, tests, docs/package | dependency-only; no API | none |
| M11 | **Deferred / no present justification** | all crates | Generic FITS/XISF AST/parser framework | N/A | would create broad unstable API | poor |
| M12 | **Deferred / no present justification** | `astro-metrics` and loaders | New generic decoded-image framework | revisit with multiple concrete consumers | broad public API | only small invariants would suit |
| M13 | **Deferred / no present justification** | FITS loader/backend | Replace CFITSIO or force it through managed validator decoding | requires separate compatibility/performance program | likely breaking | cannot prove native replacement equivalence broadly |
| M14 | **Deferred / no present justification** | XISF semantics | Typed astrometry/color/distributed/external objects | needs authoritative fixtures and consumer requirements | new public domain models | broad semantics unsuitable |
| M15 | **Do after Revision 1** | architecture docs / README links | Reconcile the older format-crate/native-FITS plan with the evidence-backed direction once implementation settles | link/structure review and factual audit | none | none |

## 14. FITS refactoring recommendations

| Recommendation | FITS priority | Migration ID | Rationale |
| --- | --- | --- | --- |
| Change FITS before the first XISF slice | **No present justification** | M13 | FITS is not causing the XISF prefix/event divergence |
| Keep CFITSIO image decode behind `astro-io` | **Required now for architectural consistency/correctness** | Existing boundary; no new change | Do not leak native decoding upward while XISF boundaries are corrected |
| Make `load_fits` reject unsupported dimensionality explicitly | **Do after XISF Revision 1** | M8 follow-up | Small local safety hardening; no current AstroMuninn pixel consumer |
| Prefer card/geometry input for FITS metadata semantics and eventually reduce public `FitsFile` coupling | **Do after XISF Revision 1** | M8 | Clarifies boundary without replacing CFITSIO |
| Prove private FITS padding/extent/type-size helpers | **Useful extraction while nearby code is being changed** | M9 | High assurance, no need to unify parsers |
| Share a complete FITS parser/AST with the normal reader | **No present justification** | M11 | Native and validator paths have intentionally different outputs/policies |
| Replace managed compressed-FITS validation with CFITSIO | **No present justification** | M13 | Would lose current resource/exactness contract |

No FITS code must precede or accompany the first XISF implementation task.

## 15. Incremental migration plan

The dependency sequence is shown in
[04-incremental-migration.puml](diagrams/04-incremental-migration.puml).

### Stage 1 — shared XISF structural foundation and first fix

- **Affected modules:** `astro-io/src/xisf.rs`,
  `astro-io/src/validation/xisf.rs`, new private XISF submodules, XISF fixtures.
- **Objective:** one 16-byte envelope, checked ranges, strict UTF-8, event stream,
  and descriptor calculation.
- **Unchanged behavior:** public loader tuple; validation entry points/error
  categories; intentionally unsupported pixel formats remain unsupported.
- **Tests before:** capture current validator-positive cases and explicitly mark
  12-byte fixtures invalid.
- **Tests after:** signature/reserved/range/truncation/UTF-8/root/namespace cases;
  loader and validator agree on header facts.
- **Fixtures:** rewrite synthetic positive files with four reserved bytes.
- **Public API:** none required for the initial internal extraction.
- **Kani:** range/extent and image-byte-count candidates.
- **Rollback concern:** keep changes cohesive; reverting restores old defects, so
  do not retain a compatibility switch for 12-byte fixtures.

### Stage 2 — migrate pixel and metadata consumers

- **Affected modules:** `astro-io/src/xisf.rs`,
  `astro-metadata/src/xisf_parser.rs`, metadata types/tests, cross-crate tests.
- **Objective:** loader consumes typed descriptors; metadata consumes raw event
  records through the minimal additive boundary.
- **Unchanged behavior:** existing public extraction/load signatures and current
  normalized metadata precedence except corrected false-success/malformed cases.
- **Tests before:** semantic snapshots for current valid FITSKeyword metadata and
  AstroMuninn routing fields.
- **Tests after:** LE/BE equality, exact payload length, escaped/padded keyword
  values/comments, strict error propagation, raw/normalized separation.
- **Fixtures:** paired byte-order files and realistic metadata records.
- **Public API:** small accessor-based raw XISF inspection result in `astro-io`;
  no XML/native types.
- **Kani:** pixel byte count and future unshuffle.
- **Rollback concern:** metadata now returns errors previously swallowed; release
  notes and downstream error goldens must make that correction explicit.

After Stage 2 parity passes, remove the old 12-byte readers, string attribute
scanner, and string `FITSKeyword`/`Property` traversal. This is the first safe
duplicate-removal point.

### Stage 3 — Revision 1 codec and extension compatibility

- **Affected modules:** XISF descriptor/decoder internals,
  `validation/xisf/streaming.rs`, validation policy, loader tests.
- **Objective:** exact one-frame-per-subblock Zstandard behavior, bounded
  zstd/zstd+sh pixel decoding, correct shuffle, and root-only extension handling.
- **Unchanged behavior:** zlib/LZ4/checksum support; existing valid 1.00 files;
  structural versus full-validation contract.
- **Tests before:** current codec/checksum/budget/cancellation baseline.
- **Tests after:** valid one-frame subblocks, invalid concatenated/skippable
  frames, multi-subblock shuffle/tail, root versus nested extensions, budget and
  cancellation release.
- **Fixtures:** synthetic edges plus writer-produced Revision 1 corpus.
- **Public API:** optional bounded reader configuration only if needed to expose
  safe compressed loading; preserve current wrapper.
- **Kani:** frame-window/cursor and subblock accounting.
- **Rollback concern:** do not weaken generic Zstandard utilities if exact-frame
  policy is XISF-specific.

### Stage 4 — release and consumer migration

- **Affected modules:** crate versions/release artifacts in a future authorized
  task; AstroMuninn dependencies, lockfile, monitor and plan tests.
- **Objective:** make corrected shared crates reach the product.
- **Unchanged behavior:** product workflow policy, destination semantics,
  validation retry/admission policy, raw FITS display.
- **Tests before/after:** shared crate full checks; AstroMuninn monitor outcome,
  metadata extraction, raw inspector, destination/path-plan goldens.
- **Fixtures:** same corrected XISF files across repositories or immutable hashes.
- **Public API:** version update only for application code.
- **Kani:** none; integration evidence is required.
- **Rollback concern:** local RavenSky changes do not affect AstroMuninn until a
  released/path-patched candidate is explicitly selected.

### Stage 5 — post-Revision-1 hardening and selective FITS cleanup

- **Affected modules:** XISF raw properties/schema/signature; later FITS metadata
  boundary/private helpers; metrics manifest only if confirmed unused; settled
  architecture documentation and README links.
- **Objective:** preserve new information, harden conformance, and clean proven
  boundary debt without reopening the core decoder.
- **Unchanged behavior:** normalized metadata precedence and CFITSIO rendering.
- **Tests:** mandatory/unique metadata, property lexical forms, detached
  signatures, multi-image/storage corpus, FITS card semantic parity, package
  dependency checks.
- **Fixtures:** authoritative PixInsight files with provenance and checksums.
- **Public API:** separately reviewed additive raw properties; any deprecation
  requires a migration window.
- **Kani:** FITS padding/extent/type size and any new pure property-size helpers.
- **Rollback concern:** keep FITS and public API cleanup in separate cohesive
  changes so Revision 1 compatibility is independently releasable.

## 16. Acceptance and verification strategy

### Deterministic checks

For implementation stages, run the narrow suites first and then:

```text
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-targets
cargo test -p astro-io --test validation
cargo test -p astro-metadata
```

Add cross-crate contract tests proving that the same XISF header facts drive
loader, validator, and metadata. Every corrected malformed case should assert a
stable category or actionable context, not merely “some error.” Preserve valid
XISF 1.00 coverage.

### Fixtures

- Correct all synthetic XISF prefixes to 16 bytes.
- Retain minimal positive/negative fixtures for every range, namespace, byte
  order, compression, checksum, and metadata edge.
- Acquire writer-produced PixInsight 1.9.5+ files for zstd/zstd+sh, properties,
  extensions, signatures, and multi-image behavior before claiming Revision 1
  interoperability.
- Record producer version/settings, checksum, expected samples/dimensions, and
  redistribution provenance.

### Resource/performance evidence

Deterministically test reservation release, decoded/window limits,
cancellation, and bounded chunking. Separately benchmark serial/fixed/automatic
application workloads for throughput, peak RSS, native overhead, storage
contention, cancellation latency, and foreground responsiveness. A passing
resource-limit test is not a peak-memory measurement.

### Diagrams/documentation

When PlantUML is available, render all four `.puml` sources to adjacent SVGs and
fail on syntax errors. Keep source authoritative. Review diagrams and report
together whenever boundaries or migration ordering change.

## 17. Risks and tradeoffs

- A minimal public raw XISF handoff is still public surface. Use accessors and
  non-exhaustive design; do not export the internal parser tree.
- Tightening malformed-input handling may expose permissively accepted files.
  Confirm that rejections are invalid under XISF 1.00/1.01 and distinguish
  malformed from unsupported.
- Metadata error propagation changes observable behavior from false success to
  failure. AstroMuninn must retain useful diagnostics and deterministic plan
  outcomes.
- Sharing too little leaves parser dialects; sharing the whole validator forces
  expensive validation and unstable policy into readers. The recommended seam
  deliberately shares syntax/safety only.
- Native memory remains partly outside Rust reservations. Do not claim a hard
  process ceiling for CFITSIO or native codecs.
- Public normalized metadata structs are already broad. Adding raw properties
  directly as fields can be semver-sensitive for struct literals; prefer a
  separately reviewed accessor/container.
- The README currently points to an older format-crate/native-FITS architecture
  plan. Until M15 reconciles the records, this assessment is the specific design
  authority for the XISF Revision 1 migration; the older plan remains useful
  historical/long-range context.
- Removing unused metrics dependencies can affect docs.rs feature-selection
  plumbing even if source is unchanged; package/documentation checks are needed.
- Current AstroMuninn worktrees contain unrelated in-progress changes. Future
  consumer migration must preserve them and use a fresh coordinated task.

## 18. Open questions

1. Should the minimal XISF raw handoff be an owned inspection result or a
   borrow-scoped iterator/visitor? Current consumers favor owned records, but
   peak header size and API ergonomics should be measured.
2. What exact integer `bounds`/`offset` normalization contract should the public
   XISF `Vec<f32>` loader use?
3. Should compressed loading gain an additive options/budget entry point in the
   Revision 1 slice, or can a conservative documented default safely support
   all intended direct consumers?
4. Does PixInsight 1.9.5 emit legacy FITS WCS cards alongside standard
   `AstrometricSolution` properties?
5. Is raw XISF Property presentation required by AstroMuninn immediately after
   compatibility, or can it remain a post-Revision-1 API decision?
6. Does any downstream consumer rely on `astro_metadata::fits_parser::extract_metadata`
   accepting a caller-owned `FitsFile`, and what deprecation window would a
   cards-first replacement require?
7. Are `astro-metrics`' declared format dependencies retained solely for
   historical/docs.rs reasons? Confirm packaging before removal.
8. After Revision 1 settles, should the older `FormatArchitecturePlan` be marked
   superseded, or revised into a long-range option document?

Questions 1–3 affect Stage 2/3 API shape but do not justify another parser.
Questions 4–7 do not block Stage 1.

## 19. Commands and evidence inspected

### Required policy and prior work

- Workspace and repository `AGENTS.md` files for `ravensky-astro` and
  AstroMuninn.
- `docs/work/xisf-1.0-revision-1/ASSESSMENT.md` as factual baseline.
- Workspace `docs/kani-adoption-plan.md` and
  `docs/PerformanceAndResourceDesign.md`.
- `docs/work/file-validation-0.6.0/PLAN.md` and crate READMEs.
- `docs/FormatArchitecturePlan.md`, `docs/CanonicalMetadataModelStrategy.md`,
  `docs/AstroMetadataPlan.md`, `docs/CfitsioControlImplementation.md`, and
  `docs/file-validation-implementation.md`.
- The `impl` skill for the planning structure; the `verify` skill is used for
  final artifact checks.

### Graft navigation

```text
graft map
graft ask ... --source
graft skeleton <focused file>
graft callers load_fits --depth 2
graft callers read_header_cards --depth 2
graft callers validate_file_with_budget --depth 2
graft callers load_xisf --depth 2
graft callers extract_metadata_from_path --depth 2
graft callers detect_stars_with_sep_background --depth 2
graft callers extract_metadata --depth 2
graft callers build_raw_metadata_lines --depth 2
```

Queries covered FITS/XISF loaders, validators, metadata extraction, codecs,
budgets, public re-exports, native boundaries, metrics, and AstroMuninn callers.

### Focused source and dependency evidence

- `astro-io/src/fits.rs`, `fits/backend.rs`.
- `astro-io/src/xisf.rs`.
- `astro-io/src/validation/{mod,input,resources,fits,xisf}.rs` and focused FITS
  tile/codec plus XISF streaming modules.
- `astro-metadata/src/{fits_parser,xisf_parser,types,lib}.rs`.
- `astro-metrics/src/{lib,sep_detect,types}.rs`.
- facade `src/lib.rs` and all relevant Cargo manifests.
- AstroMuninn organizer, workflow/raw inspector, monitor validator/executor,
  configuration/dependency references, and manifests.

Read-only commands included `cat`, `sed -n`, `nl -ba`, `rg`, `rg --files`,
`command -v plantuml`, `git status --short`, and Graft commands. No production
source, tests, manifests, public APIs, versions, or pre-existing documentation
were modified. No implementation or Kani harness was started.
