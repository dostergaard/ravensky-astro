# AGENTS.md

Repository guidance for coding agents working in the `ravensky-astro` repository.

## Scope and repository role

This file applies to the `ravensky-astro` repository.

Also follow the workspace-root `AGENTS.md`. This file adds shared-library and crate-specific guidance and takes precedence where it is more specific.

RavenSky Astro is a shared Rust library workspace providing reusable functionality for astronomical image I/O, metadata extraction and normalization, image-quality analysis, and related domain infrastructure.

Current crates:

* `astro-io` — low-level FITS/XISF loading and file-format access;
* `astro-metadata` — structured metadata extraction, parsing, normalization, and derived metadata behavior;
* `astro-metrics` — quantitative image analysis and quality metrics;
* `ravensky-astro` — thin umbrella facade over the subcrates;
* `astro-bench` — unpublished opt-in synthetic workload and measurement tooling; application calibration and runtime policy remain consumer concerns.

This repository should not own end-user workflow orchestration, CLI/GUI interaction design, packaging, licensing-tier behavior, or other product-specific policy unless explicitly required.

Keep the repository focused on durable, reusable library concerns.

---

## Repository maturity and priorities

The crates are published but remain in an early formative stage.

Some API shape, naming, module layout, documentation, and crate boundaries reflect early exploratory design. Deliberate cleanup is appropriate when it materially improves the long-term library architecture.

When concerns compete, prioritize:

1. correctness;
2. crate-boundary clarity;
3. API clarity;
4. maintainability;
5. testability;
6. semver stability;
7. documentation quality;
8. performance;
9. implementation elegance.

Do not preserve weak early design merely to avoid change, but do not churn published APIs without a clear architectural or maintainability benefit.

Because the crates are still `0.x` and downstream adoption is limited, intentional breaking corrections may sometimes be preferable to indefinitely preserving a poor abstraction. Treat such changes as explicit design work, not incidental cleanup.

---

## Architectural direction

The current coarse-grained crate split is considered correct unless a task provides a strong reason to change it:

* `astro-io` owns low-level format access, decoding, and image/file loading;
* `astro-metadata` owns structured metadata models, extraction, normalization, precedence, and derived metadata semantics;
* `astro-metrics` owns image-analysis metrics and quality scoring;
* `ravensky-astro` remains a thin facade unless selective composition clearly improves consumer ergonomics;
* `astro-bench` provides measurement infrastructure without imposing application runtime policy.

Prefer improving boundaries within this structure over inventing a new structure casually.

### Known architectural pressure points

Treat the following as known design concerns rather than patterns to reproduce:

* `astro-metadata` currently depends on and re-exports `astro_io::fits::FitsHeaderCard`, indicating an immature ownership boundary;
* XISF support needs careful architectural treatment before further public API expansion;
* the published API surface is already somewhat broad;
* cross-crate contract coverage is thinner than desired;
* no deliberate feature-flag strategy exists yet.

Do not paper over these issues with additional ad hoc coupling.

---

## Crate ownership

Before adding a type, parser, helper, trait, or API, decide which crate conceptually owns it.

### `astro-io`

Owns low-level representation and access to external formats.

Good fits:

* FITS/XISF and related file-format reading;
* image loading;
* raw header or property extraction;
* binary/layout access;
* backend-specific decoding;
* low-level format validation primitives where they belong with structural parsing.

Avoid placing here:

* normalized metadata models;
* high-level metadata semantics;
* product policy;
* image-quality metrics.

### `astro-metadata`

Owns the semantic metadata layer.

Good fits:

* structured metadata models;
* parsing raw metadata into domain values;
* normalization;
* precedence and fallback rules;
* format-independent derived metadata behavior.

Avoid placing here:

* unrelated raw file I/O;
* image-quality analysis;
* application-specific metadata policy.

### `astro-metrics`

Owns reusable quantitative image analysis.

Good fits:

* star, background, and image-quality metrics;
* numerical analysis;
* scoring primitives;
* types directly associated with reusable image analysis.

Avoid placing here:

* raw format loading;
* general metadata normalization;
* application scoring policy that is not intrinsically part of the metric.

### `ravensky-astro`

The umbrella crate should primarily provide:

* re-exports;
* discoverability;
* minimal facade ergonomics;
* selective composition that clearly benefits consumers.

Do not turn it into a miscellaneous convenience layer.

### `astro-bench`

Keep benchmark/workload infrastructure separate from application policy.

It may model representative workloads and expose measurement tools, but product-specific resource admission, configuration defaults, calibration decisions, and runtime scheduling belong to consumers.

---

## Shared-library boundary rule

Reusable astronomy-domain logic belongs here when it represents a stable library concept.

Good fits include:

* astronomy-domain types and calculations;
* metadata models and normalization;
* FITS/XISF parsing and validation primitives;
* reusable image access abstractions;
* reusable metrics and scoring primitives;
* common domain errors and utilities where reuse is genuine.

Do not pull product logic downward merely to eliminate duplication.

A shared abstraction should represent a reusable concept, not an application workflow disguised as a library API.

---

## Public API and semver

Treat every public item as a long-term commitment even while the crates remain `0.x`.

Before adding or widening a public API, ask:

* is this genuinely reusable?
* does the owning crate make sense?
* is the abstraction level correct?
* is the name stable enough to keep?
* does this expose implementation detail?
* can a smaller public surface solve the same problem?
* will this make later semver cleanup harder?

When uncertain, prefer the smaller public surface.

Be especially cautious with:

* public struct fields;
* convenience types that expose internals;
* broad trait abstractions;
* genericity without demonstrated consumers;
* re-exports that accidentally create cross-crate ownership commitments.

Prefer methods, smart constructors, builders, config types, and focused domain types when they preserve future implementation freedom.

### Compatibility

Assume published crates may have downstream consumers.

Do not casually:

* rename or remove public items;
* change type semantics;
* alter error behavior unexpectedly;
* remove accepted inputs;
* tighten parsing or validation without intent;
* change serialization or output contracts.

Breaking changes may be justified when they materially improve the long-term architecture, especially while adoption remains limited.

When intentionally breaking compatibility, update together:

* code;
* tests;
* rustdoc and crate documentation;
* examples;
* changelog or release notes;
* any affected downstream workspace consumers.

Avoid repeated naming churn unless it resolves a real design problem.

---

## Parsing, validation, metadata, and normalization

This repository interprets external file structures, metadata, and scientific values. These behaviors form library contracts.

Preserve:

* deterministic parsing and interpretation;
* explicit precedence and fallback rules;
* predictable handling of malformed or incomplete input;
* clear separation between structural format data and normalized metadata semantics;
* stable failure behavior for unsupported or invalid cases.

Do not silently change normalization, precedence, or derived-value semantics.

When parsing, metadata extraction, normalization, or validation behavior changes, update the corresponding tests and documentation.

### Structural parsing versus semantic interpretation

Prefer sharing syntax, range, decoding, and structural validation primitives rather than creating multiple divergent parsers for the same format.

Do not force validation, metadata extraction, and image loading into a single oversized object model when they have different output or resource needs.

Where practical:

* `astro-io` should own reusable low-level structural parsing and format-access primitives;
* higher layers should consume those primitives for metadata extraction, validation, or image access;
* semantic interpretation should remain outside the lowest-level parser unless it is intrinsic to the file format itself.

Avoid parallel parsing implementations whose behavior can drift independently.

---

## FITS and XISF

Treat FITS and XISF as format backends with explicit contracts, not collections of special cases.

For backend work:

* prefer clear failure over placeholder behavior;
* isolate test-only behavior from production paths;
* do not hardcode test paths in library code;
* avoid stdout/debug printing in normal library operation;
* document unsupported cases;
* avoid widening public APIs around immature internal implementations;
* make meaningful backend differences explicit.

### XISF

XISF remains an active refinement area.

Before expanding XISF-facing APIs, prefer strengthening:

* structural parsing;
* validation coverage;
* metadata extraction consistency;
* error behavior;
* test fixtures and malformed-input coverage;
* shared parser primitives where validation and extraction currently diverge.

Do not preserve an early XISF implementation merely because it already exists if a better internal boundary can be introduced without compromising the public contract.

### FITS

Apply the same architectural principles to FITS.

Avoid allowing mature FITS support to accumulate separate parsing paths for:

* validation;
* metadata extraction;
* image loading;
* structural inspection.

Where these operations need different outputs or resource budgets, share low-level syntax and structural primitives rather than forcing them through one all-purpose representation.

---

## Feature flags

There is currently no established feature-flag strategy.

Do not introduce Cargo features casually.

Features may be appropriate for:

* optional format backends;
* optional heavyweight dependencies;
* staged compatibility transitions;
* capabilities that are genuinely optional for consumers.

A feature design should:

* have a clear consumer-facing purpose;
* keep defaults understandable;
* avoid fragmenting fundamental crate semantics;
* be documented at crate level;
* avoid an unmanageable matrix of combinations.

Prefer an explicit feature architecture over scattered conditional compilation.

---

## Performance and resource contracts

Shared libraries should provide efficient, bounded primitives without embedding application schedulers or whole-machine policy.

For large-file, decode, parsing, or image-processing paths, consider and document where relevant:

* live Rust allocations;
* native/backend allocations;
* fallible allocation behavior;
* resource limits;
* cancellation boundaries;
* codec/backend thread usage;
* serial versus concurrent behavior.

Libraries may expose primitives that allow callers to coordinate resource reservations or concurrency, but should not create:

* product-specific admission schedulers;
* monitor timers;
* hidden global worker pools;
* independent whole-machine memory budgets.

Count codec-internal or backend threads when describing concurrency behavior.

Concurrency or serialization should be based on workload and backend evidence.

Require:

* bounded resource use;
* deterministic results;
* explicit backend limitations;
* serialization across all affected callers when a backend requires it.

Do not serialize unrelated work merely because one backend or operation requires serialization.

Follow the workspace Performance and Resource Design policy for broader resource principles.

Where relevant, measure both:

* throughput and latency;
* peak memory;
* serial and concurrent behavior;
* end-to-end rather than only microbenchmark performance.

Evaluate existing SIMD or hardware acceleration before adding new dependencies, retain portable fallbacks, and preserve identical validation semantics.

---

## Cross-crate contracts

Changes that move concepts or responsibilities between `astro-io`, `astro-metadata`, and `astro-metrics` deserve explicit contract testing.

Particularly valuable boundaries include:

* raw format structures → semantic metadata;
* decoded image data → metrics;
* shared error behavior;
* format-specific extraction → normalized format-independent values;
* facade re-exports → underlying crate APIs.

Prefer tests that make these boundaries visible so internal refactoring does not silently alter semantics.

When a crate-boundary change is intentional, update the relevant contract tests alongside the implementation.

---

## Verification emphasis

Follow the workspace validation and definition-of-done requirements.

For this repository, verification should emphasize reusable library contracts rather than only successful compilation.

When applicable, verify:

* public API behavior;
* parsing and validation semantics;
* malformed and unsupported inputs;
* metadata precedence and normalization;
* backend consistency;
* error contracts;
* cross-crate boundaries;
* serialization/output contracts;
* performance/resource invariants where relevant.

Regression tests are strongly preferred for defects.

For deliberate refactors, use tests to lock in the intended external contract while permitting internal structure to change.

Cross-crate contract tests are especially valuable.

---

## Documentation and architecture

Public APIs should explain the contract that consumers need to depend upon, including:

* purpose;
* inputs and outputs;
* invariants;
* edge cases;
* error behavior;
* examples where useful.

Update documentation when changing:

* public APIs;
* parsing or validation behavior;
* metadata normalization;
* supported formats;
* feature flags;
* crate capabilities;
* ownership boundaries.

Do not allow architecture documents in `docs/` to drift indefinitely from implemented design.

When exploratory decisions settle, consolidate them into authoritative guidance rather than accumulating contradictory planning notes.

Removing stale terminology and clarifying crate-level documentation are worthwhile architectural improvements.

---

## Repository principle

RavenSky Astro should remain a trustworthy shared foundation.

When several designs satisfy the immediate requirement, prefer the one that produces:

* clearer crate ownership;
* a smaller and more durable public API;
* reusable rather than product-specific abstractions;
* predictable parsing and metadata semantics;
* stronger contracts between crates;
* easier downstream adoption and maintenance.

---

<!-- graft:start -->

## Graft — repo context graph

This repo is indexed in `graft/`: small linked markdown nodes that explain each
system and carry exact file:line spans, kept in sync with the code through git.

For ANY task here — understanding how something works, finding where code lives,
or scoping a change — get context from the graph before grepping or opening
source files. Re-ask freely (it's cheap) and reuse literal identifiers you
already have (symbol, error string, file name) as the query. New to this repo?
Run `graft map` first — a token-budgeted orientation (dir clusters, hubs,
hotspots), no LLM, no key.

* Run `graft ask "<your question>" --source` → ranked nodes with the relevant
  code spans inlined (each hit's ≤8-line crux by default; `--full` for whole
  definitions when the crux isn't enough). Match the tool to the task shape:
  for understanding or editing, the top node IS the answer — cite its
  `covers:` file:line spans and edit straight from `--source`. For
  exhaustive tasks ("every occurrence / every caller of this pattern"), ranked
  results are top-N, not complete — run `graft grep "<literal>"` instead
  (exhaustive over indexed files, grouped by enclosing symbol), falling back
  to raw `grep -rn` only for unindexed files.
* `graft skeleton <file>` → every definition's signature + span, ~10× cheaper
  than reading the file; use it to skim an API surface.
* `graft callers <symbol>` gives precomputed, exact edges — who calls this.
  Add `--direction out` for what it calls, or `--depth N` to walk
  transitively for the full blast radius. For structural questions, skip
  ranking and use this directly.
* Or browse: `graft/INDEX.md` lists every node; follow the links.
* Monorepos and folders of multiple repos rank fairly across sub-projects —
  hits carry `[scope/]` labels naming which one they're from. Narrow with
  `graft ask "<task>" --in <scope>/` once you know where you're working.

If a returned span is truncated ("+N more lines"), open the file at that exact
range before finalizing. Only open source files when a node genuinely lacks a
needed detail, and then at the exact file:line the node points to — never
re-read whole files.

After big code changes, refresh the graph with `graft build` (deterministic,
no API key, $0).

<!-- graft:end -->
