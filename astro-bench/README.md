# astro-bench

Reproducible synthetic workloads and measurements for RavenSky libraries. This
optional crate provides a reusable Rust API and a CLI. Production crates and the
umbrella facade do not depend on it. The first workloads measure raw reads and
structural/full validation; metadata, metrics and application calibration follow
separately. See the [design](../docs/BenchmarkAndCalibrationDesign.md).

This initial crate is unpublished development tooling. Workspace applications can
consume it through a path dependency. Publishing it will require a stable resource
API and standalone build-provenance packaging; the current fingerprint includes
the adjacent workspace validator sources.

## Run a benchmark

New users: follow the [standalone benchmark guide](../docs/BenchmarkGuide.md) for
prerequisites, a small first run, worker comparisons, JSON interpretation, existing
captures and a complete separate-project Rust example. This README is the compact
tool and resource-contract reference.

From the workspace root:

```sh
cargo run --release -p astro-bench -- run \
  --encoding zstd --pattern noise --workload full \
  --width 2048 --height 2048 --frames 8 \
  --workers 1,2,4 --repeats 3 \
  --scratch /private/tmp --output zstd-baseline.json \
  --note 'Describe CPU, RAM, OS, scratch volume and competing workloads'
```

Use an existing scratch directory on the volume being measured; omit `--scratch`
to use the platform temporary directory. `--help` lists bounds and options. Reports
are created exclusively: an existing output is never overwritten. Ctrl-C requests
cancellation and the CLI stops/reaps any current read-only sample child before
removing its owned fixtures. Each child's configurable timeout includes initial
fingerprint verification. Preparation uses cooperative cancellation, without a
hard deadline for a blocked filesystem operation.

The runner measures separate operations, selected with `--workload`:

| Workload | Work performed | Meaning of throughput |
|---|---|---|
| `read` | Sequential file reads using a 64 KiB buffer per worker | Stored bytes / wall time; likely cached, not a disk-speed claim |
| `structural` | Declared layout and extent checks | Container size / wall time; most payload bytes are not read |
| `full` | All-byte reads, supported decoding and declared checksum verification | Stored bytes / wall time; managed reads may include repeated bytes |

`--encoding` accepts `fits`, `fits-gzip`, `fits-gzip2`, `xisf`, `zlib` and `zstd`. All frames contain one UInt16
monochrome image. Noise uses deterministic xorshift generation; gradient data
exposes highly compressible behavior. XISF includes a SHA-256 attachment checksum;
FITS has no CHECKSUM/DATASUM; GZIP tiles additionally verify their embedded CRC32
and size. These are different workloads, so they are not an isolated codec contest.
Native CFITSIO decode, LZ4, auxiliary blocks, camera metadata and scientific
star-field models are not included. Existing validator correctness tests cover more variants.

For FITS GZIP, `--tile-rows N` selects the height of full-width tiles; omit it for
one whole-image tile or use `1` for row tiles. The last tile may be shorter.
GZIP_2 groups each tile's high and low sample bytes into separate planes before
compression. Both encodings use Q byte descriptors and preserve the same logical
UInt16 values as ordinary FITS/XISF. Generation and validation use bounded Rust
streams; native CFITSIO is used only in independent fixture tests.

Run the compressed-FITS matrix after a release build:

```sh
sh astro-bench/scripts/fits-gzip-baseline.sh REPORT_DIRECTORY SCRATCH_DIRECTORY \
  'Hardware, storage and competing-workload notes'
```

This creates 24 reports and 216 isolated samples spanning GZIP_1/GZIP_2, row/image
tiles, noise/gradient, 8/32 MiB images and 1/2/4 workers. It establishes the current
bounded validator's baseline, without comparing historical native implementations.

The extended investigation uses Python 3.12+ standard-library tooling around the
same CLI (from this repository's root):

```sh
python3 astro-bench/scripts/benchmark_suite.py scaling --output RESULTS/scaling \
  --scratch SCRATCH_DIRECTORY --note 'Hardware, storage, power and competing load'
python3 astro-bench/scripts/benchmark_suite.py contention --output RESULTS/contention \
  --scratch SCRATCH_DIRECTORY --note 'Hardware, storage, power and competing load'
python3 astro-bench/scripts/benchmark_suite.py cancellation --output RESULTS/cancellation \
  --scratch SCRATCH_DIRECTORY --note 'Hardware, storage, power and competing load'
python3 astro-bench/scripts/benchmark_suite.py profile --output RESULTS/profile \
  --scratch SCRATCH_DIRECTORY --note 'Hardware, storage, power and competing load'
```

Each output directory must be new. `scaling` records 960 samples (forward/reverse
worker order, five repetitions, 1/2/4/8 workers, 8/64 MiB images and row/32-row/image
tiles). `contention` records a separate 20 ms scheduling / 64 KiB SHA-256 probe,
alone and with two CPU competitors retaining 32 MiB of touched data each. It keeps
preparation separate from the sample phase, which includes child fingerprint
verification and inter-sample gaps. This proxy does not measure application
switching or low-memory pressure. `cancellation` measures CLI SIGINT exit/cleanup
during generation and the sample-child lifetime (including hash verification),
not the library's cooperative cancellation latency during decode. It requires
POSIX and `pgrep`. `profile` additionally requires macOS `/usr/bin/sample`; its
instrumented timings are excluded from throughput comparisons.

Experiments have ten-minute per-command deadlines, one CLI at a time and existing
scratch quotas (up to 4 GiB preflight for profiling; most matrices use 1 GiB).
The child still has its independent 120-second deadline. No system-wide cache or
memory-pressure manipulation is performed. On failure, keep logs/partial evidence
and rerun into a new directory. See [completion plan](../docs/BenchmarkCompletion.md).

Test report auditing and matrix contracts with
`python3 -m unittest discover -s astro-bench/scripts -p test_benchmark_suite.py`.

## Measure existing captures

Build the separate read-only diagnostic example:

```sh
cargo build --workspace --all-features --examples --release
python3 astro-bench/scripts/capture_matrix.py CAPTURE_DIRECTORY \
  --recursive --per-format 3 --output RESULTS/captures \
  --note 'Hardware, storage, power and competing load'
```

The matrix chooses up to three files of each format in sorted traversal order,
records first-observed and repeated sequential reads with SHA-256, then measures
FITS-only, XISF-only and mixed groups at structural/full levels, 1/2/4 workers,
16 passes and five fresh-process repetitions (90 samples). Selection and file
hashes are stable across samples. Read-plus-hash timing includes checksum CPU;
first-observed reads are not guaranteed cold. Files are never copied or modified.
Passes use one atomic operation sequence and may overlap, including concurrent
reads of the same file. This measures sustained cached work; it does not emulate
an arrival stream of unique captures.

For explicit files or a diagnostic that may fail:

```sh
target/release/examples/capture_probe full 4 16 FILE1.fits FILE2.xisf
```

Capture reports have their own `kind: capture_probe` schema and example-source
fingerprint. They omit input names/metadata, retain indexed SHA-256 identities,
operation outcomes/timings, CPU/RSS, shared reservations and before/after byte
preservation. Hashes before timed validation warm caches. Worker startup/join is
timed; hash preparation/final verification are excluded. No total speed score is
produced for failed or unsupported inputs (exit status 2 with diagnostic JSON).
The matrix normally stops on rejection. Use `--allow-rejected` for an explicitly
diagnostic corpus: it keeps failed samples and an `outcomes.json` ledger while
continuing other groups. These samples remain `complete: false` and must be
excluded from throughput comparisons. Source changes and process failures still
abort the matrix.
Other errors/cancellation return failure; the supervisor keeps logs and enforces
a 120-second process deadline. Direct example execution uses cooperative Ctrl-C,
which cannot interrupt a blocked OS read.

Bounds: 256 regular input files / 8 GiB stored total, 1–8 workers, 1–256 passes and
4,096 retained operation records. Hashing uses 64 KiB chunks. All workers share
512 MiB reservations; each call is limited to `min(256 MiB, 512 MiB / workers)`.
Operation records/runtime overhead are additional bounded memory, not included
in validator reservations. Shared full native-codec exclusions still apply.
RSS is process-lifetime and null where unavailable. Use controlled directories;
final-component symlinks/duplicate canonical paths are rejected, but this is not
a sandbox for hostile directory mutation. Applications' automatic policy and
settings remain outside this diagnostic example.

## Use the library

```rust,no_run
use astro_bench::{FixtureSet, Recipe, Workload, run_sample};
use std::sync::atomic::AtomicBool;

let cancel = AtomicBool::new(false);
let fixtures = FixtureSet::generate(&std::env::temp_dir(), Recipe::default(), &cancel)?;
let sample = run_sample(&fixtures, Workload::Full, 2, &cancel, |file| {
    // Called on the coordinating thread; keep callbacks cheap.
    eprintln!("Completed frame {}", file.index);
})?;
assert_eq!(sample.completed_files, fixtures.manifest().files.len());
fixtures.cleanup()?;
# Ok::<(), anyhow::Error>(())
```

Applications can reuse recipes, samples, progress and cancellation. The library
does not install a signal handler or change settings. It joins workers on errors
and cancellation and never returns partial success. OS reads can delay cooperative
cancellation. Applications must label in-process RSS as process-lifetime; only the
CLI isolates each sample's memory high-water mark from prior samples/generation.

## Resource bounds and measurement limits

- Generation streams 64 KiB pixel chunks and codec output; no whole-image generator
  buffer or sparse file. Each frame is synced, fully validated and hashed before
  timing. Zlib/Zstandard validation now streams decoded output; LZ4 remains a
  bounded whole-block path (not part of the initial benchmark matrix).
  GZIP_2 revisits deterministic pixel state for its second byte plane instead of
  retaining a tile buffer. FITS headers/descriptors are backpatched only inside
  already-accounted space. Tile heights must be 1..=image height with at most
  16,384 tiles/image. Quota preflight includes per-tile expansion and descriptors.
- Bounds: 1–256 frames, 64 MiB decoded/image, 1–16 workers, 1–20 repetitions,
  scratch quota default 512 MiB/maximum 8 GiB. Generation checks a conservative
  expansion allowance before creating files and enforces its quota on writes.
- Admission estimates 16 MiB overhead per worker plus three image buffers and
  expansion for XISF compressed full validation, 2 MiB for bounded FITS GZIP full
  validation, or a 64 KiB buffer otherwise. Samples
  exceeding 512 MiB estimated aggregate working memory are rejected. This estimate
  is specific to these generated fixtures, **not a hard RSS cap or an adaptive
  scheduler**. The original conservative preflight is retained for comparable
  workloads. Validation workers now additionally share one 512 MiB `MemoryBudget`;
  samples record `peak_reserved_bytes` separately from measured RSS and verify
  all reservations are released. Capacity failures invalidate the sample.
- Work distribution uses an atomic index and a bounded results channel (twice the
  worker count). Measurements contain at most one entry per generated frame. Codec
  generation is single-threaded; workers each run one validation/read at a time.
- Scratch cleanup is limited to the instance's exclusively created directory.
  `FixtureSet::open` checks names, lengths, versions, regular files and hashes,
  rejects symlinks, and never owns cleanup. Use controlled scratch directories;
  this API is not a secure sandbox for attacker-modified directory trees.
  Drop cleanup is best effort; explicit cleanup reports failures. Abrupt termination
  can leave `astro-bench-*` directories to remove manually.

Version-one JSON reports retain recipe/content fingerprints, measured-source hash
(validator, harness, manifests and lockfile), build/compiler/revision, environment
notes, all repetitions and min/median/max summaries. Per-file p50/p95 pool completed
file timings within the same worker group. CPU time is a process user+system delta.
Peak RSS uses `getrusage`: bytes on macOS, KiB converted to bytes on Linux, `null`
elsewhere. It includes child setup and native/runtime allocations, excludes parent
generation, and does not measure filesystem cache or foreground responsiveness.
Fixture generator version 2 identifies the new tiled-FITS recipes; original
encodings retain version 1 and unchanged bytes. Optional `tile_rows` is omitted
when unspecified, so old recipes/manifests remain readable.

Generation and pre-sample fingerprint verification touch every file. Cache state
is **uncontrolled / likely warm** even in fresh child processes. No cache flushing
is attempted. Small samples emphasize overhead and warm data; they do not establish
disk throughput, background responsiveness or release defaults. Keep noisy timing
assertions out of unit tests. Capture repeated release runs on representative
hardware/storage and real captures before setting application recommendations.

The CLI adds `ctrlc` for portable interrupts and a small isolated `libc::getrusage`
wrapper for Unix telemetry; generators reuse the existing codec/hash/serde stack.

Recorded evidence: [initial baseline](../docs/benchmarks/2026-09-06-m4-max/README.md)
and [shared-reservation/streaming comparison](../docs/benchmarks/2026-09-06-m4-max-streaming/README.md),
plus the [bounded FITS GZIP baseline](../docs/benchmarks/2026-09-06-m4-max-fits-gzip/README.md).
The [completion investigation](../docs/benchmarks/2026-09-07-m4-max-completion/README.md)
adds larger/tiled/worker-order comparisons, sampled profiles, a responsiveness
proxy, cancellation and real captures on three supplied volumes.
