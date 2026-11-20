# P7-A candidate CI handoff

## Current publication checkpoint, 2026-11-20

The user separately authorized **only `astro-io 0.7.0`**. It was published
from qualified commit `5e25bd7a20781aafdc5f94dc22f4f1a343913a0c`
after a clean branch/remote check, exact archive SHA-256 comparison and
successful Rust 1.94 locked publish dry-run. The crates.io registry and the
downloaded archive both report SHA-256
`347071fb00b76c080a647053ce3397908c54cf87a6afa6752510015a924eb9c7`.
The archive is byte-identical to the approved local package. The official
[docs.rs build 5405037](https://docs.rs/crate/astro-io/0.7.0/builds/5405037)
succeeded, and the versioned rustdoc page is available.

The other three 0.7.0 crates remain unpublished and require separate explicit
authorization in dependency order. Before proposing each, requalify its
exact archive and commit; this evidence-only documentation change alters
Cargo's VCS metadata and therefore downstream archive hashes. Do not merge,
tag, publish binaries, or change AstroMuninn. The historical notes below
describe earlier checkpoints and are superseded by this section where their
publication status differs.

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
build steps passed. The initial correction added a test-only mutex because
bundled CFITSIO has process-global HCOMPRESS bitstream counters. The complete
source review below found those counters are already locked within CFITSIO
on reentrant builds, and replaced the temporary test mutex with explicit
CFITSIO initialization before direct codec tests. EVIDENCE.md records both
the failure and corrected source analysis.

That test-only correction passed the complete existing CI workflow in
[run 44610247661](https://github.com/dostergaard/ravensky-astro/actions/runs/44610247661).
The earlier 6267d11 green run and the 1590d7e failure remain durable
diagnostic evidence; see EVIDENCE.md for the full sequence.

Stop before real `cargo publish`, merge, tag, GitHub Release, AstroMuninn
changes, or product distribution. After separate publication authorization,
repeat each downstream registry dry-run as its 0.7.0 prerequisite becomes
available, upload in dependency order, then verify registry archives/docs.rs
and an independent registry-only consumer. P7-B and P7-C remain planned but
unexecuted; see PLAN.md.

## Qualified candidate and remaining review

The test-only correction commit
`d091319580004840ef2b28bfd15f04d3053c0096` passed all four jobs in
[CI run 44610247661](https://github.com/dostergaard/ravensky-astro/actions/runs/44610247661):
retired naming, Linux x86-64 GNU, macOS ARM64 and Windows x86-64 GNU. Rust
1.94 formatting, strict Clippy, optimized builds, tests, doctests and strict
rustdoc passed on each configured platform. Linux four-crate packaging
passed; the other platforms correctly skipped that Linux-only step.

Fresh verified 0.7.0 archives at that commit have SHA-256 values, in
dependency/publication order:

- `astro-io`: `d77948cc89c80d9c81a692ebfc6791b3dca34ae85831630bc2408211264a1b80`
- `astro-metadata`: `91a65e7a402f56ce0d49cdc34206ec09c7945072bcef28a51ea2f545ce4495e9`
- `astro-metrics`: `4485817bfdbd65fd2a29e0cf1c797c494aae6caf47a51548d2a3890c3a6084c9`
- `ravensky-astro`: `ffd8ebca2698f04b8a26a324f5baffdd6a36f2d784552590846376fd7494b3d5`

The later source review found bundled CFITSIO's `FFLOCK`/`FFUNLOCK` already
protects HCOMPRESS's process-global bitstream state in reentrant builds.
FITS open initializes that lock before high-level pixel reads; the direct
oracle tests had bypassed that path. No production lock or public API change
is needed. The direct tests now initialize CFITSIO explicitly and include a
four-worker native stress test; a cross-crate test covers exact same-file
and different-file high-level reads, metadata and full validation. The
earlier Windows failure's exact trigger remains an inference pending the
new cross-platform candidate run. Calls outside RavenSky's gate and other
CFITSIO versions remain separate integration boundaries.

The subsequent documentation commit
`b16bbc268c954516d18ecb544482305f408be65e` passed all four jobs in
[CI run 44611228993](https://github.com/dostergaard/ravensky-astro/actions/runs/44611228993).
Its archive SHA-256 values, in publication order, are
`fb91d22f76bbc3d162e7c334a71f712776054435d6caec522ad3fae52b562b69`,
`02f775b82c8d8adcba4caec9e05c3a2ff451402e41f1f940c8366cea5d5387cc`,
`b57719830613351095dddbfc1b7f1cc2cf1eb342c380a02115751110c6c53e30`,
and `d19cb8bb140c2e28e1902e9e5cfa09890e800380a7ca545011c154dae38cbe86`.
These are historical once the current source review is committed; regenerate
all four archives and rerun CI at the exact new candidate commit.

Local Rust 1.94.0 formatting, strict Clippy, complete all-target workspace
tests, doctests, strict rustdoc and optimized example builds passed on
macOS ARM64 with bundled `fitsio/src-cmake`. Twenty repeated runs of each
focused concurrency test group also passed. No production API or native
codec code changed. The exact final review commit still needs clean package
verification, normal branch push, complete candidate CI and archive hashes
before requesting publication authorization.
