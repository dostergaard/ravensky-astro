# Stage 2B — raw XISF metadata handoff

## 1. Objective

Make `astro-io` the sole owner of XISF syntax parsing and migrate
`astro-metadata` to a small owned raw-record API without changing its public
semantic entry points, normalized output model, or precedence rules.

## 2. Constraints and non-goals

- Reuse the Stage 1 envelope and streaming XML event path; do not add a parser,
  text scanner, lossy UTF-8 fallback, or independent prefix/range logic.
- Keep semantic conversion, precedence, defaults, and derived metadata in
  `astro-metadata`.
- Expose no `quick-xml` types, validation nodes/budgets, codec state, general
  document tree, or public XISF AST.
- Do not change FITS behavior or begin Stage 3 codec, subblock, shuffle,
  Zstandard, or extension work.
- Preserve malformed structural input as an error rather than successful empty
  metadata.

## 3. Current state

`astro-metadata::xisf_parser` independently reads a 12-byte prefix, scans for an
XML declaration, trims at a zero byte, converts through `from_utf8_lossy`, and
searches strings for elements and attributes. Header extraction errors are
swallowed. Its semantic layer consumes:

- ordered `FITSKeyword` name/value records (comments are currently lost);
- `Property` values for creator, creation time, and ICC-profile presence;
- root `version` and historical `blockAlignment` attributes;
- selected `Image` attributes for geometry, color/display metadata, and the
  existing `AttachmentInfo` projection.

Keyword order and duplicates feed ordered cards and a last-value-wins map.
Property lookup is first-match. Image-derived document fields use the first
image while attachment records retain image order.

## 4. Proposed design

Add an additive `astro_io::xisf::read_metadata_records` reader returning an
owned `XisfMetadataRecords`. It contains private storage and read-only accessors
for raw document attributes plus ordered `XisfFitsKeyword`, `XisfProperty`, and
`XisfImageMetadata` records. Record structs are non-exhaustive and accessor
oriented so the public commitment stays smaller than the XISF data model.

The API preserves XML-decoded but otherwise unnormalized lexical strings.
`FITSKeyword` requires `name` and `value`; `comment` is optional. `Property`
requires `id` and `type`, and preserves either a `value` attribute or direct
element text without semantic coercion. Selected image attributes are optional
raw strings because metadata extraction must not inherit the pixel loader's
narrow required-location/sample policy.

Internally, factor the existing monolithic header read into one helper shared
by the pixel loader and raw metadata reader. The new collector consumes
`visit_xml` events directly and rejects foreign namespaces in this stage,
leaving Revision 1 extension policy to Stage 3.

`astro-metadata` converts records into its existing `FitsHeaderCard`,
`XisfMetadata`, color, detector, and attachment structures. Keyword
normalization occurs only on a semantic copy; raw records remain unchanged.

Resource model: one XML buffer plus only the selected owned record strings is
retained. Parsing is serial and streaming with no event tree, threads, queues,
codecs, or new cancellation behavior. This metadata-only stage does not change
pixel or validator allocation policy and needs no performance benchmark.

## 5. Affected areas

- `astro-io/src/xisf.rs`
- `astro-io/src/xisf/structural.rs` only if the event seam needs a minimal
  syntax-level enhancement
- `astro-metadata/src/xisf_parser.rs`
- focused tests in both crates
- `docs/work/xisf-1.0-revision-1/stage-2b/{PLAN,HANDOFF,EVIDENCE}.md`

No Cargo dependency change is expected: `astro-metadata` currently has no XML
parser dependency, and `quick-xml` remains required by `astro-io`.

## 6. Execution steps

1. Add failing `astro-io` tests for empty metadata, ordered/repeated keywords,
   entity decoding, comments, properties, optional attributes, malformed
   records, malformed XML, and invalid UTF-8.
2. Add failing `astro-metadata` parity/error tests using correct 16-byte
   monolithic fixtures, including precedence and padding/entity cases.
3. Implement the public raw record types, shared header read, and event-based
   collector in `astro-io`.
4. Migrate `astro-metadata` semantic projection to the new records and remove
   every XISF XML/prefix/string-scanning helper.
5. Refactor with focused tests green, audit the legacy patterns exhaustively,
   refresh Graft, and complete the required verification stack.

## 7. Verification strategy

- Red/green focused tests in `astro-io::xisf` and `astro-metadata::xisf_parser`.
- Behavioral parity for representative equipment, detector, filter, exposure,
  coordinates, XISF document metadata, color/display data, and attachments.
- Explicit malformed prefix/XML/UTF-8/metadata-record error tests.
- Exhaustive source audit for `from_utf8_lossy`, manual `XISF0100`/12-byte
  handling, and XML string-search helpers in `astro-metadata`.
- Run every completion command in the task, followed by Graft refresh and a
  final status/diff audit.

## 8. Risks and mitigations

- **Semantic drift:** lock existing first/last precedence and defaults in tests
  before removing the scanner.
- **Accidental public AST:** expose only selected strings through accessors;
  retain no generic attribute map or XML topology.
- **Overvalidation:** require only metadata-record syntax needed for an
  unambiguous record; leave broader schema policy in validation.
- **Lossy normalization:** preserve raw decoded strings in `astro-io` and trim
  or parse copies only in `astro-metadata`.
- **Dirty Stage 2A worktree:** edit the current files in place and do not revert
  or restage the existing uncommitted Stage 2A changes.

## 9. Open questions

None block implementation. Full Property typing, raw-property presentation,
typed astrometry, schema hardening, detached signatures, and Stage 3 storage
work remain deferred.
