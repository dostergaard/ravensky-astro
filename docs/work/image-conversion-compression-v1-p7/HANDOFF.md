# P7-A candidate CI handoff

## Goal and current state

P7-A has four proposed 0.7.0 crates: `astro-io`, `astro-metadata`,
`astro-metrics`, and `ravensky-astro`, in that publication order.
`astro-bench` remains unpublished. The isolated branch is
`release/ravensky-astro-p7-preparation`; the original `master` checkout and
its unrelated `.gitignore` edit are preserved. AstroMuninn remains untouched.
The user authorized pushing this candidate for CI, not publishing crates,
merging, tagging, or releasing applications.

The original candidate commit was
`1c73ceabb66c8720fb38095a32eaae8bb454b9e4`.
[CI run 44607491633](https://github.com/dostergaard/ravensky-astro/actions/runs/44607491633)
was incomplete: Linux and naming passed, Windows Clippy failed in the
unpublished benchmark example, and GitHub cancelled the unsupported
`macos-14` ARM64 job before it ran. Commit
`6267d1121d44714c984a0961c43e6fd93e7c295b` fixed the example's
Windows type inference and selected GitHub's supported `macos-15` ARM64
runner while preserving the full target matrix.
[CI run 44608168246](https://github.com/dostergaard/ravensky-astro/actions/runs/44608168246)
passed all four jobs. Linux/macOS each passed 215 tests plus 4 doctests with
3 established ignores; Windows GNU's configured subset passed 187 tests
plus 3 doctests with 2 ignores. Strict Clippy, formatting, optimized builds
and strict rustdoc passed on each configured platform; Linux package
verification passed for all four public crates. The Windows MSYS2 action
issued a deprecation warning, and GitHub noted macOS ARM64 queue capacity.
See EVIDENCE.md for exact job results and limitations.

The four clean 6267d11 package archives verified successfully. Their SHA-256
values are, in publication order:

- `astro-io`: `6e4aaaa66745342f4cf38b7328fb91d3eb6c13bef352a8d2291a1bbb2da49d84`
- `astro-metadata`: `42385ee2eae9e65e9ba52af2531a75e8713ac2e03b954639764bbc66843e73ca`
- `astro-metrics`: `678d2e3b62c988ec95b6fe2ed9e2389c6489fb1ecd402c314f7f704bb9d6082d`
- `ravensky-astro`: `182275c21f35c70adea897555df13e9f681c29ba82d3f20118910ad2ddb93088`

The public crates.io owner API lists `dostergaard` on all four crates. A
Cargo default-registry token entry is configured locally, but its identity
and upload rights were not proven; no secret value was displayed. A real
upload has not been attempted.

## Later candidate run and final checkpoint procedure

The evidence-only commit `1590d7ea577be5f097ef66a5b14ac1c82551e2c8`
received [CI run 44609029486](https://github.com/dostergaard/ravensky-astro/actions/runs/44609029486).
Linux, macOS and naming passed. Windows GNU failed two intermittently
corrupted direct native HCOMPRESS oracle tests after its Clippy and optimized
build steps passed. Bundled CFITSIO has process-global HCOMPRESS bitstream
counters; a test-only mutex now serializes those two direct native fixtures
without narrowing the CI matrix. Focused local parallel execution and strict
Clippy passed. EVIDENCE.md contains the failure and source analysis.

Qualify the next correction commit with the complete existing CI workflow.
Regenerate all four archives from that final clean commit in a fresh target
directory and report their hashes. Reconfirm branch/master refs, worktree
cleanliness and public registry versions. Exact final-commit CI and archive
results belong in the user approval checkpoint; the earlier 6267d11 green
run and the 1590d7e failure remain durable diagnostic evidence.

Stop before real `cargo publish`, merge, tag, GitHub Release, AstroMuninn
changes, or product distribution. After separate publication authorization,
repeat each downstream registry dry-run as its 0.7.0 prerequisite becomes
available, upload in dependency order, then verify registry archives/docs.rs
and an independent registry-only consumer. P7-B and P7-C remain planned but
unexecuted; see PLAN.md.
