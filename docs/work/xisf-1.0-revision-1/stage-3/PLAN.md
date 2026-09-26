# Stage 3 implementation plan

## 1. Objective

Implement the approved XISF 1.0 Revision 1 codec and extension slice: load
attachment-backed single-channel `UInt16` images stored as `zstd` or `zstd+sh`,
enforce one Zstandard frame per compression subblock in both loading and full
validation, reverse whole-block byte shuffling safely, and ignore only foreign
extension subtrees that are direct children of the XISF root.

## 2. Constraints and non-goals

- Preserve `load_xisf(&Path) -> Result<(Vec<f32>, usize, usize)>` and the Stage
  2A `ByteOrder` / `SampleLayout` / `SampleDecoding` boundary.
- Keep the loader limited to attachment-backed, single-channel `UInt16` images.
- Do not add other codecs to the loader, inline/embedded/external loading,
  multiple-channel decoding, sample formats, public document models, extension
  APIs, metadata typing, signatures, astrometry, color semantics, or product
  work.
- Preserve validator checksum ordering, levels, budgets, cancellation, and
  error classification. Existing valid zlib/LZ4 behavior remains unchanged.
- Treat the Revision 1 specification and governing assessments as authoritative;
  do not reopen Stage 1, 2A, or 2B decisions.

## 3. Current state

- `xisf::structural` owns the monolithic envelope, strict namespace-aware XML
  events, image descriptors, checked ranges, byte order, and sample layout.
- The loader reads an attachment into memory and passes exact uncompressed bytes
  to `SampleDecoding`, but rejects every compression descriptor.
- The validator privately parses compression/subblock attributes and streams
  zlib/Zstandard under `Context` budgets. Its Zstandard loop accepts multiple
  ordinary or skippable frames in one XISF subblock, contrary to Revision 1.
- The validator, loader, and metadata-record collector currently reject every
  foreign namespace element, including a legal direct root child.
- No Stage 3 writer-produced Revision 1 fixture is present. Existing local
  PixInsight 1.8.9-3 files provide only uncompressed interoperability evidence.

## 4. Proposed design

Add one private `xisf::codec` module shared by the loader and validator. It will
own:

- parsing and checked accounting for `compression` / `subblocks` descriptors;
- Zstandard frame-header/window inspection;
- a single-frame streaming decode primitive that requires exact compressed and
  decompressed consumption; and
- the bounds-safe whole-block unshuffle transform.

The shared compression descriptor will retain the base codec, total decoded
length, optional shuffle item size, and ordered stored/decoded subblock extents.
The structural image descriptor will retain the raw `subblocks` attribute so the
loader and validator call the same parser instead of reconstructing its rules.

For each Zstandard subblock, callers inspect the frame header through the shared
primitive, reserve or enforce the resulting history allowance, then decode once
with the decoder in single-frame mode. Success requires the decoder to consume
the complete declared subblock and emit exactly its declared output length.
Skippable frames are rejected during inspection; a second frame or any other
trailer leaves input unconsumed and is rejected.

The loader will concatenate independently decoded subblocks into one allocation
whose capacity is the descriptor-derived uncompressed image byte count. For
`zstd+sh`, it will unshuffle that complete concatenated block once and require
the shuffle item width to match the supported `UInt16` sample width. The result
then enters the unchanged Stage 2A `SampleDecoding` conversion.

The XML visitor will recognize a foreign-namespace direct child of the core
`xisf` root as an extension subtree and suppress all of that subtree's events.
A foreign element anywhere inside core content remains a structural error. This
central rule gives validation, loading, and metadata collection one placement
interpretation without exposing extensions publicly.

### Resource model

- CPU: decoding is serial per `load_xisf` call and per validator block, as it is
  today. The zstd stream API creates no RavenSky worker pool; application-level
  concurrency remains outside this crate.
- Memory: the loader streams each declared compressed subblock through a 64 KiB
  bounded input buffer, reserves exactly the descriptor-derived decoded byte
  count, and uses a 64 KiB decode scratch buffer. Shuffled data temporarily owns
  one additional exact decoded-size buffer. Pixel conversion retains the
  existing exact `f32` output allocation. Every `Vec` capacity request uses
  fallible reservation. A fixed private loader Zstandard window ceiling
  prevents attacker-selected native history growth; the validator continues to
  reserve window and scratch costs from its shared `MemoryBudget`.
- Storage: each compressed attachment byte is read once by the loader. Validator
  streaming behavior and checksum passes remain unchanged except that each
  declared subblock is decoded once rather than as a concatenated frame stream.
- Cancellation: the loader API has no cancellation contract. Validator decode
  checkpoints remain before subblocks and during every output iteration, and
  all reservations remain RAII-released on success, error, or cancellation.
- Aggregate admission and responsiveness remain application policy. This stage
  adds no queues, threads, telemetry, or global scheduler.

## 5. Affected areas

- `astro-io/src/xisf/codec.rs` — new shared private codec descriptor, frame, and
  unshuffle primitives.
- `astro-io/src/xisf/structural.rs` — retain `subblocks`; root-extension skip.
- `astro-io/src/xisf.rs` — compressed attachment materialization and tests.
- `astro-io/src/validation/xisf.rs` and `validation/xisf/streaming.rs` — consume
  shared descriptors/exact-frame decoding while retaining validation policy.
- `astro-io/tests/validation.rs` — Revision 1 frame/subblock/extension and
  resource regressions.
- `astro-io/Cargo.toml` and private proof modules only if focused Kani harnesses
  remain tractable.
- `docs/work/xisf-1.0-revision-1/stage-3/{HANDOFF,EVIDENCE}.md` plus loader
  documentation whose supported subset changes.

## 6. Execution steps

1. Record the focused pre-change XISF test baseline.
2. Add failing loader, validator, frame, unshuffle, descriptor-overflow, and
   extension-placement tests; run the narrow suites and record the red state.
3. Add the shared codec primitives and migrate validator Zstandard decoding and
   descriptor parsing to them.
4. Extend the private image descriptor minimally, decode loader subblocks into
   exact output, apply whole-block unshuffle, and feed `SampleDecoding`.
5. Implement root-only foreign extension skipping in the shared XML visitor and
   remove now-redundant consumer rejection paths.
6. Refactor names/comments, update crate docs, and rerun focused tests.
7. Add and run focused Kani harnesses for pure frame-header/subblock/unshuffle
   properties if the real production functions compile tractably; otherwise
   record the concrete reason for deferral.
8. Refresh Graft, audit callers/source spans, run the verification stack, and
   complete durable evidence and handoff documents.

## 7. Verification strategy

- Unit tests: compression lexical/subblock accounting; exact frame inspection;
  deterministic shuffle golden patterns, tail preservation, degenerate input,
  invalid item widths, and overflow-safe indexing.
- Loader tests: known `UInt16` values for `zstd`, `zstd+sh`, multiple subblocks,
  LE/BE samples, short/long declared decoded lengths, concatenated/skippable/
  trailing/truncated frames, and unchanged uncompressed behavior.
- Validator integration tests: one frame per subblock, rejection categories,
  budget/cancellation cleanup, descriptor overflow, and root-versus-nested
  foreign extensions.
- Metadata contract tests: legal root extensions are ignored without hiding
  later core records; nested foreign elements fail.
- Commands: narrow `cargo test -p astro-io ...`, then `cargo fmt --all --check`,
  changed-package Clippy with warnings denied, `cargo test --workspace
  --all-targets`, doctests/docs as repository checks require, and named Kani
  harnesses when added.
- Fixtures: synthetic fixtures establish conformance edges. Existing local
  PixInsight 1.8.9-3 captures only establish legacy uncompressed behavior;
  writer-produced Revision 1 codec interoperability remains unclaimed unless a
  suitable fixture is found during implementation.

## 8. Risks and mitigations

- Native zstd may normally accept concatenated/skippable frames. Single-frame
  mode plus exact logical input consumption and an explicit skippable-header
  rejection close that ambiguity at the XISF boundary.
- Decoder read-ahead could hide trailers. Both loader slices and validator
  `Input` implement a logical-consumption contract; the shared decoder checks
  that counter rather than an underlying file cursor.
- Per-subblock unshuffle would corrupt valid files. Tests use multiple subblocks
  whose split crosses shuffle planes and require unshuffle only after
  concatenation.
- Large declared metadata can cause allocation or arithmetic failures. Descriptor
  totals use checked `u64` arithmetic, conversions to `usize` are explicit, and
  output allocation is cross-checked against Stage 2A geometry first.
- Central extension skipping must not become permissive XML handling. Only a
  foreign namespace at depth one under the core root opens a skipped subtree;
  nested foreign elements outside that subtree remain errors.

## 9. Open questions

- No writer-produced PixInsight 1.9.5+ `zstd` / `zstd+sh` fixture is currently
  available, so synthetic evidence is expected to be the Stage 3 ceiling.
- Kani dependency-graph compatibility and proof runtime must be measured before
  deciding whether all three candidate properties are economical.
