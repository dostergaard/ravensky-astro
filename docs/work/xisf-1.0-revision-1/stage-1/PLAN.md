# Stage 1 — shared XISF structural foundation

## 1. Objective

Establish one private `astro-io` implementation for the XISF monolithic
envelope, strict XML syntax events, and image/storage descriptors. Migrate the
existing validator and `load_xisf` compatibility loader to it, correcting the
historical 12-byte-prefix behavior without expanding decoder capabilities.

## 2. Constraints and non-goals

- Preserve the public `load_xisf` and validation signatures.
- Keep validation categories, budgets, cancellation, checksums, and codecs.
- Keep the loader limited to uncompressed, single-channel, attachment-backed
  `UInt16` images.
- Do not expose `quick-xml`, validator nodes/context, codecs, reservations, or
  native handles.
- Do not add Zstandard loader support, root-extension policy changes, a public
  XISF AST, metadata migration, or broad Revision 1 decoding.
- Do not accept the defective 12-byte prefix as a compatibility form.

## 3. Current state

- `astro-io/src/xisf.rs` reads 12 bytes, scans XML strings, and has fixtures
  that omit the four reserved prefix bytes.
- `astro-io/src/validation/xisf.rs` reads the correct 16-byte prefix and has the
  strongest namespace-aware XML parser, but syntax is embedded in validation
  policy and represented by a validation-only `Node` tree.
- Validation fixtures already construct a correct 16-byte prefix.
- The loader and validator independently parse image geometry, sample format,
  compression, and attachment locations.

## 4. Proposed design

Add a private `astro-io::xisf::structural` module with:

- a checked monolithic envelope parser for the signature, XML length, reserved
  bytes, source extent, and `16..16+length` XML range;
- a strict UTF-8, namespace-aware XML visitor that emits owned, format-specific
  start/text/end facts without exposing `quick-xml` types or retaining a
  general document AST;
- typed image geometry/sample/byte-order and block-location descriptors with
  checked byte-count and attachment-range operations.

The visitor processes one XML event at a time. The validator remains the owner
of its private node tree and builds it from shared syntax events while retaining
its reservation growth, structure accounting, and cancellation checkpoints.
The loader records the first core `Image` descriptor and rejects unsupported
capabilities explicitly.

Resource model: the loader holds the declared XML header and selected image
descriptor plus its existing payload buffer. The validator retains its header
and working-memory limits; shared parsing does not create a second document
tree. No threads, queues, native resources, or new concurrency are introduced.

## 5. Affected areas

- `astro-io/src/xisf.rs`
- new private files below `astro-io/src/xisf/`
- `astro-io/src/validation/xisf.rs`
- `astro-io/tests/validation.rs`
- focused XISF unit/integration fixture helpers
- this stage handoff/evidence directory

## 6. Execution steps

1. Add failing envelope/XML regression tests covering the 16-byte prefix,
   truncation boundaries, reserved bytes, checked ranges, UTF-8, roots,
   namespaces, malformed XML, and rejection of the 12-byte form.
2. Add failing loader/validator agreement tests using a shared positive fixture
   shape and malformed-prefix cases.
3. Implement the private envelope and XML event primitives.
4. Implement shared image/storage descriptor parsing and checked byte counts.
5. Rebuild the validator's private nodes from shared events; preserve all
   validation-only policy and resource accounting.
6. Replace loader string scanning and 12-byte reads with the shared envelope,
   events, descriptors, and checked attachment extent.
7. Consolidate positive fixture construction around the 16-byte prefix and add
   an explicit legacy 12-byte negative fixture.
8. Refactor after targeted tests are green, update handoff/evidence, refresh
   Graft, and run completion verification.

## 7. Verification strategy

- Targeted structural-module unit tests for pure range and XML behavior.
- Targeted `astro-io` XISF loader tests.
- `cargo test -p astro-io --test validation` for validation behavior and
  resource-policy regression coverage.
- `cargo test -p astro-io xisf` for all focused XISF tests.
- `cargo fmt --all --check`.
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`.
- `cargo test --workspace --all-targets` if the targeted stack passes and the
  runtime remains practical.
- `git diff --check` and a final source/status audit.

This stage changes bounded parsing mechanics but not concurrency or codec
resource behavior, so no new throughput or peak-memory benchmark is required.
Deterministic budget/cancellation coverage in the existing validator suite must
remain green.

## 8. Risks and mitigations

- **Validation weakening:** retain the validation-only tree and compare focused
  validation suites before/after extraction.
- **Budget regression:** invoke validator checkpoints and reservation growth per
  shared event; avoid an intermediate event vector.
- **Namespace drift:** preserve current core/unbound interpretation and report
  foreign namespaces as facts for validator policy rather than accepting new
  Revision 1 extensions in this stage.
- **Loader capability creep:** descriptor parsing may represent more syntax, but
  the compatibility loader keeps explicit subset checks.
- **Error-category drift:** map shared incomplete/invalid/unsupported structural
  errors explicitly into existing validation categories.

## 9. Open questions

None block Stage 1. Public raw metadata handoff, big-endian pixel decoding,
extension acceptance, and codec changes remain later stages.
