# Stage 2A handoff

## Goal and status

XISF Revision 1 Stage 2A is implemented and verified. The compatibility loader
now decodes its supported `UInt16` subset with the declared byte order and exact
descriptor-derived lengths.

## Final decoder architecture

- The shared private structural layer owns `ByteOrder` parsing. The only legal
  spellings are `little` and `big`; omission resolves to `Little` per XISF
  §10.4. Structural envelope integers remain independently little-endian.
- `ImageDescriptor::sample_layout` returns the checked sample count and exact
  uncompressed byte count together.
- The loader resolves a private `SampleDecoding` from the descriptor. It keeps
  the supported primitive conversion, sample count, and exact byte count with
  the checked attachment range.
- `UInt16` conversion uses `u16::from_le_bytes` or `u16::from_be_bytes`; it does
  not use host-native reinterpretation.

## Exact-length semantics

The declared attachment extent must equal the geometry/sample-derived byte
count. Short extents, incomplete final samples, and extra bytes inside that
extent fail. Bytes elsewhere in the monolithic file remain legal because they
can represent other blocks or unused file content.

## Public API impact

None. `load_xisf(&Path) -> Result<(Vec<f32>, usize, usize)>` is unchanged, as
are its normalized `raw UInt16 / 65535` output semantics.

## Intentionally unsupported capabilities

The loader still rejects compression, inline/embedded/external storage,
multiple channels, and sample formats other than `UInt16`. No metadata,
astrometry, color, extension, signature, FITS, product, or publication work was
added.

## Residual risks and corpus evidence

- Synthetic fixtures prove exact LE/BE byte conversion with known values.
- Three local PixInsight 1.8.9-3 `UInt16` captures omit `byteOrder` and therefore
  exercise the little-endian default. One existing ignored loader test was run
  explicitly and passed.
- No writer-produced big-endian fixture is available, so PixInsight big-endian
  interoperability is not claimed.

## Kani

No harness was added. The private pure production function
`ImageDescriptor::sample_layout` is now a natural bounded target: for symbolic
nonzero dimensions and a supported sample width, success must equal the
mathematical sample and byte products, while every overflowing product must
return `Invalid`.

## Stage 2B and Stage 3 boundaries

Stage 2B adds the minimal raw-record boundary needed by `astro-metadata` and
migrates its historical XISF reader onto the shared structural path. It must
preserve the raw `FITSKeyword` and `Property` information needed for semantic
interpretation without exposing parser-library types or creating a public XISF
AST.

Stage 3 adds bounded codec/subblock handling and feeds exact uncompressed bytes
into the Stage 2A `SampleDecoding` seam. It owns Zstandard framing, shuffle
handling, and the related Revision 1 storage behavior. It should reuse
`ByteOrder` and `SampleLayout` rather than reopening the Stage 2A decoder
contract.

## Architecture deviations

None. The implementation preserves the Stage 1 private descriptor/event seam
and adds no public AST or decoder object model.
