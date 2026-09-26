# Stage 3 handoff

## Goal and status

Stage 3 is complete for the approved loader subset. Attachment-backed,
single-channel `UInt16` images can now load from uncompressed, `zstd`, and
`zstd+sh` storage without changing the public `load_xisf()` signature or the
Stage 2A sample-decoding contract.

## Final architecture

`astro-io/src/xisf/codec.rs` is the private shared format layer used by both the
loader and validator. It owns:

- lexical parsing of `compression` and `subblocks`;
- checked exact stored/decoded subblock accounting;
- Zstandard frame-header and history arithmetic;
- exact-one-frame streaming decode; and
- whole-block unshuffle plus its checked source-index calculation.

The structural `ImageDescriptor` now retains only the additional raw
`subblocks` value needed by this layer. The loader verifies that the compression
descriptor's decoded byte count equals the geometry-derived Stage 2A
`SampleDecoding::expected_bytes`, independently decodes each declared subblock,
concatenates the output, applies one whole-block unshuffle for `+sh`, and passes
the resulting exact byte block to the unchanged sample decoder.

The validator retains its existing broader codec support and policy, but uses
the same descriptor parser and exact Zstandard decoder as the loader. It does
not reconstruct shuffled pixels because shuffle is a byte permutation; it does
validate descriptor syntax, totals, and image sample-width agreement.

## Supported compression and frame policy

- Loader: `zstd:<decoded-size>` and
  `zstd+sh:<decoded-size>:<item-size>` on attachment storage. For the supported
  `UInt16` layout, the shuffle item size must be 2.
- Optional `subblocks="c1,u1:c2,u2:..."` partitions the stored stream into
  contiguous independently compressed parts. Stored and decoded sums must
  match their enclosing descriptors exactly.
- Every declared Zstandard subblock must contain exactly one ordinary Zstandard
  frame. Skippable frames, concatenated frames, frame trailers, malformed or
  truncated frames, and short/long decoded output fail deterministically.
- Shuffle reversal occurs only after all decompressed subblocks have been
  concatenated. It is independent of sample byte order and preserves an
  incomplete tail as required by the format transform.

## Resource and safety behavior

- All descriptor sums, ranges, frame-window calculations, index calculations,
  and platform-size conversions are checked.
- Loader compressed input uses a 64 KiB bounded buffer; output is a fallibly
  reserved geometry-derived allocation with a 64 KiB decode scratch buffer.
- The compatibility loader applies a RavenSky-specific 128 MiB Zstandard history
  ceiling, matching the upstream Zstandard streaming-decoder default
  (ZSTD_WINDOWLOG_LIMIT_DEFAULT = 27). This is a local resource-policy limit, not
  an XISF format limit. Consequently, load_xisf() may reject an otherwise valid
  XISF/Zstandard frame requiring more than 128 MiB of decoder history.
- Validator decoding retains the shared `MemoryBudget`, 64 KiB buffers,
  history-plus-1-MiB codec admission, cancellation checkpoints, and RAII
  release behavior.
- Decoder read-ahead cannot hide trailers because exactness uses logical bytes
  consumed from the declared subblock rather than the underlying file cursor.

## Extension elements

The namespace-aware structural visitor now centrally ignores a foreign-
namespace subtree only when its root is a direct child of the XISF document
root. Loader, metadata, and validator consumers therefore agree without a
public extension model. Foreign elements in core content remain structural
errors. Ignored extension events still consume validator structure/attribute
budget and cancellation checkpoints.

## Public API and scope

There is no public API signature change and no new public XISF document or
extension model. Existing loader limitations remain: attachment storage only,
one channel, and `UInt16`. Inline/embedded/external loading, other loader codecs
or sample formats, typed properties/astrometry, color semantics, signatures,
and later Revision 1 hardening remain out of scope.

## Files changed for Stage 3

- `astro-io/src/xisf/codec.rs` — new private shared codec layer and Kani proofs.
- `astro-io/src/xisf.rs` — bounded compressed loader path and regressions.
- `astro-io/src/xisf/structural.rs` — `subblocks` retention and root-extension
  traversal.
- `astro-io/src/validation/input.rs` — logical-consumption interface support.
- `astro-io/src/validation/xisf.rs` — shared descriptors and extension budgets.
- `astro-io/src/validation/xisf/streaming.rs` — shared exact-frame Zstandard
  decode with existing resource policy.
- `astro-io/tests/validation.rs` — frame, subblock, resource, shuffle-width, and
  extension placement regressions.
- `astro-io/Cargo.toml` — explicit `cfg(kani)` lint registration.
- `astro-io/README.md` — supported loader, frame, extension, and resource policy.
- this directory — plan, evidence, and handoff.

Other modified files in the worktree belong to the preceding coordinated
stages and were preserved.

## Validation and evidence

Final ordinary verification is green for formatting, `astro-io` Clippy with
warnings denied, package tests (98 passed, 1 ignored), workspace tests (164
passed, 2 ignored), doctests (4 passed), and whitespace checks. Three Kani
harnesses completed 852 checks with zero failures. See `EVIDENCE.md` for exact
commands, red/green history, resource cases, and the unrelated workspace-wide
Clippy limitation.

The synthetic conformance fixtures remain the authoritative edge-case suite.
After Stage 3, `tests/PI_XISF_data/` added a local twelve-file PixInsight 1.9.4
and 1.9.5 corpus covering compressed/uncompressed large mono, small mono, and
small OSC/RGB `UInt16` images. Its compressed files provide real-writer
`zstd+sh:<decoded-size>:2` diagnostic evidence; see the tracked corpus README,
manifest, generated XML/header dumps, and the post-stage section in
`EVIDENCE.md`. The binaries are intentionally ignored because the corpus is
approximately 822 MiB (862 MB).

## Deviations and remaining work

There was no architecture deviation. The loader implementation improved on the
plan by streaming compressed input instead of retaining the whole declared
compressed extent. The first allocation-heavy unshuffle proof shape was
replaced with a proof over the exact production index helper after CBMC memory
evidence; runtime golden and tail tests still exercise the full transform.

Remaining Revision 1 work is limited to later approved stages and deferred
capabilities. Do not reopen Stage 1/2A/2B contracts or widen the loader subset
without new requirements and evidence.
