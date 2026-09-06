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

`--encoding` accepts `fits`, `xisf`, `zlib` and `zstd`. All frames contain one UInt16
monochrome image. Noise uses deterministic xorshift generation; gradient data
exposes highly compressible behavior. XISF includes a SHA-256 attachment checksum;
FITS uses ordinary images with no CHECKSUM/DATASUM. These are different workloads,
so they are not an isolated codec contest. No tiled FITS/native CFITSIO decode,
LZ4, byte shuffling, auxiliary blocks, camera metadata or scientific star-field
model is included yet. Existing validator correctness tests cover more variants.

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
  timing. The existing validator still allocates complete compressed subblocks and
  decoded output in full mode. Streaming those reads is the next implementation.
- Bounds: 1–256 frames, 64 MiB decoded/image, 1–16 workers, 1–20 repetitions,
  scratch quota default 512 MiB/maximum 8 GiB. Generation checks a conservative
  expansion allowance before creating files and enforces its quota on writes.
- Admission estimates 16 MiB overhead per worker plus three image buffers and
  expansion for compressed full validation, or a 64 KiB buffer otherwise. Samples
  exceeding 512 MiB estimated aggregate working memory are rejected. This estimate
  is specific to these generated fixtures, **not a hard RSS cap or an adaptive
  scheduler**. It is deliberately separate from future shared validator reservations.
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

Generation and pre-sample fingerprint verification touch every file. Cache state
is **uncontrolled / likely warm** even in fresh child processes. No cache flushing
is attempted. Small samples emphasize overhead and warm data; they do not establish
disk throughput, background responsiveness or release defaults. Keep noisy timing
assertions out of unit tests. Capture repeated release runs on representative
hardware/storage and real captures before setting application recommendations.

The CLI adds `ctrlc` for portable interrupts and a small isolated `libc::getrusage`
wrapper for Unix telemetry; generators reuse the existing codec/hash/serde stack.
