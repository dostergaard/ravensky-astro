# Managed compressed-FITS measurements — 2026-09-08

All 54 samples (432 full validations) passed, retained source fingerprints and
released their reservations. Six additional SIGINT probes exited cooperatively.
This supplements the earlier GZIP/XISF/capture investigation with the new managed
Rice, PLIO and HCOMPRESS paths. It does not compare against the old native decoder.

## Conditions and reproduction

- Apple M4 Max, 14 logical/physical cores, 36 GiB RAM; macOS 26.6.2 ARM64.
- Rust 1.94.0, optimized release profile, local internal storage, no simultaneous
  builds/tests. Cache state uncontrolled and likely warm; source hashing precedes
  every measured validation sample. Power mode was not measured.
- Independent Homebrew `fpack` 1.7.0 / CFITSIO 4.070 generates fixtures. Native
  parity tests separately use the dependency's bundled CFITSIO 3.49.
- Four generated inputs per case, each 8 MiB of original pixels; two passes per
  sample; three samples at each of 1, 2 and 4 workers. Timings exclude generation
  and source hashing. PLIO uses mask-like runs; Rice uses noise and gradients;
  HCOMPRESS uses noisy integer pixels and quantized floating-point pixels.
- 512 MiB shared admission; per-call limit `min(256 MiB, 512 MiB / workers)`.
  Predeclared acceptance: RSS below 192 MiB, reservations within 512 MiB, complete
  successful operations and unchanged sources. All passed. These are local
  investigation targets, not recommended application defaults.
- Reports retain compiler, revision, source/build hashes and input hashes.
  The measured build was based on `c9c047d` plus the managed implementation;
  its source fingerprint is in `results/summary.json`. Subsequent edits to API
  comments and release versions change that fingerprint without changing decoding.

From the repository root, with Python 3.12+ and `fpack` on PATH:

```sh
cargo build --locked --release -p astro-bench --example capture_probe
python3 docs/benchmarks/2026-09-08-managed-fits/run.py --output target/managed-fits-new
python3 docs/benchmarks/2026-09-08-managed-fits/cancel.py --output target/managed-fits-cancel-new
```

Choose new output directories. The runners remove only their exclusively created
temporary fixtures. Allow at least 1 GiB of free temporary disk space. Generation
is bounded but intentionally outside the measurement; the Python generator is not
a throughput benchmark. Empty generation logs/stderr show quiet successful tools.

## Results

Wall times are medians for eight operations, not per-file latencies. Raw JSON
contains individual operation times and min/max repetition ranges.

| Workload | 1 worker (s) | 2 workers (s) | 4 workers (s) | 4-worker speedup | Maximum RSS (MiB) | Maximum reserved (MiB) |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| Rice noise, 32-row tiles | 0.5303 | 0.2712 | 0.1517 | 3.50× | 6.72 | 0.344 |
| Rice gradient, 32-row tiles | 0.1135 | 0.0574 | 0.0326 | 3.48× | 6.94 | 0.344 |
| PLIO masks, 32-row tiles | 0.00953 | 0.00510 | 0.00313 | 3.05× | 6.56 | 0.328 |
| HCOMPRESS noise, 32-row tiles | 1.5233 | 0.7851 | 0.4419 | 3.45× | 10.14 | 4.863 |
| HCOMPRESS noise, whole-image tile | 1.6350 | 0.8399 | 0.4720 | 3.46× | 105.73 | 289.510 |
| HCOMPRESS quantized float, whole-image tile | 0.3986 | 0.2083 | 0.1204 | 3.31× | 79.47 | 132.782 |

Whole-image HCOMPRESS demonstrates why tile geometry must affect admission. Its
coefficient working set grows with tile pixels; reservations deliberately exceed
measured resident memory. The streamed codecs retain small buffers. Neither
reservation telemetry nor these RSS observations prove an OS-enforced ceiling.

Cancellation runs use one noisy whole-image HCOMPRESS input, 256 passes, 1 or 4
workers and SIGINT one second after launch. All six stopped in **0.775–1.744 ms**,
below the predeclared 250 ms shutdown target, returned cancellation rather than
success JSON, and preserved the input hash. Timing includes process teardown;
there is no phase handshake to isolate a specific decoder instruction. Blocking
filesystem calls and other machines can behave differently.

## Limits of this evidence

No Windows/Linux timings, network storage, induced OS memory pressure or human
foreground application-switching assessment were available. No adaptive scheduler
exists in the library; selecting application concurrency remains downstream work.
The short PLIO samples are especially sensitive to timer and scheduling overhead.
Source preservation and deterministic resource tests accompany the timings, but
format compatibility still depends on the documented supported profiles. Lossy
validation checks representation integrity, not scientific fidelity.
