# Stage 3 evidence

## Scope and environment

- Repository baseline: `74a0d0b8177ae4ad08df91faad6bb48ec6324739`.
- Rust/Cargo: 1.98.1.
- Kani: 0.68.0 with CBMC 6.11.0.
- The worktree already contained the coordinated Stage 1, 2A, and 2B changes;
  Stage 3 was implemented on top of them without reverting or widening them.

## TDD evidence

The focused pre-change XISF run was green: 28 unit tests passed with one
ignored, and four filtered validation integration tests passed.

The new tests then demonstrated the three confirmed gaps before implementation:

- valid attachment-backed `zstd` loading failed as unsupported compression;
- a concatenated Zstandard construction still validated successfully; and
- a foreign-namespace direct child of the XISF root failed as an unsupported
  namespace.

After implementation, focused green runs covered the shared codec, loader,
validator, metadata records, and extension placement. The final package run is
recorded below.

## Conformance coverage

Synthetic XISF fixtures exercise:

- valid `zstd` with deterministic `UInt16` pixels in both little- and
  big-endian byte order;
- valid `zstd+sh`, including two independently compressed subblocks whose split
  crosses shuffle planes and therefore requires whole-block unshuffle;
- exact declared decompressed length, including short and long output;
- descriptor/subblock stored and decoded totals, malformed lexical forms, and
  checked accumulation overflow;
- one standard Zstandard frame per subblock, with rejection of concatenated
  frames, arbitrary trailing bytes, leading/trailing skippable constructions,
  malformed signatures, truncation, and excessive history requirements;
- golden unshuffle bytes, an incomplete tail, empty input, zero/oversized item
  widths, and `UInt16` item-width agreement;
- legal ignored foreign extension subtrees directly below the XISF root,
  rejected foreign elements inside `Image` or `Metadata`, metadata isolation,
  and structure-budget accounting for ignored extension contents; and
- unchanged uncompressed attachment, LE, and BE behavior.

These are synthetic conformance tests. The explicitly ignored real-sample
loader regression passed against the available PixInsight 1.8.9-3 capture.

## Post-stage real-writer corpus

The local `tests/PI_XISF_data/` corpus now contains twelve intentionally
gitignored PixInsight-produced binaries: 1.9.4 and 1.9.5, each with large mono,
small mono, and small OSC/RGB `UInt16` sources saved compressed and
uncompressed. The tracked CSV manifest, README, and twelve generated
`.xisf.headers` files document the corpus without putting its approximately
822 MiB (862 MB) of binary data in Git.

All twelve files were successfully inspected with
`dump_xisf_metadata`; the generated dumps include semantic metadata, the raw
format summary, and the complete pretty-printed XML header. The compressed
files use writer-produced `zstd+sh:<decoded-size>:2` image descriptors, while
the image elements omit `byteOrder` and none declares `subblocks`. The OSC
files report three-channel RGB geometry and are useful inspection/validation
fixtures even though the compatibility loader deliberately remains limited to
single-channel `UInt16` images.

This is real-writer structural and diagnostic evidence, not a replacement for
the synthetic edge-case suite. In particular, it does not cover explicitly
declared big-endian byte order, subblocks, malformed/concatenated/skippable
frames, or resource-limit failures.

## Resource and failure evidence

- Compression and subblock lengths use checked `u64` accumulation and exact
  stored/decoded total comparisons.
- Loader output allocation is derived from Stage 2A geometry, uses fallible
  reservation, and is filled only by subblocks whose decoded totals equal that
  geometry. Compressed input is streamed through a bounded 64 KiB buffer rather
  than allocated from the declared stored size.
- Loader Zstandard history is inspected before decoder construction and capped
  at 128 MiB. The regression uses a valid-looking header advertising a
  multi-terabyte window and confirms deterministic rejection.
- Validator Zstandard input/output remains streamed in 64 KiB buffers. Rounded
  frame history plus the existing 1 MiB codec allowance is reserved from the
  shared `MemoryBudget` before native decoder construction.
- Validator cancellation checkpoints remain before subblocks and on each
  decode output iteration. Existing budget/cancellation integration tests pass
  and verify reservation release on all exits.
- Logical input-consumption counters exclude decoder read-ahead, allowing the
  shared decoder to reject any bytes after the first frame within the declared
  subblock.

## Kani

All three focused pure-Rust harnesses passed:

| Harness | Property | Result |
|---|---|---|
| `zstd_frame_header_is_bounded_and_standard` | header/range arithmetic, standard magic, bounded power-of-two history | 342 checks, 0 failed |
| `subblock_totals_never_wrap` | checked `u64` totals agree with a `u128` oracle for up to three extents | 310 checks, 0 failed |
| `unshuffle_indices_are_in_bounds` | every bounded valid target maps to an in-range source through the production index helper | 200 checks, 0 failed |

The first whole-`Vec` unshuffle harness attempt was stopped after becoming
allocator-model dominated; a reduced attempt exhausted CBMC memory. The final
harness proves the production index calculation directly, while ordinary tests
cover allocation, complete transforms, and incomplete-tail preservation. Native
Zstandard decoding was intentionally not modeled.

## Verification

Successful commands on the final implementation:

- `cargo fmt --all --check`
- `cargo clippy -p astro-io --all-targets --all-features -- -D warnings`
- `cargo test -p astro-io --all-targets --all-features -- --test-threads=1`
  — 98 passed, 0 failed, 1 ignored
- `cargo test --workspace --all-targets --all-features -- --test-threads=1`
  — 164 passed, 0 failed, 2 ignored
- `cargo test --workspace --doc --all-features` — 4 passed, 0 failed
- `cargo test -p astro-io test_load_xisf_real_sample -- --ignored
  --test-threads=1` — 1 passed
- the three named `cargo kani -p astro-io --harness ...` commands above — 852
  checks, 0 failed
- `git diff --check`
- `graft build`, followed by a Stage 3 source/caller audit

`cargo clippy --workspace --all-targets --all-features -- -D warnings` remains
blocked outside the changed package by seven pre-existing Rust 1.98 lint sites:
three `chunks_exact_to_as_chunks` findings in `astro-bench/tests/workloads.rs`,
one shared `unnecessary_sort_by` site in `examples/shared/metadata_dump.rs`, and
three `unnecessary_sort_by` sites in `examples/shared/metadata_stats.rs`. The
shared metadata-dump site is emitted once for each of two examples. No finding
was in `astro-io`; its warnings-denied Clippy run passed.
