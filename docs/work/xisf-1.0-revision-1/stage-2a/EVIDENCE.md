# Stage 2A evidence

## Behavior target

- Decode supported `UInt16` samples as explicit little- or big-endian data.
- Apply the specification's little-endian default when `byteOrder` is absent.
- Reject invalid byte-order syntax and nonexact declared image extents.
- Preserve the public loader signature and narrow supported subset.

## Red

Initial command:

```text
cargo test -p astro-io xisf -- --nocapture
```

After correcting a test-helper lifetime mistake, the intended red run produced
21 passed, 3 failed, and 1 ignored unit tests. The failures proved that:

- big-endian golden bytes were decoded as reversed little-endian values;
- `byteOrder="middle"` silently fell through to little-endian decoding; and
- the primitive decoder ignored a trailing byte.

## Green and refactor

Added typed shared byte-order parsing, one checked `SampleLayout`, a private
resolved `SampleDecoding`, explicit standard-library endian conversion, exact
slice checks, and golden/boundary regressions. Removed the now-unused
`byteorder` dependency.

Coverage includes explicit little and big endian, omitted/default little endian,
wrong-interpretation guard values, invalid syntax, exact data, one-byte-short
and incomplete samples, declared trailing bytes, bytes outside the declared
extent, unsupported sample format, and checked sample-count overflow.

## Verification

Passed:

```text
cargo fmt --all -- --check
cargo check --workspace
cargo clippy -p astro-io --all-targets --all-features -- -D warnings
cargo test -p astro-io xisf
cargo test -p astro-io --test validation
cargo test -p astro-io --all-targets
cargo test --workspace --all-targets
cargo test -p astro-io xisf::tests::test_load_xisf_real_sample -- --ignored --exact --nocapture
```

Observed final counts:

- focused XISF selection: 25 unit tests passed, 1 real-capture test ignored by
  default, and 4 matching validation integration tests passed;
- validation integration target: 42 passed;
- all `astro-io` targets: 42 unit, 2 resource, and 42 validation tests passed;
  1 real-capture test remained ignored by default;
- full workspace: 149 passed and 2 intentionally ignored;
- explicit local PixInsight capture run: 1 passed.

The first final focused run encountered the documented parallel temporary-path
collision in one file-backed test. Its isolated rerun and the exact focused
rerun passed. The first `astro-io --all-targets` run also saw an unrelated FITS
HCOMPRESS native round-trip failure; that test passed in isolation and the full
target rerun passed.

Workspace-wide Clippy was attempted. Changed-package Clippy passed, but the
workspace command stopped on seven unrelated existing findings:

- four `unnecessary_sort_by` findings in the previously documented example
  files; and
- three `chunks_exact_to_as_chunks` findings in
  `astro-bench/tests/workloads.rs` under the current toolchain.

Those unrelated files were not changed.

## Synthetic and real-writer evidence

The endian and exact-value assertions use deterministic synthetic fixtures.
The local real-writer corpus contains three PixInsight 1.8.9-3 `UInt16` files
with omitted `byteOrder`; one was loaded successfully as supplemental default-LE
evidence. No real-writer BE fixture exists, so no BE interoperability claim is
made.

## Kani and deviations

Kani was deferred. The exact future target and property are recorded in
`HANDOFF.md`. There were no deviations from the approved architecture or scope.

## Completion status

Complete and verified for Stage 2A. Workspace-wide Clippy remains partially
blocked only by unrelated files described above.
