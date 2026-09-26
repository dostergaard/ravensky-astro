# Stage 2A — XISF pixel decoder correctness

## 1. Objective

Remove the silent big-endian `UInt16` corruption path from `load_xisf` while
preserving its public signature and narrow compatibility-loader role.

## 2. Constraints and non-goals

- Continue to support only uncompressed, single-channel, attachment-backed
  `UInt16` images.
- Preserve the Stage 1 envelope, XML event, descriptor, and checked-range
  architecture; decode from those descriptors rather than reparsing XML.
- Do not add codecs, channel layouts, sample formats, metadata behavior,
  public decoder types, FITS changes, or Stage 2B work.
- Preserve normalized `Vec<f32>` output (`raw UInt16 / 65535`).

## 3. Current state

- `ImageDescriptor` preserves `byteOrder` as an optional string and already
  computes checked geometry-derived byte counts.
- `parse_image_data_block` enforces an exact attachment extent, but the final
  pixel helper recalculates dimensions and always calls a little-endian read.
- The XISF specification allows exactly `little` and `big`; an omitted
  attribute defaults to little-endian.

## 4. Proposed design

- Parse `byteOrder` once in the shared structural layer into a private
  `ByteOrder` enum, applying the specification's little-endian default and
  rejecting other lexical values as malformed.
- Expose checked sample-count and byte-count operations from `ImageDescriptor`,
  with byte count derived from the checked sample count.
- Resolve the loader's supported sample decoder and exact sample/byte layout
  from the descriptor. Carry those facts with the resolved attachment range.
- Decode `UInt16` with `u16::from_le_bytes` or `u16::from_be_bytes` over exact
  two-byte chunks. The decoder receives the already resolved expected lengths,
  rechecks the supplied slice exactly, and never uses host-native endianness.

Resource model: the loader retains the XML buffer, one descriptor, one exact
payload buffer, and the output `Vec<f32>`, as in Stage 1. Decoding is serial and
single-pass; there are no queues, threads, native allocations, codecs, or new
cancellation boundaries. Concurrency would add no value to this small primitive
and is outside the compatibility API's current resource contract.

## 5. Affected areas

- `astro-io/src/xisf/structural.rs`
- `astro-io/src/xisf.rs`
- Stage 2A plan, handoff, and evidence documents

## 6. Execution steps

1. Add golden LE, BE, omitted-default, and invalid-byte-order loader tests.
2. Add exact, short, odd, oversized, descriptor-mismatch, and overflow tests.
3. Run focused tests and record the expected endian/length failures (red).
4. Add the typed structural byte-order contract and shared sample-count
   arithmetic.
5. Add the private resolved sample-decoding seam and explicit endian conversion.
6. Refactor comments and names while keeping focused tests green.
7. Run the required focused, crate, workspace, formatting, lint, and diff checks;
   refresh Graft and record evidence.

## 7. Verification strategy

- Synthetic golden values such as `0x0102` and `0xABCD`, whose reversed forms
  are observably different, prove LE/BE equivalence and guard interpretation.
- Unit tests cover decoder slice boundaries and descriptor arithmetic.
- Loader tests cover XML-to-descriptor-to-attachment integration and unchanged
  little-endian/default behavior.
- Run all commands required by the Stage 2A request, including focused XISF and
  validation tests, all `astro-io` targets, workspace tests, formatting,
  changed-package Clippy, workspace Clippy where practical, and `git diff
  --check`.
- No performance benchmark is warranted because allocation shape, threading,
  codec behavior, and asymptotic work are unchanged.

## 8. Risks and mitigations

- **Validator drift:** use the same `ByteOrder` parser for image descriptors and
  validator-only block descriptors; retain existing error-category mapping.
- **Length ambiguity:** compare geometry-derived bytes with the declared
  attachment extent exactly, while permitting unrelated bytes outside that
  declared range.
- **Capability creep:** keep recognized but unsupported formats and storage
  forms as explicit errors.
- **False interoperability claim:** synthetic fixtures prove byte semantics but
  do not establish PixInsight writer interoperability.

## 9. Open questions

None block Stage 2A. A future Kani pilot can target the pure checked
sample-count/byte-count production functions; no function will be exposed or
reshaped solely for a harness in this stage.
