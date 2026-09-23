# Stage 1 handoff

## Goal

Implement the shared private XISF structural foundation and correct the
12-byte-prefix defect in `astro-io`.

## Current status

Implementation is complete and verified for the Stage 1 scope.

## Decisions already made

- Use a private streaming event/descriptor layer, not a public AST.
- Keep the validator's `Node` as validation-only policy state rebuilt from
  shared structural events.
- Preserve `load_xisf`'s signature and narrow supported pixel subset.
- Treat byte order, Zstandard frame policy, metadata handoff, and root extension
  acceptance as later stages unless extraction makes a safety fix inseparable.
- Keep the validator's complete private `Node` topology because conformance,
  embedded-data, reference, and checksum policy still needs parent/child state;
  all XML syntax and shared descriptors now enter that tree through the common
  structural event path.

## Why those decisions were made

They follow both governing assessments and avoid either retaining parser
dialects or coupling ordinary consumers to validation budgets/policy.

## Files/components changed

- Added `astro-io/src/xisf/structural.rs` for the envelope, checked ranges,
  strict XML events, namespace/depth facts, image descriptors, storage
  locations, and checked image byte counts.
- Migrated `astro-io/src/xisf.rs` and `astro-io/src/validation/xisf.rs` to the
  shared foundation.
- Corrected `astro-io` positive fixture construction and added explicit legacy
  12-byte negative fixtures/tests.
- Added `stage-1/{PLAN,HANDOFF,EVIDENCE}.md`.

## Validation completed

- Focused XISF red/green tests.
- Full `astro-io` targets.
- Full workspace test targets.
- Formatting and changed-package Clippy with warnings denied.
- Graft source/caller audit and graph refresh.

## Observed evidence/results

- The initial red run failed on legacy-prefix/reserved-byte acceptance,
  qualified namespaces, malformed XML, UTF-8 context, and prefix diagnostics.
- The final focused XISF run passed 17 tests with one local-capture test ignored.
- The full validation integration target passed 42 tests.
- All `astro-io` targets passed: 34 unit tests plus 2 resource and 42 validation
  tests; one local-capture test remained intentionally ignored.
- `cargo test --workspace --all-targets` passed.
- `cargo clippy -p astro-io --all-targets --all-features -- -D warnings` passed.

## Known failures or limitations

- Workspace-wide Clippy reaches unrelated examples but fails on four existing
  `unnecessary_sort_by` findings in `examples/shared/metadata_dump.rs` and
  `examples/shared/metadata_stats.rs`; changed-package Clippy is clean.
- `astro-metadata` retains its historical XISF reader until the later public
  raw-record handoff stage. Stage 1 intentionally adds no public parser API.
- The compatibility loader still has its pre-existing little-endian decoder;
  byte-order-correct decoding remains the explicitly separate consumer stage.

## Open questions

- None for Stage 1.

## Next concrete steps

1. In the next approved stage, add the small raw-record boundary needed by
   `astro-metadata` and migrate its historical reader.
2. Migrate the pixel decoder to explicit LE/BE behavior with complete golden
   coverage.
3. Keep Zstandard frame-policy and extension acceptance changes in their
   separately scoped Revision 1 stage.

## Things not to reconsider without new evidence

- No public AST/API is needed in Stage 1.
- The 12-byte fixture form is invalid and must not receive a compatibility path.
- Codec/metadata/extension work remains out of scope.
