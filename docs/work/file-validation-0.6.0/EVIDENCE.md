# Validation and benchmark evidence

Status: historical evidence audited 2026-09-09; release closeout checks and actual
publication results are recorded below as completed. [PLAN.md](PLAN.md) is the
settled contract; [HANDOFF.md](HANDOFF.md) owns the operational handoff.

## Provenance and interpretation

Raw evidence is retained in `docs/benchmarks/`, with source/compiler/build/script
fingerprints and input identities. Earlier reports apply to their recorded code
and conditions; later codec support must not be retroactively attributed to them.
The [closeout audit](records/retained-evidence-audit.json) recomputed current codec
counts, maximum RSS, speedups, cancellation timings and all 14 supplied fixture
hashes from retained artifacts. Tests are correctness evidence; timings are
separate observations. **No timing, worker count or RSS measurement is a universal
default, guarantee, hardware recommendation or OS-enforced memory limit.**

## Correctness coverage

| Area | Evidence and observations |
| --- | --- |
| FITS/XISF structure | Generated fixtures in `astro-io/tests/validation.rs` cover layouts, HDUs/local blocks, bounds, truncation, limits, unknown features and changes during validation. |
| Full payloads | Checksum corruption, stored-data checks before decompression, exact codec extents/output, supported XISF zlib/LZ4/LZ4HC/Zstandard/shuffle and digest variants. |
| Compressed FITS | `validation/compressed.rs` covers native-generated integer/float/quantized/fallback tiles, masks, malformed descriptors, truncations, mutations, bounded random HCOMPRESS input and shared contention/cancellation. |
| Algorithm parity | Rice/HCOMPRESS unit tests compare decoded samples against native-generated/oracle values, including signed/wrapping values, odd geometry and lossy HCOMPRESS. PLIO native masks and hand-built instructions verify its representation contract. |
| Resource lifetime | `astro-io/tests/resources.rs`, validator unit/integration tests and benchmark workloads check shared accounting, limits/busy distinctions, errors, cancellation and cleanup. |
| Native consumers | `astro-metadata/tests/fits_concurrency.rs` exercises loader/header/metadata and validation callers. Native reentrancy gate tests cover serial/reentrant/nested/unwind behavior. |
| Supplied fixtures | 14 binary-preserved files in `astro-bench/tests/test_data`; checksum_false is rejected at Full; two incomplete XISF thumbnail fixtures remain incomplete; the supplied Rice files now pass managed Full validation. Random groups remain unsupported. |

Unknown checksums/codecs in Full are not counted as successful validation. Source
preservation is tested both by validator observations and by independent before/
after benchmark hashes. No invalid capture was repaired to manufacture a pass.

## Earlier baseline and completion investigation

See [initial baseline](../../benchmarks/2026-09-06-m4-max/README.md),
[streaming comparison](../../benchmarks/2026-09-06-m4-max-streaming/README.md),
[completion record](../../benchmarks/2026-09-07-m4-max-completion/README.md) and its
[machine-readable audit](../../benchmarks/2026-09-07-m4-max-completion/audit.json).
The initial and streaming checkpoints each retain 216-sample comparisons. The
later completion audit records:

- 960 scaling samples, eight UInt16 inputs and worker/tile/size/order variants;
  maximum unprofiled RSS 9.53125 MiB (reported conservatively as below 9.54 MiB).
- 40 bounded-contention samples with a short-task/scheduling proxy, not a human
  application-switching assessment. Two competitors each hash a touched 32 MiB
  payload; this is not induced system RAM pressure.
- 10 CLI cancellation trials with child reaping and scratch cleanup; maximum
  38.842 ms, below the predeclared two-second local target.
- 270 real-capture samples: 240 successful and 30 explicit rejections. Three
  older local FITS captures have invalid HDU checksums; independent CFITSIO 4.7.0
  verification found valid data checksums (+1) but invalid HDU checksums (−1).
- 15 additional successful samples of three 714.29 MiB FITS files, maximum RSS
  6.33 MiB. No source was copied, repaired or modified by the probes.
- 1,255 successful primary unprofiled samples total, plus 30 rejected samples.
  Two instrumented profile runs and the initial aborted capture attempt (45
  structural successes and one failed full sample) are retained but not pooled.

Machine: Apple M4 Max, 36 GiB RAM, 14 logical/physical cores, macOS 26.6.2 ARM64,
Rust 1.94, optimized release builds. Earlier completion runs used AC power with
low-power mode off. Cache/desktop/core placement were uncontrolled. No builds or
tests ran concurrently with measurements. Swap snapshots/counters stayed zero;
snapshots are not continuous pressure telemetry. Source hash
`1b0a1b651e6c3a84b4a204268dfe9aa3eebf5556e0f754d5f268c16c3e7e4c1d`
ties the earlier measured sources together despite intervening tooling commits.

| Storage | Source context | Selected stored data |
| --- | --- | ---: |
| Internal APFS SSD | Ignored local `tests/data`, three FITS + three XISF | 96.24 MiB |
| User-described mechanical HDD | `/Volumes/WD_ElementsHD/RavenSkyTestFiles/QA` | 535.80 MiB |
| User-described external SSD | `/Volumes/ap_projects/0Working`, recursive selection | 672.82 MiB |
| External SSD large-file extension | Supplied `NGC7000/app`, three FITS | 2,142.86 MiB |

No network storage or user-owned Windows/Linux machine was available. Subsequent
hosted CI supplies build/test evidence, not equivalent storage timings.

## Managed compressed-FITS matrix

[Raw matrix and conditions](../../benchmarks/2026-09-08-managed-fits/README.md):
six cases × 1/2/4 workers × three repetitions = 54 samples, **432 operations**.
All succeeded, all source hashes matched, and reservations returned to zero.
Independent `fpack` 1.7.0 / CFITSIO 4.070 generated four 8-MiB-pixel files per case;
two validation passes per sample. Generation and hashing are outside timed work.
512 MiB shared admission and `min(256 MiB, 512 MiB / workers)` per call. Cache
likely warm but uncontrolled; this matrix did not measure power mode.

| Case | Four-worker speedup over serial | Maximum RSS (MiB) |
| --- | ---: | ---: |
| Rice noise / 32 rows | 3.496× | 6.72 |
| Rice gradient / 32 rows | 3.478× | 6.94 |
| PLIO mask / 32 rows | 3.045× | 6.56 |
| HCOMPRESS noise / 32 rows | 3.447× | 10.14 |
| HCOMPRESS noise / whole tile | 3.464× | **105.734375** |
| HCOMPRESS quantized float / whole tile | 3.311× | 79.47 |

Peak whole-tile reservations were about 289.51 MiB across four workers; all
samples passed predeclared RSS <192 MiB and reservations ≤512 MiB targets.
This is about **3–3.5× throughput** at four workers locally. Large HCOMPRESS tiles
consume admitted coefficient buffers; the small streamed-codec RSS cannot be
generalized to every tile. This matrix is not a native-versus-managed comparison.

Six separate whole-image HCOMPRESS SIGINT probes (1/4 workers, 256 passes,
signal one second after launch) stopped in 0.775–1.744 ms, all **below 1.8 ms**
and the 250 ms local target. The maximum recomputed value is 1.743541972 ms.
Sources were unchanged; cancellation did not emit a success report. Shutdown
includes process teardown and has no codec-phase handshake. Blocking I/O,
different hardware, foreground switching and pressure remain separate concerns.

## Reproduction

Use [BenchmarkGuide.md](../../BenchmarkGuide.md) for complete standalone Rust/CLI
examples, options and report interpretation. From the repository root:

```sh
cargo build --locked --release -p astro-bench --bin astro-bench --example capture_probe
python3 astro-bench/scripts/benchmark_suite.py --help
python3 astro-bench/scripts/capture_matrix.py --help
python3 docs/benchmarks/2026-09-08-managed-fits/run.py --output target/managed-fits-new
python3 docs/benchmarks/2026-09-08-managed-fits/cancel.py --output target/cancel-new
```

Choose new output directories and preserve diagnostic failures. Capture matrices
accept an explicit directory, optional recursion, per-format selection and
`--allow-rejected` for known-invalid input. The large-file extension uses the
three sorted FITS inputs, one pass, 1/2/4 workers and five fresh processes per
worker count. Preserve first-observed versus repeated-hash timing distinctions.
No unsafe cache purges or source alterations are part of reproduction.

## Prior verification checkpoints

`RELEASING.md` records 131 workspace tests and four doctests passing locally,
nine Python tests, formatting, Clippy/rustdoc with warnings denied and optimized
builds. Two tests are intentionally ignored: the local-only XISF capture loader
test and the pre-existing SEP test whose comment requires proper initialization.
The XISF test was run explicitly and passed (3856 × 2180, 8,406,080 pixels).
The SEP ignored test is not counted as passed and was not enabled during closeout.

An isolated copy with AstroMuninn's unchanged vendored `fitsio-sys 0.5.5` passed
108 applicable I/O/metadata/benchmark tests on macOS; the normal RavenSky lock
uses `fitsio-sys 0.5.7`. This is not a Windows MSVC result.

Four clean 0.6.0 package archives from `6237264` passed Cargo verification and
inspection in a fresh `target/package-check-060`. Notices were present and root
benchmark artifacts excluded. Cargo 1.94 offline multi-package verification hit
`no hash listed`; online verification worked. Reusing larger old archives also
left trailing bytes locally; fresh archives passed complete gzip-consumption
checks. Do not publish a stale preflight archive. Fresh release verification below
supersedes this checkpoint for the actual released contents.

[CI run 34237121494](https://github.com/dostergaard/ravensky-astro/actions/runs/34237121494)
passed on `2e9ba9505f0685839a14b5141d6faea20bbeb759`: complete Linux/macOS
workspace, Windows GNU I/O+metadata+benchmarks. Earlier failures identified SEP's
Windows `rand_r` dependency and two header tests deleting live native handles;
the latter tests now close handles first. No production loading change resulted.

## Fresh closeout and publication record

Fresh checks on 2026-09-09 passed, with complete commands and outputs retained:

- [Local command/results index](records/local-verification.json): 131 workspace
  tests, two intentionally ignored, four doctests, formatting, Clippy and rustdoc
  with warnings denied, optimized workspace/example builds and nine Python tests.
- [Explicit local XISF check](records/local-xisf.log): the ignored local capture
  loader test passed when invoked explicitly; SEP remains ignored.
- [Vendored-backend check](records/vendored-backend.json) and
  [log](records/vendored-backend.log): a fresh isolated source copy selected
  AstroMuninn's unchanged `fitsio-sys 0.5.5`, passing 108 tests with the unavailable
  local-capture test intentionally ignored. Application tracked files were unchanged.
- [Historical CI recheck](records/implementation-ci.json): actual GitHub results
  confirm all three jobs succeeded on `2e9ba95` in run `34237121494`.
- [Registry preflight](records/registry-preflight.json): all four intended 0.6.0
  versions returned 404 on 2026-09-09. No republishing or version changes are needed.

Task-document relative links, CI YAML/branch targets and manifest versions were
also checked. Production Rust code is unchanged from the approved head.

Clean review packages from `e294ba542960c1d780a5dc072e1f86bb6e823c2f` passed
`cargo package --locked --workspace --exclude astro-bench --target-dir
target/package-closeout-review`. The [build log](records/review-package.log) and
[archive audit](records/review-packages.json) retain hashes, sizes, clean source
identity, 0.6.0 dependency requirements, matched Rust sources, complete gzip ends
and notices. Reproduce inspection with [verify_archives.py](verify_archives.py):

```sh
python3 docs/work/file-validation-0.6.0/verify_archives.py \
  target/package-closeout-review/package target/package-inspection.json \
  e294ba542960c1d780a5dc072e1f86bb6e823c2f
```

Merge, actual release archives, publication and clean-consumer results follow
after execution; successful review checks do not imply publication.
