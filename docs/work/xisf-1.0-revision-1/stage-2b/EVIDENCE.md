# Stage 2B evidence

## Behavior target

- One shared structural interpretation for XISF loading, validation, and raw
  metadata extraction.
- Ordered, XML-decoded raw `FITSKeyword` and `Property` records without parser
  types crossing crates.
- Existing `AstroMetadata` semantics and precedence retained.
- Malformed structure propagated as errors.
- Historical 12-byte, lossy UTF-8, and XML string-scanning paths removed from
  `astro-metadata`.

## Red

The initial raw-boundary test command was:

```text
cargo test -p astro-io metadata_records
```

It failed to compile at every test call because `read_metadata_records` did not
exist. This established the missing cross-crate seam before implementation.

After adding the raw boundary, the semantic migration command was:

```text
cargo test -p astro-metadata xisf -- --test-threads=1
```

It ran 5 tests with 3 passed and 2 failed for the intended product reasons:
the historical reader did not populate representative metadata from the valid
16-byte fixture, and malformed XML still returned successful empty metadata.

An initial pre-change baseline `cargo test -p astro-io xisf` also encountered
the already documented parallel temporary-path collision in one Stage 2A
file-backed test. The same test and all focused suites passed later; Stage 2B
fixtures are in-memory.

## Green and refactor

Implemented accessor-oriented raw records, a shared monolithic-header read,
and a collector directly over `visit_xml`. Migrated semantic projection to
those records and deleted the independent prefix/XML/string helpers. Public
rustdoc and both crate READMEs now state the raw-versus-semantic boundary.

Regression coverage proves:

- valid documents with no relevant records;
- single/multiple/repeated keywords and preserved order;
- XML-decoded escaped values/comments and raw padding preservation;
- direct-text, scalar-attribute, and absent Property values;
- selected image attributes and absent optional attributes;
- missing keyword/property required attributes;
- malformed XML and invalid UTF-8 propagation;
- legacy 12-byte rejection through `astro-metadata`;
- representative normalized metadata, duplicate precedence, coordinates, and
  document properties without an Image.

The deliberate behavior corrections are event-based entity/comment handling,
semantic normalization of permitted FITS value padding/quotes, and structural
failure propagation. No metadata precedence was redesigned.

## Final verification

Passed:

```text
cargo fmt --all -- --check
cargo check --workspace
cargo clippy -p astro-io --all-targets --all-features -- -D warnings
cargo clippy -p astro-metadata --all-targets --all-features -- -D warnings
cargo test -p astro-io xisf
cargo test -p astro-io --test validation
cargo test -p astro-metadata
cargo test --workspace --all-targets
git diff --check
graft build
```

Observed counts:

- focused `astro-io xisf`: 28 unit tests passed, 1 local-capture test ignored,
  plus 4 matching validation tests passed;
- validation integration target: 42 passed;
- `astro-metadata`: 19 unit tests and 1 integration test passed;
- full workspace: 155 passed and 2 intentionally ignored;
- both changed-crate Clippy commands passed with warnings denied;
- formatting, workspace check, diff whitespace check, and Graft refresh passed.

Workspace-wide Clippy was also attempted:

```text
cargo clippy --workspace --all-targets --all-features -- -D warnings
```

It stopped only on seven unrelated existing findings already recorded by
Stage 2A:

- four `unnecessary_sort_by` findings in
  `examples/shared/metadata_dump.rs` and
  `examples/shared/metadata_stats.rs`; and
- three `chunks_exact_to_as_chunks` findings in
  `astro-bench/tests/workloads.rs`.

No changed file produced a Clippy finding.

## Legacy-path and dependency audit

Updated Graft queries show `read_metadata_records` called by
`astro-metadata::xisf_parser::extract_metadata`; no `extract_attribute` symbol
remains. Exhaustive source searches found no `from_utf8_lossy`, XML tag search,
manual header-length decode, `read_exact`, or XML parser dependency in
`astro-metadata` production code. Remaining `XISF0100`/12-byte construction in
that module is test-only positive/negative fixture code.

`cargo tree -p astro-metadata --depth 1` confirms no direct XML/parser
dependency. No Stage 2B dependency change was necessary.

## Kani, performance, and deviations

No Kani target was added. No benchmark was required because the stage adds no
codec, thread, queue, concurrency, or pixel-allocation behavior. It retains one
XML buffer and selected owned record strings without materializing an event or
document tree.

There were no architecture deviations and no PlantUML files changed.

## Completion status

Complete and verified for Stage 2B. Workspace-wide Clippy is partially blocked
only by the unrelated findings listed above.
