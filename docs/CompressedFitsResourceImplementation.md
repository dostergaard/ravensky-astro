# Remaining compressed-FITS resource controls

Status: implemented and locally verified on `feature/file-validation` (2026-09-08).
Merge and publication remain subject to the user's review and platform CI.

## Objective

Finish bounded full validation of the declared compressed-FITS coverage without
allowing hidden CFITSIO tile/cache allocations through the validation API.

## Constraints and non-goals

Preserve structural validation, checksums before decoding, source stamps, typed
failures and caller-owned budgets. Do not change image-loader outputs, add an
application scheduler, or claim an OS-enforced RSS limit. Unknown compression
extensions must remain explicit Unsupported outcomes. No merge or publication
before review.

## Current state and implementation choice

Before this change, integer GZIP used bounded managed streams. Other compressed layouts used estimated
native allocations for standalone validation and are excluded from shared calls.
CFITSIO's low-level Rice/Hcompress/PLIO decoders are not suitable for direct FFI
on arbitrary input: length checks are incomplete or absent. Native allocator
interposition would also couple consumers to a particular backend build, including
AstroMuninn's vendored patch. A portable isolated worker needs separate process
distribution and OS-specific resource controls.

Extend the managed validation path instead. Reuse existing bounded input, GZIP,
table geometry and reservation infrastructure. Audit existing Rust codecs before
adding a dependency or adapting code. The inspected `ricecomp 0.5.0` and
`hcompress 0.4.0` APIs require whole-tile buffers and lack cancellation and exact
consumption reporting; Hcompress still uses infallible byte getters on truncated
input. `fitskit 0.3.0` has reusable algorithm code but also allocates before
admission and tolerates some malformed/trailing input. None is a drop-in validator.
Any adapted code must retain its license and attribution and receive independent
native-fixture and malformed-input tests.

## Proposed design and resource model

Generalize tile dispatch to named P/Q columns, per-row quantization parameters,
GZIP/raw fallback columns and null-mask payloads. Stream Rice and PLIO decoding
without retaining image arrays. For Hcompress, reserve the coefficient array and
scratch before fallible allocation, bound dimensions/bitplanes from the stream
against the FITS tile, and check cancellation throughout entropy/transform work.
Do not buffer whole files or retain idle tile caches. Keep one caller worker with
no codec threads. Every success/error/cancellation releases reservations; busy
admission returns without waiting. Decoding must account exact input/output
extents and reject malformed instructions without panics or native fallback.

Validation checks container/payload integrity, not rendering choices or the
scientific fidelity of lossy compression. Document any distinction between
checking quantization/smoothing metadata and producing display pixels.

## Affected areas and execution

1. Add failing tests for native-generated codecs through shared admission.
2. Implement bounded tile/codec handling in `astro-io/src/validation/fits/` and
   remove the validator's estimated native fallback when coverage is verified.
3. Exercise floating-point/fallback/mask cases, malformed streams, dimensions,
   limits, concurrency and cancellation; preserve independent decoder evidence.
4. Update public coverage docs, benchmark supported representative workloads,
   then prepare packaging/version/CI work as separate rollback checkpoints.

## Verification strategy

Use CFITSIO-generated fixtures as an independent oracle, plus manually malformed
headers/streams. Compare outcomes and decoded values where algorithms are adapted.
Test all byte truncations of small streams, surplus bytes/output, impossible
dimensions, reservation release and cancellation. Run formatting, Clippy, workspace
tests/doctests, documentation and release builds. Keep performance runs separate
from builds/tests; compare serial/fixed workers, reservations/RSS and cancellation.
Existing GZIP measurements remain a regression baseline, not evidence for new
codecs. Set measurement targets before collecting new results.

## Risks and open evidence

Hcompress needs an indivisible coefficient working set; reject oversized tiles
before allocation. Quantization and fallback layouts need native-produced samples
in addition to synthetic fixtures. Backend loader allocation limits remain distinct
from managed validator guarantees. Windows and Linux CI must be verified before
release; native dependency compatibility in AstroMuninn needs an explicit check.
Unknown extensions and platform/pressure gaps remain documented release decisions.

### Native test-oracle limitation found during verification

The bundled CFITSIO 3.49 PLIO encoder can overrun its output allocation for
high-delta positive 32-bit samples: `imcomp_calc_max_elem` reserves `4 * pixels`
bytes, while `pl_p2li` can emit two 16-bit words per pixel plus its seven-word
header. A 64 × 64 test image with 64 × 8 tiles reproduced a macOS allocator
SIGTRAP in `imcomp_compress_tile`, even with a single test thread. Logging changed
the heap layout and hid the symptom; it was not a fix. Native-generated PLIO
fixtures now use mask-like values with a proven margin under this allocation.
Malformed/high-value instruction cases use bounded hand-built streams. This
affects the test encoder, not the new managed validator, which never calls it.
It is another reason not to treat the native library's estimates as hard bounds.

## Verification result

Local macOS verification passed: workspace formatting, Clippy with warnings denied,
all-target/all-feature tests, four doctests, rustdoc with warnings denied, optimized
workspace/example builds and nine benchmark-script tests. Codec tests compare
Rice/HCOMPRESS decoded samples with independent native results, including lossy
HCOMPRESS transforms, odd geometry and signed values. Small streams undergo every
truncation and byte mutation; randomized bounded HCOMPRESS streams must not panic.
Integration tests cover float/quantized/fallback/mask profiles, source preservation,
per-call limits, occupied shared admission, cancellation and concurrent cleanup.

The [54-sample measurement record](benchmarks/2026-09-08-managed-fits/README.md)
adds 432 successful operations, 1/2/4-worker scaling, measured RSS and six
cancellation probes. All predeclared local targets passed. Other-platform CI,
package verification and the downstream vendored-backend check remain release
preparation work; this is not approval to merge or publish.

## References

- [FITS tiled compression convention 2.3](https://fits.gsfc.nasa.gov/registry/tilecompression/tilecompression2.3.pdf)
- Bundled CFITSIO 3.49: `ricecomp.c`, `pliocomp.c`, `fits_hdecompress.c`, `imcompress.c`
- [fitskit](https://github.com/ssmichael1/fitskit),
  [ricecomp](https://github.com/cruzzil/ricecomp),
  [hcompress](https://github.com/cruzzil/hcompress)
