# Stage 1 evidence

## Behavior target

- One authoritative 16-byte monolithic prefix and checked XML range.
- Strict UTF-8 and well-formed namespace-aware XML events.
- Shared image/storage facts for the validator and compatibility loader.
- Explicit rejection of historical 12-byte synthetic files.
- No validator policy, codec, budget, cancellation, or public API regression.

## Red

Command:

```text
cargo test -p astro-io xisf
```

The initial run failed 5 focused tests for the intended reasons:

- legacy 12-byte prefix was accepted;
- nonzero reserved bytes were accepted;
- qualified core namespace elements were not found by string scanning;
- malformed XML was accepted / UTF-8 diagnostics were not actionable; and
- truncated length/reserved fields lacked field-specific context.

## Green and refactor

Implemented a private streaming structural visitor, checked envelope/ranges,
typed image/storage descriptors, and explicit structural-to-validation error
mapping. The validator keeps its validation-only topology but no longer owns an
independent XML parser or prefix/range calculation. The loader no longer scans
XML strings or calculates a 12-byte XML offset.

The fixture helper now always emits signature + length + four reserved bytes +
XML, while a separately named helper constructs only the intentional negative
legacy form.

## Verification

Passed:

```text
cargo fmt --all --check
cargo test -p astro-io xisf
cargo test -p astro-io --test validation
cargo clippy -p astro-io --all-targets --all-features -- -D warnings
cargo test -p astro-io --all-targets
cargo test --workspace --all-targets
```

Observed final counts:

- focused XISF unit selection: 17 passed, 1 intentionally ignored;
- validation integration target: 42 passed;
- all `astro-io` targets: 34 unit passed, 1 intentionally ignored, 2 resource
  passed, 42 validation passed;
- full workspace tests: all executed tests passed; two pre-existing tests
  remained intentionally ignored.

Workspace-wide Clippy was also run. It passed all changed crates and then failed
on four unrelated, pre-existing `unnecessary_sort_by` findings in:

- `examples/shared/metadata_dump.rs:170`;
- `examples/shared/metadata_stats.rs:492`;
- `examples/shared/metadata_stats.rs:508`; and
- `examples/shared/metadata_stats.rs:519`.

The first item is shared by two example targets, which is why Clippy reported
more target failures than unique source findings. These files were not changed.

## Residual risk

- No writer-produced Revision 1 corpus was introduced; Stage 1 claims
  structural correctness for deterministic fixtures, not full PixInsight
  interoperability.
- The later metadata raw-record handoff and byte-order decoder migration remain
  intentionally outstanding.
- No new performance benchmark was needed because this stage adds no codec,
  threading, queue, or concurrency behavior; existing budget/cancellation tests
  remained green.

## Completion status

Complete and verified for Stage 1. Workspace-wide Clippy remains partially
blocked by unrelated example lints documented above.
