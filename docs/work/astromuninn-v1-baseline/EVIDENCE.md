# RavenSky Astro integration evidence

## Starting audit, 2026-11-19

- Original checkout: `/Users/dean/projects-dev/RavenSky/ravensky-astro`,
  `master` at `11fb02c59938cc3d736dc03bedefe58ee44b4db7`, with one
  unrelated unstaged `.gitignore` edit. No user file was moved or staged.
- Remote `origin/master`: the same commit, confirmed by `git ls-remote` and
  `git fetch origin master`; no independent remote advance was observed.
- Clean P5: `59c6de1d5f53b182b1c1033acd423d53d7fc5796`. Its history
  contains P1 `406ff618`, P2/P3 `ef4688ba`, and P4 `5205f133`.
  `git merge-base master P5` returned master, with 10 commits only on P5.
- The integration worktree was created from `origin/master` at
  `/Users/dean/projects-dev/RavenSky/.worktrees/ravensky-astro-v1-baseline`.
  `git merge --ff-only feature/image-conversion-compression-p5` advanced it
  to P5 with no conflict, replay or source edit.
- Published 0.6.2 corresponds to tag `v0.6.2` at
  `0d8c57a053cf922cbe8075fc77e82b69192f9aeb`; the P5 branch adds typed
  I/O, fidelity and the compressed-XISF capability correction after that tag.
  Thus registry 0.6.2 cannot supply the exact P5 implementation.

## Verification

On macOS arm64 (Darwin 27.2.0) with Rust/Cargo 1.100.0:

- `cargo test --offline --workspace --all-features`: **197 passed, 3 ignored,
  0 failed** across 21 test/doc-test harnesses. This includes `astro-io`
  typed FITS/XISF and validation tests, `astro-metadata` fidelity tests,
  and benchmark-crate tests; the three ignored cases remain optional.
- `cargo clippy --offline --workspace --all-targets --all-features -- -D
  warnings`: passed on rerun. Rust 1.100 first flagged three pre-existing
  `chunks_exact_to_as_chunks` findings in `astro-bench` tests and four
  `unnecessary_sort_by` findings in example helpers. The integration makes
  equivalent test/example-only iterator and sort changes; the full suite was
  rerun after those edits. No shared production API or algorithm changed.
- `cargo fmt --all --check`, `git diff --check`, and
  `cargo build --offline --release --workspace --all-features`: passed. The
  optimized build included all public workspace crates and `astro-bench`.
- Cargo 1.100 emitted existing manifest warnings for unused workspace
  documentation, redundant homepage fields and unused declared dependencies.
  No Rust source warning survived strict Clippy.

The initial sandboxed test build stopped inside `fitsio-sys` autotools with
`Operation not permitted`; the same unchanged build and tests passed with
native build access. Tests use repository fixtures and disposable directories;
no astronomical archive, application configuration or job journal was
modified. Remote reachability is recorded after the integration commit is
pushed.
