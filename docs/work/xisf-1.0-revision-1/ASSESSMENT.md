# XISF 1.0 Revision 1 impact assessment

- Status: read-only specification assessment; no implementation is included
- Assessment date: 2026-09-22
- Primary scope: `astro-io`, `astro-metadata`, and the `ravensky-astro` facade
- Downstream scope: AstroMuninn XISF metadata extraction, validation, and organization

## Executive summary

XISF 1.0 Revision 1 is deliberately backward compatible, but it raises the
minimum expectations for a conforming decoder. The most material additions for
RavenSky are mandatory Zstandard support, explicit support for both byte orders
and SHA-1/SHA-256/SHA-512, root-level extension handling, XML-signature-aware
document handling, tighter metadata/schema invariants, and the standard
`AstrometricSolution` property namespace.

The repository already has strong support in the full validator for Zstandard,
all mandatory checksum algorithms, XISF's 16-byte monolithic-file prefix,
bounded decompression, and both normal and shuffled compression identifiers.
It does not need a wholesale XISF rewrite.

There is, however, one P0 correctness defect independent of Revision 1's
publication: the public `astro_io::xisf::load_xisf` pixel loader always decodes
`UInt16` samples as little-endian. A valid big-endian image can therefore be
accepted and silently return incorrect pixels. That loader and the
`astro-metadata` XISF reader also begin XML at byte 12 instead of byte 16; their
synthetic unit tests reproduce the same invalid prefix and hide the defect.
These are current nonconformances, not merely missing support for future files.

The assessed implementation backlog is:

| Priority | Count | Meaning |
|---|---:|---|
| P0 | 1 | Silent data corruption/correctness risk |
| P1 | 5 | Required compatibility or significant downstream reliability |
| P2 | 5 | Conformance hardening and preservation of new information |
| P3 | 3 | Optional/deferred breadth with no present workflow dependency |

The first implementation slice should establish one shared, tested monolithic
header/XML reader that starts XML at byte 16, enforces strict UTF-8, and returns
errors instead of silently discarding malformed headers. The pixel loader can
then honor `byteOrder` on top of that foundation. This sequencing removes the
invalid fixture convention before adding Revision 1 features.

## Scope, constraints, and non-goals

This assessment:

- compares the September 2026 Revision 1 announcement and XISF document
  version 1.01 against the final 2017 document version 1.00 source;
- maps normative additions and corrections to current RavenSky code and tests;
- traces relevant behavior into AstroMuninn;
- identifies implementation, regression-test, fixture, and bounded-verification
  work; and
- defines a prioritized implementation plan with acceptance criteria.

It intentionally does not modify Rust code, tests, manifests, public APIs, or
existing documentation. It does not claim full schema conformance merely from
the passing test suite, and it does not propose implementing features for which
RavenSky has no current reader or writer surface unless they are needed to
preserve interoperability.

## Authoritative sources and version comparison

Primary official sources:

- [Revision 1 announcement](https://pixinsight.net/dev/index.php?articles/xisf-1-0-specification-revision-1.22/),
  published 2026-09-18. It announces document version 1.01, calls out the new
  standard astrometric-solution representation, and states the compatibility
  and updated decoder requirements.
- [Current XISF 1.0 specification](https://pixinsight.com/doc/docs/XISF-1.0-spec/XISF-1.0-spec.html),
  document version 1.01, September 2026.
- [Official XISF specification repository](https://gitlab.com/pixinsight/XISF-specification),
  used to compare the side-by-side `pidoc/XISF-1.0.0` and
  `pidoc/XISF-1.0.1` sources and the final 2017 history.

Comparison baseline:

| Item | Previous | Revision 1 |
|---|---|---|
| Document version | 1.00, 2017-04-17 | 1.01, September 2026 |
| Official source baseline | final 2017 source, commit lineage ending at `04cd885` / merge `8817dd3` | revision source at `f72b856` / `9dbdd65` when inspected |
| Compatibility policy | Original XISF 1.0 | All previously valid units remain valid; encoders targeting old decoders should prefer zlib or LZ4 over new Zstandard identifiers |

Revision 1 does not define a new file-format major version. A reader can add
the new behavior without breaking valid version 1.00 inputs. Tightening checks
should reject only inputs that were already invalid, though RavenSky's current
synthetic fixtures will need correction because some encode a 12-byte rather
than 16-byte monolithic prefix.

## Current architecture and downstream flow

| Surface | Current role | Relevant evidence |
|---|---|---|
| `astro-io/src/xisf.rs` | Narrow public pixel loader for an uncompressed, one-channel, attachment-backed `UInt16` image | Module contract at lines 1-30; parsing at 39-250 |
| `astro-io/src/validation/xisf.rs` | Structural/full validation, block locations, checksums, decompression, geometry limits | Parse/validate at 26-607 |
| `astro-io/src/validation/streaming.rs` | Bounded frame inspection and streaming decompression | Zstandard frame handling at 53-160 |
| `astro-metadata/src/xisf_parser.rs` | XISF header and FITS-keyword metadata extraction | Header extraction at 19-113; keyword parsing at 117-221 |
| `ravensky-astro/src/lib.rs` | Public umbrella re-export | Re-exports at 42-44 |
| `astromuninn/src-tauri/src/organizer.rs` | Uses `astro_metadata::xisf_parser::extract_metadata_from_path` | XISF branch at 233-292 |
| `astromuninn/src-tauri/src/workflow.rs` | Builds organization plans/destinations from extracted metadata and shows raw keyword cards | Planning at 837 onward; raw inspection at 1141-1205 |
| `astromuninn/src-tauri/src/monitor/validator.rs` | Maps `validate_file_with_budget` results to monitoring outcomes | Validation path at 143-199 |

AstroMuninn currently consumes published `astro-io` and `astro-metadata`
versions rather than local path dependencies. Delivery therefore requires a
crate release/version update and then an AstroMuninn dependency update; changing
the local shared crates alone will not change the product binary.

The public pixel loader is not on AstroMuninn's current production path.
Consequently, its endian defect is severe for direct library consumers but does
not presently corrupt AstroMuninn organization output. AstroMuninn is directly
exposed to the metadata prefix/error-swallowing issues and to validator rejection
of valid extension elements.

## Revision 1 delta inventory

### Normative additions

| Revision 1 change | Specification area | RavenSky status | Required response |
|---|---|---|---|
| Standard `AstrometricSolution` property namespace | §11.5.3.7; Annex A supplies projection background | Unknown properties are not semantically represented; metadata extraction is string-pattern based | Preserve without failure first; typed interpretation is P3 unless a workflow requirement emerges |
| `zstd` and `zstd+sh` compression identifiers | §§10.6.9-10; baseline decoder in §7.2 | Full validator supports both; narrow pixel loader rejects every compression method | Add loader support if it continues to represent a decoder; validate exact frame semantics |
| Corrected and normative color-transform definitions | §8.5.4.2 and Annex B | No color-transform implementation | No regression; defer typed implementation |
| RGB working-space luminance coefficients are derived and serialized values must agree | §§8.5.4.1, 11.8 | No RGB working-space interpretation/validation | P3 semantic validation if exposed later |
| XML Schema definition | §9.5 | Parser has selected structural checks, not schema-equivalent validation | Implement high-value invariants directly; do not add a runtime schema dependency without evidence |
| Foreign-namespace extension elements may be root children and must be ignored if unknown; unknown children inside core elements remain invalid | §§7, 9.5 | Namespace parser rejects every foreign namespace anywhere | P1: accept/skip only valid root extension subtrees and retain core-child rejection |
| Block-index identifier generation recommendation (`xoshiro256**`) | §9.4 | No writer/block-index implementation | N/A for current production code |
| Boolean lexical output is `true`/`false`; decoders also accept `1`/`0` | §8.3.4 | No general scalar-property decoder | Cover when scalar parsing is consolidated |
| Empty vectors and matrices have inline, zero-length representations | §§8.4.4.5-6, 11.1.8-9 | No general vector/matrix property decoder | Preserve/accept in future property parser; add focused cases |
| Standard metadata properties: `XISF:BlockAlignmentSize`, `XISF:ChecksumAlgorithms`, `XISF:MaxInlineBlockSize`, `XISF:OutputHints` | §11.4.2 | `XisfMetadata::block_alignment` exists, but parser looks for a nonstandard `blockAlignment` attribute and misses the property | P2: parse/preserve correctly; unknown standard properties must not fail |
| `Image/@id` is unique when present | §11.5.2 | Validator counts images but does not enforce unique image IDs | Add structural validation and duplicate-ID regression |
| Root-child whitespace is insignificant; signed material must not be reformatted | §9.5 | Parser ignores relevant outer whitespace and does not rewrite files | Existing behavior is adequate; document signature boundary behavior |
| Additional nonfinite float spellings: `nan`, `-nan`, `inf`, `-inf` | §8.3.3 | No general float-property decoder | Add lexical coverage with future scalar parser |
| Baseline decoder conformance is explicit: both byte orders, all standard codecs, SHA-1/SHA-256/SHA-512, normal/planar organization, minimum sample/color support, and graceful unsupported-object handling | §7 | Full validator covers checksums/codecs and reports typed unsupported features; pixel loader is deliberately narrow and silently mishandles big-endian samples | Clarify API conformance claims; repair silent error and required compatibility paths |

### Informative additions and reorganizations

| Change | Code impact |
|---|---|
| Expanded introduction, design goals, scope, overview, definitions, and references | Documentation context only |
| Annex A projection descriptions | Useful only if typed astrometric-solution interpretation is implemented |
| Expanded XML-signature explanation | Exposes a parser compatibility issue because a detached signature follows the root element |
| Reorganized examples and explanatory material | No direct implementation change; examples inform fixture design |

### Corrections and clarifications

| Correction | Current impact |
|---|---|
| Scalar table fixes (`Int128` limits and epsilon labeling) | No `Int128` implementation; editorial today |
| Decimal zero regular expression and float lexical grouping | Relevant to any future schema-equivalent scalar parser |
| String values exclude surrogate code points and are always UTF-8 | Metadata reader currently uses lossy UTF-8 and can silently alter invalid input; P1 prerequisite fix |
| Property format-specification corrections | No formatter implementation |
| Normal/planar channel and sample order, and one data block per image | Narrow loader supports one channel only; validator checks one image block but not decoded channel ordering |
| Equal display shadows/highlights semantics | No display-function implementation |
| UUID randomness language and RFC 9562 reference | No writer-generated UUID surface |
| URL/path location extends through the last `)` | External locations are currently unsupported; apply if support is added |
| LZ4 block form, one Zstandard frame per subblock, independently compressed contiguous subblocks, whole-block shuffle-before-split, unchanged incomplete shuffle tail, and `byteOrder` describing uncompressed samples | Validator accepts concatenated/skippable Zstandard frames as one subblock; other rules need explicit fixtures |
| Inline/embedded checksum is over decoded binary; compressed checksum is over stored compressed bytes; SHA-3 means FIPS 202 | Current full validator follows this ordering and has published checksum-vector tests |
| Table string-cell examples | Editorial |
| Observation location is geodetic: east/north positive and height above reference ellipsoid, default WGS84 | Current XISF property parser does not map these values; avoid reusing `Mount`'s “above sea level” wording for future mapping |
| Equinox is inapplicable to ICRS/GCRS | No XISF coordinate-frame interpretation |
| Slope-map values are proportional to angle | No slope-map interpretation |
| Embedded ICC flag exception | No ICC decoding/validation |
| FITS keyword names are unpadded; value padding is optional and discouraged | Parser does not robustly trim permitted value padding and can change or fail metadata extraction |
| First block-index node begins at byte 16 | No block-index implementation; existing full validator correctly treats the monolithic XML header as beginning at byte 16 |
| Property identifiers support namespaces | No identifier validation; the string-pattern property extractor is too narrow |
| Adaptive display-function inversion/averaging correction | No adaptive display-function implementation |
| `Observation:Center` is the telescope aim, not the astrometric solution; the solution takes precedence for positional computation | Current workflows do not compute sky positions from XISF properties; preserve source distinctions in any future model |
| Corrected examples | Fixture guidance only |

## Impact and priority matrix

Priorities describe RavenSky work, not the importance assigned by the XISF
authors. “Current nonconformance” means the present implementation accepts,
rejects, or interprets a valid/invalid XISF unit incorrectly. A deliberately
narrow API is called a capability gap where it makes no broader conformance
claim.

| ID | Priority | Area | Classification | Current behavior and risk | Revision 1 trigger |
|---|---|---|---|---|---|
| F1 | P0 | `astro-io` pixel decoding | Current nonconformance | Always reads `UInt16` as little-endian, silently corrupting valid big-endian samples | Explicit baseline support for both byte orders; §10.4 clarification |
| F2 | P1 | Shared monolithic header parsing | Current nonconformance | Pixel and metadata readers start XML at byte 12, consuming reserved bytes and dropping four XML bytes; unit fixtures encode the same invalid prefix | Prefix/block-index clarification and prerequisite for all new features |
| F3 | P1 | Pixel-loader compression | Revision 1 capability gap | Public loader rejects all compressed images; validator can decode zlib/LZ4/Zstandard | Mandatory `zstd`/`zstd+sh`; §7 baseline codec set |
| F4 | P1 | Zstandard subblock validity | Revision 1 nonconformance | Streaming validator accepts concatenated or skippable frames inside one XISF subblock, and a regression test expects acceptance | Exactly one Zstandard frame per compression subblock |
| F5 | P1 | XML extensions | Revision 1 nonconformance | Any foreign namespace is rejected, including valid root extension elements; AstroMuninn monitoring can reject an otherwise valid file | Extension model in §§7 and 9.5 |
| F6 | P1 | FITSKeyword metadata | Current interoperability defect exposed by clarification | Attribute/string scanning does not XML-decode values or robustly handle permitted padding; numeric/date/path metadata can be lost or altered | §11.6 padding and name clarification |
| F7 | P2 | Core schema invariants | Conformance hardening | No duplicate `Image/@id` check, mandatory/unique `Metadata` check, required metadata-property check, property-ID syntax check, or known-child model | New uniqueness/schema language; existing §11.4 requirements |
| F8 | P2 | Detached XML signature | Revision 1 compatibility gap | Single-root XML parser rejects the detached signature document structure even when the XISF root is otherwise valid | Expanded §9.5 signed-document description |
| F9 | P2 | Standard/new property preservation | Data-visibility gap | Metadata properties are matched by fragile exact strings; scalar `value=` forms and new standard properties are missed; astrometric solution is invisible | §11.4.2 and §11.5.3.7 |
| F10 | P2 | Scalar/property lexical forms | Conformance coverage gap | No consolidated parser/tests for numeric boolean forms, new nonfinite forms, strict UTF-8, or empty inline aggregates | §§8.3-8.4 additions/corrections |
| F11 | P2 | Pixel/compression rule coverage | Regression-risk gap | No authoritative cases for shuffled multi-subblock tails, big-endian compressed samples, exact data length, or channel/sample-order rules | §§8.5.3, 10.4, 10.6 clarifications |
| F12 | P3 | Typed astrometry | Optional semantic feature | No typed model or projection evaluation; unknown information is mostly not surfaced | New standard `AstrometricSolution` |
| F13 | P3 | Color semantics | Optional semantic feature | RGB working space, color transforms, display functions, and ICC semantics are not interpreted | Annex B and related corrections |
| F14 | P3 | Distributed units/block indexes/external locations | Existing scope limitation | Validator is monolithic/local; block-index and URL/path features are unsupported | Block-index recommendation and URL/path clarification |

## Detailed findings

### F1 — P0: valid big-endian pixels are silently decoded incorrectly

`astro-io/src/xisf.rs:225-250` checks image geometry and reads every sample
with `read_u16::<LittleEndian>()`. The image `byteOrder` attribute is not parsed.
The full validator accepts the lexical byte-order value but does not return
decoded pixels, so it cannot compensate for the loader.

This is worse than an unsupported-feature error: the call succeeds with wrong
scientific data. The minimum safe behavior is to parse `byteOrder`, dispatch to
big- or little-endian decoding, default exactly as the specification requires,
and reject unknown values. Exact decoded byte length should be enforced rather
than accepting an unexplained suffix.

Acceptance evidence:

- a two-sample big-endian fixture yields the same numeric values as its
  little-endian equivalent;
- an unknown `byteOrder` is a typed error;
- truncated and oversized data blocks are rejected deterministically; and
- the normalized result is explicitly tested against the chosen sample-bounds
  policy.

The loader currently divides raw values by 65535 and ignores image `bounds` and
`offset`. That behavior should be reviewed in the same pixel-semantics design,
but it is a pre-existing scope question rather than a Revision 1 change.

### F2 — P1: two readers use a 12-byte prefix instead of the required 16 bytes

The monolithic prefix is the 8-byte signature, 4-byte XML-header length, and 4
reserved zero bytes. `astro-io/src/xisf.rs:39-55` and
`astro-metadata/src/xisf_parser.rs:19-113` read only the first 12 bytes before
reading XML. Their tests construct the same shortened prefix, while
`astro-io/src/validation/xisf.rs:432-570` and
`astro-bench/src/fixtures.rs:460-540` correctly use 16 bytes.

The metadata reader compounds the issue with `if let Ok(xml_content)`, returning
default/partial metadata after header extraction failure, and with
`String::from_utf8_lossy`, despite the format's strict UTF-8 requirement.

One internal parser should own signature, length, reserved-byte, range, UTF-8,
and XISF-root extraction checks. Both public consumers should propagate an
actionable error. Sharing this code avoids a third dialect of the prefix.

### F3 — P1: add Zstandard support at the actual pixel-loading surface

`astro-io/src/validation/xisf.rs:572-607` already supports zlib, LZ4/LZ4HC, and
Zstandard, including shuffle variants. `astro-io/src/xisf.rs:103-140` rejects
any compression attribute. Revision 1 makes Zstandard a standard mandatory
decoder codec and the announcement says PixInsight 1.9.5 writes the revised
format. It is not yet established whether ordinary 1.9.5 image output selects
Zstandard by default; an application-produced fixture is required.

The implementation should reuse the validator's bounded codec and unshuffle
primitives rather than introduce another decompressor. Unsupported or
resource-limited images must return a typed per-object error; broader callers
that enumerate objects should remain able to access unaffected objects as §7
requires.

### F4 — P1: exactly one Zstandard frame is allowed per XISF subblock

`astro-io/src/validation/streaming.rs:96-160` loops until all compressed input is
consumed, and `frame_window` at 53-95 treats a skippable frame as a zero-window
frame. Consequently, `astro-io/tests/validation.rs:433-455` explicitly accepts
concatenated/skippable Zstandard frames within one declared XISF subblock.
Revision 1 now states that each subblock contains exactly one Zstandard frame.

Change the XISF layer to reject a second or skippable frame after the declared
frame rather than weakening the generic bounded decompressor if that utility
has legitimate non-XISF callers. Reverse the existing test expectation and add
one valid frame per each of multiple XISF subblocks.

### F5 — P1: accept only the extension location the schema permits

`astro-io/src/validation/xisf.rs:26-143` uses namespace-aware XML events but
rejects any element outside the core namespace. Revision 1 permits extension
elements from a foreign namespace as children of the XISF root and instructs
decoders to ignore unknown extensions. It does not permit unknown extension
children arbitrarily nested inside core elements.

Implement a depth-aware skip only when a foreign-namespace start element occurs
as a direct root child. Tests must prove a valid extension subtree is ignored,
the next core image is still validated, an unknown nested core child fails, and
an extension nested within `Image` fails. AstroMuninn's monitor validator should
then stop classifying the valid root-extension case as unsupported.

### F6 — P1: replace FITSKeyword string scanning with XML event parsing

`astro-metadata/src/xisf_parser.rs:117-157` assumes a self-closing tag shape,
does not unescape XML attributes, drops comments, strips quotes without first
normalizing allowed padding, and depends on attribute spelling/order elsewhere.
Revision 1 clarifies that FITS keyword names are not padded while value padding
is permitted, though discouraged.

Use the same XML event stream as the shared header reader. Preserve name, value,
and comment separately; XML-decode attributes; parse a normalized semantic copy
without overwriting the raw serialized value. This matters downstream because
AstroMuninn uses extracted object, frame, filter, instrument, date, binning, and
gain values to build destination paths.

### F7–F11 — P2 conformance and preservation work

Add schema checks incrementally rather than treating a large XSD engine as a
prerequisite. The first set should cover invariants with direct safety or
interoperability value:

- exactly one mandatory `Metadata` element;
- required unique `XISF:CreationTime` (`TimePoint`) and
  `XISF:CreatorApplication` (`String`) properties;
- unique nonempty `Image/@id` values when present;
- property identifier syntax, including namespace-qualified identifiers;
- known core child placement while ignoring unrecognized attributes and
  properties as required; and
- strict UTF-8 and the supported scalar lexical forms.

This will require updating synthetic positive fixtures that currently omit
mandatory metadata. Those fixtures should not be grandfathered as “legacy”:
the requirements predate Revision 1.

For XML signatures, separate the monolithic XISF root byte range from the
optional detached signature rather than feeding two top-level elements to a
single-root parser. Verification of the cryptographic signature can initially
remain an explicit unsupported capability, provided unsigned content can be
read without rewriting signed bytes and policy is documented.

New standard properties and unknown property namespaces should be preserved in
a lossless/raw representation before adding public typed models. That permits
AstroMuninn's inspector to display Revision 1 data without prematurely defining
projection or coordinate semantics. Any public API addition must be additive
and reviewed for semver impact.

### F12–F14 — P3 deferred breadth

Typed `AstrometricSolution` support is valuable for future plate-solution and
coordinate workflows, but AstroMuninn currently organizes from FITS-like
metadata and does not calculate celestial positions. The safe near-term
requirement is that these properties neither reject the file nor disappear from
raw inspection. Before typed support is designed, capture PixInsight 1.9.5
files with several projections and compare the standard properties with any
legacy FITS WCS cards emitted in the same files.

Color transforms, RGB working spaces, display functions, ICC interpretation,
distributed units, block indexes, and external URL/path locations have no
present production implementation. They should remain explicitly unsupported
rather than partially interpreted. A future feature request should separately
define resource budgets, trust boundaries, URI policy, and scientific use cases.

## Reader, writer, and public-API consequences

### Readers

- Full validation is closest to Revision 1 readiness: standard codecs and
  mandatory checksums already work, range arithmetic is checked, and the
  monolithic prefix is correct.
- Pixel loading is a narrow reader, not a conforming baseline decoder. Its
  documentation should continue to state scope precisely until compression,
  byte order, and minimum sample/color capabilities are aligned.
- Metadata extraction must stop treating malformed XISF as successful empty
  metadata. This is observable behavior but corrects false success.
- Unsupported features should identify the affected object and allow
  enumeration/processing of independent objects where the API supports it.

### Writers

No RavenSky production XISF writer was found. The `astro-bench` fixture builder
is test/benchmark infrastructure, so encoder-specific rules such as UUID
generation, block-index identifiers, serialized RGB luminance coefficients,
and old-decoder codec selection are currently N/A. If that builder is promoted
to shared fixture generation, it should emit mandatory metadata and all 16
prefix bytes.

### Public APIs and semver

- Correct endian decoding and strict prefix parsing are bug fixes within the
  documented loader contract.
- New compression support can be additive without widening function signatures
  if errors and resource budgets are reused.
- Returning errors that were previously swallowed by metadata extraction may
  alter callers; release notes should identify the corrected behavior.
- A raw-property view should be added before a typed astrometry API. Avoid
  exposing XML-parser types or projection internals in shared public APIs.
- The `ravensky-astro` facade re-exports the crates, so public additions and
  behavioral corrections propagate through the umbrella crate.

## AstroMuninn impact

| Workflow | Revision 1 effect | Planned mitigation |
|---|---|---|
| File monitoring/validation | Valid root extension can currently be classified unsupported; exact-frame invalid Zstandard can currently pass | F4/F5 validator fixes and product-level regression cases |
| Metadata extraction | Incorrect prefix, lossy UTF-8, swallowed errors, and FITSKeyword padding can yield missing/altered metadata | F2/F6 shared parser and error propagation |
| Destination planning | Missing object/filter/date/instrument metadata can change the generated path or fallback classification | Golden organization-plan tests using corrected fixtures |
| Raw inspector | Reads FITSKeyword cards/header map only; new standard metadata and astrometric solution are invisible | P2 raw-property preservation and display |
| Pixel processing | AstroMuninn does not call `load_xisf`; F1 does not currently corrupt its output | Fix shared library before any future product pixel use |
| Distribution | Product depends on released crates | Publish/bump shared crates, then update lockfile/dependencies in a coordinated change |

An important open compatibility question is whether PixInsight 1.9.5 continues
to emit legacy FITS WCS keywords alongside `AstrometricSolution`. If it does
not, AstroMuninn will not currently display plate-solution metadata, though its
existing destination schema does not appear to depend on WCS.

## Test and fixture strategy

### Required authoritative fixtures

No official `.xisf` or `.xisb` sample files were found in the official
specification repository, and the official pages inspected did not expose a
downloadable Revision 1 sample corpus. Synthetic byte fixtures are necessary
for deterministic edge cases, but at least one real writer-produced corpus is
required before claiming PixInsight interoperability.

Obtain or generate with PixInsight 1.9.5 or later:

1. a small uncompressed `UInt16` little-endian image with mandatory metadata;
2. the equivalent big-endian image, if the application can write it;
3. `zstd` and `zstd+sh` images, including multiple compression subblocks and a
   final shuffle group with an incomplete tail;
4. an image containing the standard `AstrometricSolution` property set, with
   the exact projection and any co-emitted FITS WCS cards recorded;
5. a file containing a foreign-namespace root extension;
6. a signed file with the detached XML signature structure;
7. FITSKeyword elements with padded/unpadded values, XML-escaped attributes,
   comments, and realistic AstroMuninn routing metadata; and
8. a multi-image file with unique IDs and one unsupported object to exercise
   “rest remains accessible” behavior.

Record the producing application/build, export settings, checksums, expected
dimensions/sample values, and licensing/provenance alongside each committed or
externally fetched fixture. If fixtures are too large for the repository, store
immutable hashes and a reproducible generation/download procedure.

### Synthetic positive and negative cases

| Area | Positive cases | Negative cases |
|---|---|---|
| Prefix/XML | 16-byte prefix, strict UTF-8, legal whitespace | nonzero reserved bytes, overflow/truncation, invalid UTF-8, 12-byte legacy test prefix |
| Endian/sample | equivalent LE/BE `UInt16`, exact byte count | unknown byte order, odd/truncated/oversized data |
| Zstandard | one frame per subblock; multi-subblock `zstd+sh`; unchanged tail | concatenated frames, skippable+data frame, declared-size mismatch, budget excess |
| Extensions | ignored foreign root subtree followed by valid core image | foreign/core unknown child nested inside core content |
| Metadata/schema | one Metadata, required properties, unique image IDs | missing/duplicate Metadata or required properties; duplicate IDs; invalid property ID |
| Scalars | `true`, `false`, `1`, `0`; all nonfinite spellings; empty vector/matrix | malformed lexical forms, invalid aggregate length |
| FITSKeyword | padded semantic values, raw preservation, entities/comments | malformed entity, missing required attributes, invalid numeric semantic value |
| Signature | unsigned and detached-signature envelope parsing | extra unrelated top-level content, altered signed byte range |
| Downstream | stable AstroMuninn metadata and destination plan | typed diagnostic rather than silent empty metadata |

### Existing tests and changed expectations

The following focused suites passed during assessment:

- `cargo test -p astro-io --test validation`: 39 passed;
- `cargo test -p astro-io xisf`: 10 passed and 1 ignored in the unit target,
  with four matching integration cases also passing; and
- `cargo test -p astro-metadata xisf`: 3 passed.

These results are a baseline, not evidence of Revision 1 conformance. In
particular, the existing test named
`zstd_concatenated_and_skippable_frames_are_accounted_independently` confirms
the behavior that Revision 1 now forbids, and the loader/metadata unit fixtures
confirm the incorrect 12-byte prefix convention. Those expectations must be
changed, not retained as compatibility behavior.

## Kani and bounded-proof candidates

Kani is appropriate for pure range/index transformations and checked arithmetic,
not for proving a native codec or an XML library correct.

High-value harnesses:

1. **Monolithic range arithmetic.** For symbolic file length and header/block
   lengths, successful prefix/header/attachment range construction never
   overflows and every returned range is within the file.
2. **Image byte-count arithmetic.** For symbolic dimensions, channels, and
   bytes per sample within configured bounds, `Ok(n)` is the mathematical
   product, all intermediate operations are checked, and allocation limits are
   honored.
3. **Byte unshuffle permutation.** For bounded element size/count, every output
   position is initialized exactly once, indices remain in bounds, round-trip
   shuffle/unshuffle is identity, and an incomplete tail remains unchanged.
4. **Zstandard frame-header cursor logic.** For a bounded symbolic byte slice,
   frame-window parsing never panics or reads out of bounds and the XISF wrapper
   cannot accept a second/skippable frame as part of the same subblock.

Keep normal unit/property tests for XML event structure, filesystem I/O,
checksum vectors, native Zstandard decompression, floating-point projections,
and color transforms. End-to-end fixtures remain necessary even if all proposed
harnesses pass.

## Prioritized implementation plan and acceptance criteria

### Phase 1 — eliminate silent and foundational parser defects

1. **F2: shared monolithic header/XML reader (P1 prerequisite).** Move prefix,
   length/range, reserved-byte, strict UTF-8, and root-boundary logic into a
   reusable internal primitive. Convert both readers and correct their fixtures.
   Acceptance: all valid fixtures begin XML at byte 16; malformed/truncated/
   non-UTF-8 input returns a typed error; no metadata error is silently changed
   into successful empty metadata.
2. **F1: byte-order-correct pixel decoding (P0).** Parse and apply `byteOrder`,
   reject unknown values, and require exact decoded data length. Acceptance:
   golden LE/BE pixel equality plus truncation/suffix/error tests.
3. **F6: XML-event FITSKeyword parsing (P1).** Preserve raw name/value/comment
   and normalize only for typed interpretation. Acceptance: padded and escaped
   values produce stable metadata and AstroMuninn destination plans.

### Phase 2 — Revision 1 codec and extension compatibility

4. **F4: one Zstandard frame per subblock (P1).** Add the XISF-specific frame
   boundary check and reverse the concatenated/skippable acceptance test.
5. **F3: bounded `zstd`/`zstd+sh` pixel loading (P1).** Reuse validation codec,
   budget, subblock, and unshuffle logic. Acceptance: synthetic plus PixInsight
   fixtures decode to known samples without unbounded allocation.
6. **F5: root extension skipping (P1).** Add namespace/depth-aware root-only
   skip logic. Acceptance: legal extension accepted, nested illegal variants
   rejected, later images still validated.

### Phase 3 — structural conformance and information preservation

7. **F7: high-value schema invariants (P2).** Enforce Metadata/required-property
   and image-ID rules, then update all positive fixtures to be conformant.
8. **F8: detached-signature envelope parsing (P2).** Locate/preserve the signed
   XISF root bytes; expose verification status without rewriting content.
9. **F9/F10: raw property model and scalar lexical parser (P2).** Correctly
   expose standard metadata properties and preserve unknown/astrometric values.
   Add boolean, nonfinite, empty aggregate, namespace, and UTF-8 cases.
10. **F11: clarified storage regression corpus (P2).** Add multi-subblock,
    shuffle-tail, byte-order, exact-length, LZ4-block, and sample-order cases.

### Phase 4 — demand-driven semantic breadth

11. **F12: typed astrometry (P3).** Proceed only with a concrete consumer,
    authoritative fixtures, projection-domain tests, and an additive API design.
12. **F13: color semantics (P3).** Treat color transforms, RGB working spaces,
    display functions, and ICC profiles as a separate numerically tested effort.
13. **F14: distributed/block-index/external data (P3).** Define URI/security,
    random-access, cancellation, size, and trust policies before implementation.

### Release and downstream acceptance

Before release:

- run formatting, Clippy, and the full relevant crate test suites;
- run AstroMuninn Rust tests and golden organization-plan tests against the
  released/path-patched candidate crates;
- record peak-memory and cancellation behavior for compressed fixtures under
  validation budgets;
- confirm valid 1.00 fixtures remain accepted;
- confirm each newly rejected case is invalid under 1.00 or 1.01, not a valid
  legacy form; and
- publish shared crates and update AstroMuninn dependencies as an explicit
  coordinated step.

## Risks and tradeoffs

- **Conformance labels:** the narrow pixel API should not be advertised as a
  full baseline decoder until its supported object/sample/color set is widened.
- **Strictness rollout:** new invariant checks will expose invalid internal test
  fixtures and possibly permissively accepted third-party files. Diagnostics
  must distinguish malformed from merely unsupported.
- **Parser unification:** reusing one structural reader reduces divergence, but
  validation and metadata extraction need different output and budget needs;
  share syntax/range primitives, not an oversized all-purpose object model.
- **Signature handling:** parsing an envelope is not signature verification.
  Do not imply authenticity until trust anchors, algorithms, and policy exist.
- **Unknown properties:** ignoring for semantics is required; discarding them
  from a user-facing raw inspector is avoidable information loss.
- **Astrometry:** the new standard model is scientifically sensitive. Coordinate
  frames, equinox applicability, units, projection domains, and precedence over
  `Observation:Center` need authoritative examples before typed calculations.
- **Memory/concurrency:** compressed decoding must retain existing bounded
  allocation/cancellation guarantees and account for native codec memory.

## Open questions

1. Does PixInsight 1.9.5 select `zstd`/`zstd+sh` by default, and with which
   subblock size and compression level?
2. Does it emit legacy FITS WCS cards alongside `AstrometricSolution`, and are
   they intentionally equivalent in every supported projection?
3. Can PixInsight generate big-endian, signed, extension-bearing, and
   multi-image reference files, or must another conforming encoder provide them?
4. Should `load_xisf` remain intentionally narrow with explicit unsupported
   errors, or evolve into the crate's baseline-conforming decoder surface?
5. What is the intended normalization behavior for integer image `bounds` and
   `offset` in the public `Vec<f32>` loader?
6. Should raw XISF properties be exposed through `astro-metadata`, a lower-level
   `astro-io` document model, or both?
7. Is cryptographic XML signature verification a product requirement, or is
   structurally compatible preservation/status reporting sufficient?
8. What fixture redistribution terms apply to files generated by PixInsight?

These questions do not block Phase 1. Questions 1-3 should be answered before
claiming Revision 1 interoperability; questions 4-7 affect later API scope.

## Assessment verification

This was a read-only code/specification assessment. No implementation files,
tests, manifests, public APIs, or pre-existing documentation were changed. The
only intended workspace addition is this report.

Executed behavioral checks:

```text
cargo test -p astro-io --test validation
    PASS: 39 passed, 0 failed

cargo test -p astro-io xisf
    PASS: 10 passed, 0 failed, 1 ignored in the unit target;
          4 matching integration tests passed

cargo test -p astro-metadata xisf
    PASS: 3 passed, 0 failed
```

The passing tests establish the current baseline only. They do not negate F1-F6
because several assertions encode the currently incorrect/obsolete behavior.

## Command and source-retrieval audit

The following commands were run during the assessment. Repeated focused source
inspection commands are grouped by executable and list every target/intent; no
mutation command was run before creation of this report directory/file.

### Workspace orientation and policy

```sh
cat /Users/dean/.codex/attachments/74524e38-2d5e-489c-8e12-aeb07f0a833b/pasted-text.txt
cat .agents/skills/impl/SKILL.md
cat .agents/skills/verify/SKILL.md
graft map
rg --files -g AGENTS.md
cat ravensky-astro/AGENTS.md
cat astromuninn/AGENTS.md
find ravensky-astro/docs/work -maxdepth 2 -type f
find astromuninn/docs/work -maxdepth 2 -type f
git -C ravensky-astro status --short
git -C astromuninn status --short
```

### Graph queries

The graph was queried with `graft ask ... --source` for the XISF reader,
validator, compression/checksum support, metadata extraction, public re-exports,
and AstroMuninn downstream consumers. Exact structural lookups included:

```sh
graft skeleton ravensky-astro/astro-io/src/xisf.rs
graft skeleton ravensky-astro/astro-io/src/validation/xisf.rs
graft skeleton ravensky-astro/astro-io/src/validation/streaming.rs
graft skeleton ravensky-astro/astro-metadata/src/xisf_parser.rs
graft skeleton ravensky-astro/astro-metadata/src/types.rs
graft skeleton ravensky-astro/ravensky-astro/src/lib.rs
graft skeleton astromuninn/src-tauri/src/organizer.rs
graft skeleton astromuninn/src-tauri/src/workflow.rs
graft skeleton astromuninn/src-tauri/src/monitor/validator.rs
graft callers load_xisf
graft callers extract_metadata_from_path
```

### Official source retrieval and comparison

```sh
curl -fsSL https://pixinsight.com/doc/docs/XISF-1.0-spec/XISF-1.0-spec.html -o /private/tmp/XISF-1.0-spec-r1.html
curl -A 'Mozilla/5.0' -fsSL https://pixinsight.com/doc/docs/XISF-1.0-spec/XISF-1.0-spec.html -o /private/tmp/XISF-1.0-spec-r1.html
pandoc -f html -t gfm /private/tmp/XISF-1.0-spec-r1.html -o /private/tmp/XISF-1.0-spec-r1.md
curl -A 'Mozilla/5.0' -fsSL https://pixinsight.net/dev/index.php?articles/xisf-1-0-specification-revision-1.22/ -o /private/tmp/xisf-r1-announcement.html
git ls-remote https://gitlab.com/pixinsight/XISF-specification.git
git clone https://gitlab.com/pixinsight/XISF-specification.git /private/tmp/XISF-specification
git -C /private/tmp/XISF-specification log --oneline --all --decorate
rg --files /private/tmp/XISF-specification
find /private/tmp/XISF-specification -type f \( -name '*.xisf' -o -name '*.xisb' \)
```

The first specification `curl` failed under restricted DNS; an approved
network retry reached the server but received HTTP 406, and the browser-user-
agent retry succeeded. Official pages were also opened/searched with the web
retrieval tool for the announcement, current specification, previous official
text, Zstandard behavior, and sample fixtures. No official binary sample was
located.

### Focused text and source inspection

`rg`, `sed -n`, and `nl -ba` were run against the following targets and literal
identifiers. These were read-only inspections; ranges were narrowed to the
definitions returned by Graft:

```text
/private/tmp/XISF-1.0-spec-r1.md
/private/tmp/xisf-r1-announcement.html
/private/tmp/XISF-specification/pidoc/XISF-1.0.0/**
/private/tmp/XISF-specification/pidoc/XISF-1.0.1/**

ravensky-astro/astro-io/src/xisf.rs
ravensky-astro/astro-io/src/validation/xisf.rs
ravensky-astro/astro-io/src/validation/streaming.rs
ravensky-astro/astro-io/tests/validation.rs
ravensky-astro/astro-io/README.md
ravensky-astro/astro-metadata/src/xisf_parser.rs
ravensky-astro/astro-metadata/src/types.rs
ravensky-astro/astro-bench/src/fixtures.rs
ravensky-astro/ravensky-astro/src/lib.rs
ravensky-astro/Cargo.toml

astromuninn/Cargo.toml
astromuninn/src-tauri/Cargo.toml
astromuninn/src-tauri/src/organizer.rs
astromuninn/src-tauri/src/workflow.rs
astromuninn/src-tauri/src/monitor/validator.rs
```

The searched identifiers/phrases included `Revision`, `Conformance`,
`AstrometricSolution`, `zstd`, `zstd+sh`, `byteOrder`, `checksum`, `SHA-1`,
`SHA-256`, `SHA-512`, `Metadata`, `Image`, `FITSKeyword`, `Extension`,
`XML signature`, `block index`, `BlockAlignmentSize`, `CreationTime`,
`CreatorApplication`, `Observation:Center`, `load_xisf`, `extract_metadata`,
`validate_file_with_budget`, and dependency declarations for `astro-io` and
`astro-metadata`.

### Behavioral tests and report creation

```sh
cargo test -p astro-io --test validation
cargo test -p astro-io xisf
cargo test -p astro-metadata xisf
mkdir -p ravensky-astro/docs/work/xisf-1.0-revision-1
```

The report itself was created with the workspace patch tool, as required by the
agent editing policy.

## Final report-only checks

The following report-only commands were run:

```sh
git diff --check -- docs/work/xisf-1.0-revision-1/ASSESSMENT.md
git diff --no-index --check /dev/null docs/work/xisf-1.0-revision-1/ASSESSMENT.md
git status --short
rg -n '^## (Executive summary|Revision 1 delta inventory|Impact and priority matrix|Test and fixture strategy|Kani and bounded-proof candidates|Prioritized implementation plan and acceptance criteria|Open questions|Command and source-retrieval audit|Final report-only checks)$' docs/work/xisf-1.0-revision-1/ASSESSMENT.md
rg -o '\| F[0-9]+ \| P[0-3] \|' docs/work/xisf-1.0-revision-1/ASSESSMENT.md | sort | uniq -c
command -v markdownlint-cli2 || command -v markdownlint || true
sed -n '700,760p' docs/work/xisf-1.0-revision-1/ASSESSMENT.md
rg -n 'Pending|TODO|TBD' docs/work/xisf-1.0-revision-1/ASSESSMENT.md || true
rg '^\| F[0-9]+ \| P[0-3] \|' docs/work/xisf-1.0-revision-1/ASSESSMENT.md | cut -d'|' -f3 | tr -d ' ' | sort | uniq -c
git status --short -- docs/work/xisf-1.0-revision-1/ASSESSMENT.md
wc -l docs/work/xisf-1.0-revision-1/ASSESSMENT.md
```

Results:

- The initial path-limited `git diff --check` returned clean but did not inspect
  the untracked report. `git diff --no-index --check` then identified three
  deliberate Markdown hard-break spaces in the heading metadata; they were
  removed. The final no-index whitespace check reported no errors (its exit 1
  denotes that the compared files differ, as expected).
- All nine required report sections were found.
- Finding IDs F1 through F14 occurred exactly once in the priority matrix; the
  verified distribution is P0=1, P1=5, P2=5, P3=3.
- The marker search found only its own audited command and this verification
  statement; no unresolved work marker remains.
- No Markdown linter executable was installed, so Markdown was checked through
  targeted structure searches, visual review, and Git's whitespace check.
- Worktree status showed this new report directory plus pre-existing changes to
  `.gitignore`, `AGENTS.md`, `.continue/`, and `.ignore`. Those unrelated files
  were not modified by this assessment.
