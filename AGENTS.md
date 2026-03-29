# AGENTS.md

Repository guidance for coding agents working in the ravensky-astro repository.

## Scope

This file applies to the ravensky-astro repository.

Also follow the workspace-root `AGENTS.md`. This repository file adds library- and crate-specific guidance and takes precedence where it is more specific.

---

## Repository role

RavenSky Astro is a shared-library workspace.

It provides reusable Rust crates for astronomical image I/O, metadata extraction and normalization, image-quality metrics, and a thin umbrella facade crate.

Current workspace crates:

- `astro-io`: low-level FITS/XISF loading and related file-format access helpers
- `astro-metadata`: structured metadata extraction, parsing, and normalization
- `astro-metrics`: quantitative image-analysis and quality metrics
- `ravensky-astro`: umbrella crate and facade over the subcrates

This repository is not the place for end-user workflow orchestration, GUI behavior, CLI interaction design, packaging logic, or product-tier behavior unless explicitly required.

Prefer keeping this repository focused on durable, reusable library concerns.

---

## Repository maturity note

This repository is published but still in an early formative stage.

Some existing crate structure, API shape, naming, module layout, and documentation reflect early design decisions made during initial exploration of Rust and the crate ecosystem. The current architecture direction is sound, but boundary cleanup and API tightening are still appropriate.

Agents may recommend or perform deliberate refactoring when it materially improves:

- crate boundaries
- API clarity
- naming consistency
- documentation quality
- testability
- maintainability
- long-term semver stability

Prefer making these improvements now, while downstream adoption is still limited, rather than freezing immature structure indefinitely.

Even so, refactoring must remain intentional, justified, and well-scoped. Do not churn the codebase without a clear architectural or maintainability benefit.

---

## Repository priorities

When making decisions, prioritize in this order:

1. correctness
2. crate-boundary clarity
3. API clarity
4. maintainability
5. testability
6. semver stability
7. documentation quality
8. performance
9. implementation elegance

Do not trade away clarity or long-term crate quality for clever abstractions or short-term convenience.

---

## Current architectural direction

The current coarse-grained split is considered correct unless the task explicitly justifies changing it:

- `astro-io` should own low-level format access and image/file loading concerns
- `astro-metadata` should own structured metadata models, extraction, and normalization
- `astro-metrics` should own image-analysis metrics and quality scoring
- `ravensky-astro` should remain a thin facade unless there is a clear reason to make it a more opinionated composition layer

Prefer tightening boundaries inside this structure over inventing a new structure casually.

---

## Known architectural pressure points

The repository currently has several known weak spots that should be handled deliberately:

- `astro-metadata` currently depends on and re-exports `astro_io::fits::FitsHeaderCard`, which suggests an immature crate boundary
- XISF support in `astro-io` should be treated as an area needing extra care before further API expansion
- some naming and documentation still drift from older "Astro Core" terminology
- the public API surface is already published and somewhat broad
- cross-crate contract testing is thinner than ideal
- there is currently no feature-flag strategy even though future backend or capability splits may need one

Do not paper over these issues with ad hoc additions. Prefer explicit design corrections.

---

## Shared-library boundary rules

Prefer putting genuinely reusable domain logic in this repository.

Good fits include:

- astronomy-domain types and calculations
- metadata models and normalization
- FITS/XISF or related data-access abstractions
- reusable parsing and validation logic
- metrics, analysis, and scoring primitives
- common error and utility types where reuse is real

Avoid adding:

- app-specific workflow assumptions
- CLI presentation behavior
- GUI state or interaction logic
- product-tier distinctions
- packaging or installer behavior
- policy decisions that belong to consuming applications

Do not pull product logic downward into shared crates merely to reduce duplication unless the abstraction is genuinely reusable and improves the design.

---

## Crate-boundary guidance

Before adding new logic, decide carefully which crate it belongs in.

### `astro-io`

Best for:

- file-format reading
- image loading
- raw format access
- low-level header-card extraction
- backend-specific decode behavior

Avoid placing here:

- normalized metadata models
- high-level metadata semantics
- product-facing interpretation rules
- analysis or scoring logic

### `astro-metadata`

Best for:

- structured metadata models
- metadata parsing
- normalization
- precedence and fallback rules
- consistent derived metadata behavior across formats

Avoid placing here:

- raw file I/O concerns that do not belong to metadata interpretation
- image-analysis metrics
- UI- or product-specific metadata policy

### `astro-metrics`

Best for:

- star/background/quality metrics
- numerical analysis
- scoring logic
- analysis-oriented types tied to image quality

Avoid placing here:

- raw format loading
- metadata normalization policy unless strictly needed for metrics inputs

### `ravensky-astro`

Best for:

- re-exports
- minimal facade ergonomics
- selective composition only when it clearly improves consumer experience

Do not casually turn the umbrella crate into a grab bag.

---

## Public API guidance

Be conservative with public API changes.

For public items:

- prefer clear, unsurprising names
- encode meaning in types where practical
- document invariants and edge cases
- avoid exposing implementation details
- prefer small, composable interfaces
- avoid over-generalizing too early

Before adding a public type, trait, function, module, or field, ask:

- is this truly reusable?
- is this the right crate for it?
- is the name stable enough to live with?
- does this expose the correct abstraction level?
- does this make future semver cleanup harder?
- can a smaller surface achieve the same outcome?

If the answer is uncertain, prefer the smaller public surface.

Public field exposure should be treated cautiously. Prefer methods, builders, smart constructors, or focused config/data types where they improve long-term API durability.

---

## Semver and compatibility

Assume that published crates may have downstream consumers.

Do not casually:

- rename public items
- change type semantics
- alter error behavior in surprising ways
- remove variants, fields, or supported inputs
- tighten parsing or validation rules without reason
- change output formats or serialization behavior without explicit intent

Because the crates are still `0.x` and downstream adoption appears limited, deliberate breaking refactors may be acceptable when they clearly improve long-term crate quality.

Treat these as intentional design corrections, not incidental cleanup. When making such changes, update:

- documentation
- tests
- examples
- changelog or release notes
- crate-level guidance where applicable

Avoid repeated naming churn unless it fixes a real architectural problem.

---

## Type and trait design

Prefer strong domain types over loosely structured primitives.

Prefer:

- enums over ambiguous flags
- newtypes where they clarify meaning
- builders or config structs over long parameter lists
- trait implementations only when they reflect clear semantic meaning
- composition over elaborate trait hierarchies

Avoid:

- public APIs that depend on booleans with unclear meaning
- trait machinery that is harder to understand than the concrete design
- exposing internal convenience abstractions as public contracts
- genericity that is not justified by real consumer needs
- field-heavy public structs when a narrower API would better preserve future flexibility

---

## Error handling

Use explicit, meaningful error handling.

Prefer:

- domain-appropriate error types
- context-preserving propagation
- actionable messages where errors cross crate boundaries
- distinguishing invalid input, unsupported cases, and internal failures where useful
- failing clearly instead of returning placeholder data for unsupported or broken cases

Avoid:

- `unwrap()` and `expect()` in production paths
- vague catch-all error messages
- silently discarding malformed or unexpected data unless the contract explicitly allows it
- debug printing or stdout output in published library code except where explicitly intended

When parsing or normalizing data, make failure behavior explicit and predictable.

---

## Parsing, metadata, and normalization behavior

Many crates in this repository interpret external data, metadata, file structures, or scientific values.

For this kind of logic:

- preserve determinism
- document precedence and fallback rules
- make normalization behavior explicit
- handle malformed or incomplete inputs predictably
- avoid silent semantic drift

If parsing, metadata extraction, normalization, or derived-value logic changes, update tests and documentation accordingly.

Be especially careful when changing cross-crate boundaries between raw extracted data and normalized metadata semantics.

---

## XISF and format-backend guidance

XISF support should be treated as an active refinement area.

When working on XISF or backend-specific code:

- prefer clear failure over placeholder behavior
- remove or avoid hardcoded test-path logic in library code
- avoid stdout printing in normal library operation
- keep test-only behavior isolated from production paths
- document unsupported cases explicitly
- avoid expanding public API around immature internal behavior until the implementation is sound

If backend behavior differs materially across formats, make the contract explicit.

---

## Dependency discipline

Do not add dependencies casually.

Before adding a crate:

- check whether the standard library already suffices
- check whether the dependency is already present in the workspace
- prefer mature, well-maintained crates
- consider compile time, binary size, transitive dependency cost, and maintenance burden
- avoid large dependencies for small convenience gains

Prefer keeping foundational crates especially lightweight and stable.

Unused dependencies should be removed when identified.

---

## Feature-flag guidance

This workspace currently has no feature-flag strategy.

Do not introduce features casually. But if a task requires optional backends, staged migrations, or optional capabilities, prefer a deliberate and documented feature design over ad hoc conditional compilation.

A good feature design should:

- have a clear user-facing purpose
- avoid fragmenting core crate semantics unnecessarily
- keep default behavior understandable
- be documented at the crate level
- avoid creating hard-to-test combinations without reason

---

## Performance guidance

Performance matters when processing large files, metadata sets, or image-related data, but do not sacrifice clarity without a good reason.

Optimize deliberately.

Good reasons to optimize include:

- repeated allocations in hot paths
- unnecessary cloning of large structures
- avoidable repeated parsing or normalization work
- accidental quadratic behavior
- poor scaling across large datasets

Prefer measured or well-reasoned improvements over speculative tuning.

---

## Concurrency guidance

Use concurrency only when it materially improves design or performance.

Do not introduce async, threads, channels, locks, or parallel processing casually.

When concurrency is appropriate:

- keep ownership and synchronization easy to reason about
- document assumptions and invariants
- preserve deterministic observable behavior where practical
- avoid making APIs harder to use unless the gain is clear

---

## Testing expectations

When changing behavior in this repository, validate with the narrowest useful tests first, then broader checks as appropriate.

Expected validation usually includes:

- `cargo fmt`
- `cargo clippy --all-targets --all-features`
- relevant unit tests
- relevant integration tests
- doc tests where applicable

Add or update tests when changing:

- public behavior
- parsing or normalization logic
- metadata extraction
- error behavior
- serialization or output contracts
- backend-specific behavior
- performance-sensitive logic when regressions are plausible
- crate-boundary behavior between `astro-io`, `astro-metadata`, and `astro-metrics`

Prefer tests that verify behavior and contracts, not implementation trivia.

Regression tests are strongly preferred for bug fixes.

For deliberate refactors, add or update tests that lock in the intended crate contracts and prevent regressions while structure changes.

Cross-crate contract tests are especially valuable in this workspace.

---

## Documentation expectations

Public crates and public APIs should be documented clearly.

Prefer rustdoc that explains:

- purpose
- expected inputs and outputs
- invariants
- edge cases
- error behavior
- examples where useful

Update documentation when changing:

- public APIs
- parsing behavior
- normalization rules
- supported formats
- feature flags
- crate capabilities or intended usage
- architectural boundaries between crates

Do not leave stale examples or outdated crate-level guidance behind.

Improving rustdoc coverage, crate-level documentation, examples, and usage guidance is a worthwhile form of foundational refactoring in this repository.

The architecture docs in `docs/` should not drift indefinitely from the actual code. When design decisions settle, prefer converging planning notes into clearer authoritative guidance.

---

## Refactoring guidance

Deliberate refactoring is encouraged in this repository when it improves the long-term quality of the shared crates, especially while downstream adoption remains low.

Good refactors in this repository include:

- clarifying crate boundaries and responsibilities
- moving types or concepts into better ownership boundaries
- improving module organization
- simplifying or reshaping immature public APIs
- strengthening type design
- isolating parsing or normalization logic
- improving rustdoc coverage and examples
- improving testability and test structure
- adding cross-crate contract tests
- reducing real duplication
- separating stable core concepts from exploratory or volatile code
- renaming poorly chosen items for long-term clarity before adoption hardens
- removing stale terminology and docs drift

When refactoring public APIs, documentation, examples, and tests should be updated together.

Avoid:

- aesthetic churn without architectural benefit
- unnecessary abstraction
- broad rewrites without a clear target state
- mixing unrelated cleanup into feature work unless it is required
- preserving weak early design choices solely to avoid change

Prefer intentional, documented improvements that move the repository toward a cleaner and more durable crate ecosystem.

---

## Things agents should not do casually

Do not casually:

- introduce product-specific assumptions into shared crates
- widen the public API without a clear reason
- make breaking changes to published crate behavior
- change parsing or normalization semantics silently
- add heavyweight dependencies for convenience
- expose internal helper types as public API
- introduce concurrency complexity without evidence it is worthwhile
- optimize hot paths without identifying an actual problem
- leave prototype behavior in published library paths

These changes have long-term maintenance cost.

---

## Preferred agent workflow in this repository

When beginning work:

- identify which crate or crates are actually in scope
- inspect existing public APIs and patterns before adding new ones
- check whether the change belongs in shared library code or in a consuming application
- preserve compatibility unless the task explicitly requires otherwise
- use an explicit implementation strategy for non-trivial refactors, API reshaping, crate-boundary changes, backend cleanup, or documentation uplift efforts

When finishing work:

- summarize crate-level and API-level effects
- note any compatibility implications
- report validation performed
- mention downstream consumer impacts when relevant

---

## Recommended skills

When available in this workspace, prefer using skills for repeatable library-engineering workflows.

Recommended examples:

- `grill-me` for clarifying abstraction boundaries, consumer needs, and API direction before implementation
- `impl` for non-trivial library design or refactoring plans
- `tdd` for parsing, metadata, normalization, error handling, backend cleanup, and regression fixes
- `verify` before declaring work complete
- `api-audit` when reviewing a crate, module, or library change

Use these skills to reduce hidden assumptions and improve confidence in shared-library changes.

---

## Repository-local principle

RavenSky Astro should remain a trustworthy shared foundation.

When uncertain, choose the design that is clearer, more reusable, more stable, and easier for downstream consumers to understand and depend on.
