# RavenSky Astro integration for AstroMuninn v1.0.0

## Objective

Provide the completed P1–P5 `astro-io` and `astro-metadata` implementation at
one remotely reachable, immutable Git revision for AstroMuninn's P7 baseline.

## Constraints and current state

On 2026-11-19, `master` and `origin/master` were both
`11fb02c59938cc3d736dc03bedefe58ee44b4db7`. The clean P5 branch was
`59c6de1d5f53b182b1c1033acd423d53d7fc5796`; `master` is its ancestor,
and P5 contains P1–P4. The original checked-out `master` has an unrelated
unstaged `.gitignore` edit that must remain untouched. Public crates 0.6.2
were published before the P1–P5 conversion work; no new crate publication or
version change is authorized in this task.

## Design and affected areas

Fast-forward a clean `integration/astromuninn-v1.0.0-baseline` worktree from
the current committed master to P5, retaining its full history. Add only this
integration record, run the shared-crate checks, and push the branch to the
configured origin. AstroMuninn will pin that exact resulting commit for
`astro-io` and `astro-metadata`. Production Rust APIs and algorithms are not
changed by this integration. Shared version 0.6.2 remains the source package
version; a future registry release must use a coordinated new version.

## Execution and verification

1. Check remote master, ancestry, worktrees and dirty files; fast-forward the
   integration branch with no replay or squash.
2. Run workspace tests with all features, focused typed/fidelity tests, Rust
   formatting, strict all-target/all-feature Clippy and optimized crate builds.
3. Record results and limitations; commit the integration documentation.
4. Push the integration branch only after checking remote movement, then
   verify the exact commit through `git ls-remote`.

## Risks and open questions

The original `master` checkout's unrelated edit prevents an automatic master
update under the preservation rule; report it for the owner after the tested
branch is ready. The shared source must be pushed before AstroMuninn can claim
a reproducible Git dependency. This task does not tag or publish any crate.
