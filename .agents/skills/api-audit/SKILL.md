---
name: api-audit
description: Use when reviewing a crate, module, or library change for public API quality, crate-boundary correctness, semver risk, and documentation readiness. Focus on whether the current public surface is durable, minimal, well-owned, and appropriate for a published library.
---

# Purpose

This skill performs a focused audit of a library's public surface.

Use this skill when:
- reviewing a published or publishable crate
- evaluating public API quality before adding features
- considering breaking cleanup while adoption is still low
- checking whether items are public that should not be
- reviewing crate-boundary ownership of public types
- preparing for release or documentation cleanup
- planning an architectural refactor that affects public contracts

Do not use this skill for:
- purely internal implementation changes with no public surface implications
- generic bug fixing unless the bug is caused by API design
- product UX or workflow review

# Audit Goals

The goal is to determine whether the public API is:

- minimal
- clear
- reusable
- well-owned by the correct crate
- semver-responsible
- documented enough for real consumers
- consistent with the repository's architectural intent

# Review Areas

Work through the following areas as relevant.

## 1. Public Surface Inventory
Identify the public surface being exposed:
- public modules
- public structs, enums, traits, functions, constants, and type aliases
- public fields
- re-exports
- facade exports from umbrella crates

Ask:
- what is public today?
- what exists only for convenience?
- what appears accidentally public?

## 2. Surface Area Discipline
Check whether the public API is larger than necessary.

Ask:
- can this item be private or crate-visible instead?
- should this public field be hidden behind methods or builders?
- is this helper really part of the external contract?
- is the crate exposing implementation details it may want to change later?

## 3. Crate Ownership
Check whether public items live in the correct crate.

Ask:
- does this type belong here conceptually?
- is one crate leaking raw types into another crate's public model?
- is the public surface aligned with the intended responsibilities of each crate?
- are re-exports helping consumers or hiding boundary problems?

## 4. API Shape and Durability
Review the design quality of public items.

Ask:
- are names clear and stable?
- do types encode meaning clearly?
- are booleans or loosely typed parameters hiding intent?
- are there overly broad or premature abstractions?
- is the API likely to become a semver burden?

## 5. Error and Contract Semantics
Check whether public behavior is explicit.

Ask:
- are failure modes clear?
- are unsupported cases explicit?
- does the API fail clearly or rely on placeholder behavior?
- are invariants and edge cases documented well enough?

## 6. Documentation Quality
Review rustdoc and crate-level guidance.

Ask:
- does the public item explain purpose, inputs, outputs, invariants, and errors?
- are examples present where helpful?
- is crate-level documentation aligned with actual usage?
- do docs still reflect stale names or old architecture?

## 7. Refactor Opportunity
Identify what should be fixed now versus later.

Classify findings into:
- safe to leave as-is
- should be improved soon
- worth a deliberate breaking cleanup now
- needs architectural planning before change

# Output Structure

Use this structure:

## Scope Audited
State which crate, module, or public surface was reviewed.

## Public Surface Summary
Summarize the main public items or API areas reviewed.

## Findings
Group findings under:
- Surface area
- Crate ownership
- API shape
- Error/contract semantics
- Documentation
- Semver risk

For each finding, state:
- what the issue is
- why it matters
- how urgent it is

## Recommendations
Separate recommendations into:
- non-breaking improvements
- breaking-but-worthwhile improvements
- items to leave alone for now

## Next Step
Recommend one of:
- implement directly
- write an implementation strategy
- add tests first
- defer until a broader refactor

# Severity Guidance

Use practical severity labels:
- low
- medium
- high

Reserve high severity for issues such as:
- wrong crate ownership of important public types
- public API that locks in immature design
- placeholder or misleading published behavior
- semver traps that will worsen if left alone

# Operating Rules

- Be conservative about recommending breaking changes unless the long-term gain is clear.
- Be honest when an item is awkward but tolerable.
- Prefer smaller public surfaces.
- Prefer documented, well-owned contracts over convenience re-exports.
- Do not confuse personal taste with architectural risk.
- Do not recommend churn without a clear payoff.

# Anti-Patterns

Do not:
- produce a generic style review
- nitpick formatting or local code style
- recommend hiding everything by default without regard to usability
- call for large rewrites without identifying the actual API risk
- treat every public field as automatically wrong
