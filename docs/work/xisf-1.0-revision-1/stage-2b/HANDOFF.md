# Stage 2B handoff

## Goal and status

XISF Revision 1 Stage 2B is implemented and verified. `astro-io` is now the
sole XISF syntax owner, and `astro-metadata` consumes its narrow raw-record
boundary without parsing the envelope or XML.

## Final public raw-record API

`astro_io::xisf::read_metadata_records<R: Read + Seek>` returns owned,
non-exhaustive `XisfMetadataRecords` through private fields and read-only
accessors. The result exposes only:

- document `version` and historical `blockAlignment` lexical values;
- ordered `XisfFitsKeyword` name, value, and optional comment records;
- ordered `XisfProperty` identifier, declared type, and optional scalar
  attribute/direct-text value records; and
- ordered `XisfImageMetadata` records containing optional `id`, `geometry`,
  `sampleFormat`, `colorSpace`, `bitsPerSample`, `compression`,
  `compressionParameters`, `checksumType`, `checksum`, `xResolution`,
  `yResolution`, `resolutionUnit`, `displayFunction`, and
  `displayParameters` strings.

The values are XML-decoded but otherwise lexical and unnormalized. Keyword,
property, and image order is retained independently. No `quick-xml` event,
validator node/context, codec/storage object, or general document topology is
public.

## Shared parsing and removed legacy logic

- The pixel loader and metadata reader share one `read_monolithic_header`
  helper over the Stage 1 `MonolithicEnvelope` range contract.
- Raw metadata collection consumes `visit_xml` directly.
- `astro-metadata` no longer reads the XISF signature/length, assumes a 12-byte
  prefix, scans for an XML declaration or tags/attributes, trims at NUL bytes,
  or calls `from_utf8_lossy`.
- The historical parser was removed rather than retained as a fallback.

## Semantic behavior

Existing entry points and output types are unchanged. Semantic interpretation
still lives in `astro-metadata`, including first-Property precedence,
last-keyword-wins normalized fields/maps, ordered duplicate cards, binning and
attachment defaults, coordinate extraction, and derived session date.

Representative equipment, detector, filter, exposure, mount, environment,
WCS, document, color/display, attachment, duplicate-keyword, duplicate-property,
and coordinate behavior is covered. Two parser defects are deliberately
corrected: XML entities/comments are no longer lost, and permitted outer FITS
value padding/quotes are normalized on the semantic copy while the raw
`astro-io` value remains unchanged. Malformed XML, UTF-8, prefixes, and required
metadata-record attributes now return errors instead of empty metadata.

## Public API and dependencies

The change is additive in `astro-io`; `astro-metadata` public signatures and
types are unchanged. `astro-metadata` already had no direct XML/parser
dependency, so no manifest dependency could be removed. `quick-xml` remains an
`astro-io` implementation dependency.

## Kani and architecture

No Kani harness was added. This stage introduced event-driven string
collection and semantic projection, not a small pure bounded transformation
that would add value beyond the Stage 2A `ImageDescriptor::sample_layout`
candidate.

There were no architecture deviations. The public surface is smaller than a
document AST and contains only facts demonstrated by the historical semantic
consumer.

## Remaining Stage 3 boundary

Stage 3 still exclusively owns codec/subblock decoding, exact one-frame
Zstandard policy, `zstd+sh`, shuffle/unshuffle, bounded compressed pixel
loading, and root-only foreign extension acceptance. Stage 2B added none of
those behaviors.

Deferred Revision 1 metadata work remains full Property typing, vectors/
matrices/tables, typed `AstrometricSolution`, broader schema invariants,
detached signatures, color-transform semantics, distributed/block-index/
external storage, and a user-facing general raw-property model.

## Verification status

All required changed-scope checks and the full workspace test suite pass.
Workspace-wide Clippy remains blocked only by the same seven unrelated existing
example/benchmark findings documented in Stage 2A. See `EVIDENCE.md` for exact
commands and counts.
