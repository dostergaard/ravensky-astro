# astro-io

I/O operations for astronomical image formats.

## Overview

`astro-io` provides functionality for loading and saving astronomical image formats, including FITS and XISF. It handles the low-level details of file I/O, image data extraction, and compression.

## Features

- FITS file loading
- XISF file loading
- Efficient image data handling
- Support for various data types (8-bit, 16-bit, 32-bit float)

## CFITSIO concurrency

FITS loading, header access and metadata extraction share
`astro_io::fits::backend`. The linked backend's
`is_reentrant()` capability controls admission: independent handles run concurrently
on reentrant builds; other builds admit one thread with nested calls supported.
Loaders wait for admission. FITS and XISF validation do not acquire this native
gate. Validation uses separate caller-owned memory admission and reports
`ResourceBusy` when concurrent reservations temporarily occupy its allowance.

Callers using `fitsio` directly must enclose open, operations, error handling and
close/drop in `with_cfitsio` or `try_with_cfitsio`. Do not let live handles escape
the closure. Borrowed-handle helpers protect their own operations, but cannot cover
the caller's separate opens/closes. Uncoordinated direct calls and independently
linked copies remain outside the gate. See the
[control design and native allocation audit](../docs/CfitsioControlImplementation.md).

## Windows FITS Path-Length Note

On Windows, FITS file access in AstroMuninn and the ravensky-astro FITS APIs depends on CFITSIO (via `fitsio` / `fitsio-sys`). CFITSIO currently opens disk files using its `fopen`-based path handling (`file_openfile`), which in this environment follows the classic Windows path-length boundary.

Use full FITS paths shorter than 260 characters (`< 260`). At 260 or more, FITS open calls may fail.

This limitation is specific to FITS loading and metadata access through CFITSIO.
Managed FITS validation and XISF handling do not use that path implementation.

## Installation

Add to your `Cargo.toml`:

```toml
[dependencies]
astro-io = "0.6.0"
```

## API Reference

### FITS Module

```rust
/// Read a FITS file and return its pixel data, width, and height
pub fn load_fits(path: &Path) -> Result<(Vec<f32>, usize, usize)>
```

- **Parameters**:
  - `path`: Path to the FITS file
- **Returns**:
  - A tuple containing:
    - `Vec<f32>`: Pixel data as a flattened vector of 32-bit floats
    - `usize`: Width of the image in pixels
    - `usize`: Height of the image in pixels
- **Errors**:
  - If the file cannot be opened
  - If the primary HDU is not an image
  - If the image data cannot be read
  - On Windows, FITS open may fail when the full pathname is 260 characters or longer due to CFITSIO `fopen` path handling.

```rust
/// Normalize pixel values to a 0.0-1.0 range
pub fn normalize_pixels(pixels: &[f32]) -> Vec<f32>
```

- **Parameters**:
  - `pixels`: Slice of pixel values
- **Returns**:
  - `Vec<f32>`: Normalized pixel values in the range 0.0-1.0

### XISF Module

```rust
/// Read an XISF file and return its pixel data, width, and height
pub fn load_xisf(path: &Path) -> Result<(Vec<f32>, usize, usize)>
```

- **Parameters**:
  - `path`: Path to the XISF file
- **Returns**:
  - A tuple containing:
    - `Vec<f32>`: Pixel data as a flattened vector of 32-bit floats
    - `usize`: Width of the image in pixels
    - `usize`: Height of the image in pixels
- **Errors**:
  - If the file cannot be opened
  - If the XISF signature is invalid
  - If the XML header cannot be parsed
  - If required image attributes such as `geometry`, `sampleFormat`, or `location` are missing or invalid
  - If the file uses an unsupported XISF variant such as compressed, non-`UInt16`, or multi-channel image data
  - If the image payload is truncated or cannot be read

Current scope:

- Uncompressed attachment-backed image data
- Single-channel images
- `UInt16` samples decoded to normalized `f32`

## Usage Examples

### Loading a FITS file

```rust
use astro_io::fits;
use std::path::Path;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = Path::new("/path/to/image.fits");
    let (pixels, width, height) = fits::load_fits(path)?;
    
    println!("Image dimensions: {}x{}", width, height);
    println!("Total pixels: {}", pixels.len());
    
    // Normalize pixel values to 0.0-1.0 range
    let normalized = fits::normalize_pixels(&pixels);
    
    Ok(())
}
```

### Loading an XISF file

```rust
use astro_io::xisf;
use std::path::Path;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = Path::new("/path/to/image.xisf");
    let (pixels, width, height) = xisf::load_xisf(path)?;
    
    println!("Image dimensions: {}x{}", width, height);
    println!("Total pixels: {}", pixels.len());
    
    Ok(())
}
```

## License

This project is dual-licensed under the MIT License or the Apache License, Version 2.0.

## File validation

Introduced in the 0.6.0 release line.

`astro_io::validation::validate_file` checks a local file read-only, independently
of the pixel loaders and metadata normalization. Format detection uses file
bytes. `ValidationOptions::default()` selects `Structural`; use
`.with_level(ValidationLevel::Full)` for the stronger check. The module rustdoc
contains a compilable usage example.

| Coverage | Structural | Full adds |
| --- | --- | --- |
| FITS primary/image HDUs, all standard sample widths and dimensions | Mandatory header order, checked sizes, complete padded HDU chain | Reads all physical bytes |
| FITS ASCII/binary tables | Column layout and variable-array heap descriptor bounds | Reads table/heap storage |
| FITS tiled images | Tile geometry, table/heap extents, declared decoded size | Managed GZIP_1/GZIP_2, Rice, PLIO, HCOMPRESS and raw/fallback tile validation below |
| FITS `DATASUM` / `CHECKSUM` | Presence and encoding | Stored-data/HDU ones-complement verification |
| XISF 1.0 images, thumbnails, profiles and property blocks | Prefix/XML, local references, geometry, sample widths, block extents | Reads every physical byte and local block |
| XISF attachments, inline Base64/hex, embedded Data | Location/encoding and declared lengths | Payload checks below |
| XISF zlib, LZ4, LZ4HC, Zstandard, shuffle and subblocks | Stored/decoded lengths and descriptor consistency | Decompression and exact output-length verification |
| XISF SHA-1, SHA-256, SHA-512, SHA3-256, SHA3-512 | Present checksum descriptors | Digest verification over stored bytes before decoding |

XISF validation supports multiple images/channels and UInt8/16/32/64,
Float32/64 and Complex32/64 samples. It does not inherit the UInt16 monochrome
restriction of `load_xisf`. Shuffle is a reversible byte permutation; validation
checks its descriptor and decoded length without interpreting numeric samples.
Local `Reference` targets are checked and their underlying blocks validated;
image counts refer to physical Image elements, not reference aliases or thumbnails.

A `ValidationReport` identifies the completed level, source stamp, image and
structure counts, I/O byte count, checksum coverage, and `undecoded_codecs()`.
Structural reports explicitly list compressed algorithms not decoded, including
unknown algorithms with understandable layouts. Full mode returns `Unsupported`
for unknown codecs or checksum algorithms and never downgrades to structural
success. Missing optional checksums are allowed; `checksums().present() == 0`
means no checksum protection was available. I/O counts include managed rereads; validation no longer calls CFITSIO.

Typed failures distinguish incomplete input, invalid structure, integrity
mismatch, unsupported features, resource limits, source changes, cancellation,
and I/O errors. Applications own quiet periods, retries, and presentation.

### Bounds and limitations

Defaults: 64 MiB combined header bytes, 256 MiB working allowance per call,
100,000 structures, and 64 GiB combined declared decoded data. The decoded limit
is total data checked, not a RAM allocation or physical-file-size limit.
`ValidationLimits` exposes each nonzero limit. Working limits now include retained
buffers and conservative parser/backend allowances, so a header/codec stage can
be rejected earlier than under the previous estimates.

Use `validate_file_with_budget(path, options, cancel, &budget)` with one cloneable
`MemoryBudget` shared across concurrent calls. `validate_file` remains available
and creates a private allowance for that call. Reservations are acquired before
buffer/stage allocation and released automatically on success, failure, cancellation
and unwinding. `ResourceBusy` means another reservation temporarily prevents
admission; queue/retry outside workers. `ResourceLimit` means this call's retained
memory plus the next requirement cannot fit its per-call or total shared capacity.
No validator call waits while holding a partial reservation. The application owns
fairness, pressure adaptation, worker counts and its returned reports/queue memory.

```rust,no_run
use astro_io::validation::{MemoryBudget, ValidationOptions, validate_file_with_budget};
use std::path::Path;
let budget = MemoryBudget::new(64 * 1024 * 1024)?;
let report = validate_file_with_budget(Path::new("image.xisf"),
    &ValidationOptions::default(), None, &budget)?;
assert_eq!(budget.used_bytes(), 0);
println!("Peak call reservations: {}", report.peak_reserved_bytes());
# Ok::<(), astro_io::validation::ValidationError>(())
```

| Stage | Memory behavior |
|---|---|
| File reads/checksums | Fallible, reserved buffers of at most 64 KiB; final whole-file read preserves gap/padding coverage |
| XISF XML | Reserve 32× XML length for parser/string scratch and growth overlap, plus 1 KiB per node/attribute before insertion; retain metadata allowance through traversal |
| Inline XISF | Reserved packed-text and decoded-byte buffers; LZ4 subblocks borrow inline slices instead of cloning them |
| Zlib | 64 KiB input/output buffers, 1 MiB inflater-state allowance; decoded bytes are counted and discarded; require explicit stream end, exact output and no trailing bytes |
| Zstandard | 64 KiB input/output, rounded declared frame history plus 1 MiB context/block allowance; admit each concatenated/skippable frame independently and enforce native window limit |
| LZ4/LZ4HC | Complete attached input and decoded output blocks must fit live reservations; no streaming claim for the raw-block decoder |
| FITS header/tables | 1 KiB per retained structural keyword covers map/string/table bookkeeping before insertion |
| FITS GZIP_1/GZIP_2 tiles and GZIP fallback | Reserve 1 MiB inflater + 256 KiB header allowance, ≤64 KiB each input/output, and axis bookkeeping. Stream/discard output without a decoded-tile cache |
| FITS Rice / PLIO | ≤64 KiB buffered compressed input; decode differences / execute run instructions without allocating a pixel array |
| FITS HCOMPRESS | Reserve stored compressed bytes plus `16 × tile_pixels + 64 KiB` for coefficients and scratch before fallible allocation. Geometry and bitplanes are checked before allocation and the buffered header must match the admission observation. No native calls or retained tile cache |

### Compressed-FITS coverage

Both validation entry points use the same managed decoders; there is no estimated
native fallback. Supported profiles include P/Q descriptors, arbitrary column
ordering, short edge tiles and the following codecs:

- GZIP_1/GZIP_2: integer and lossless Float32/64 payloads, or 32-bit quantized
  integers. One complete member per tile, exact decoded size, CRC32/ISIZE and
  no trailing bytes. Optional headers are limited to 64 KiB. Concatenated members
  are `Unsupported`. Unshuffling is a reversible permutation; no pixel array is
  required for validation.
- RICE_1/RICE_ONE: BYTEPIX 1/2/4, explicit coding-block limits (1–65,536 pixels;
  default 32), bounded unary codes, exact byte consumption and zero padding.
  BYTEPIX 8 is explicitly unsupported.
- PLIO_1: checked short/long line-list headers, bounded instructions, nonnegative
  24-bit values and output runs that cannot exceed the tile. An unwritten tail
  is implicitly zero, as defined by the decoder contract.
- HCOMPRESS_1: 32/64-bit coefficient transforms, positive two-dimensional tile
  geometry (additional unit axes allowed), lossy scale and exact stream ends.
  A tile exceeding the working allowance is rejected before decoding.
- NOCOMPRESS: exact raw payload extent. GZIP_COMPRESSED_DATA and legacy
  UNCOMPRESSED_DATA fallback columns are supported; exactly one payload per row
  must be nonempty. Null masks support GZIP, Rice and PLIO.

Quantized Float32/64 tiles validate scale/zero metadata, supported NO_DITHER /
SUBTRACTIVE_DITHER_1 / SUBTRACTIVE_DITHER_2 declarations and dither seeds, then
validate the encoded integer payload. This does not render dequantized pixels,
apply optional HCOMPRESS display smoothing, or assess scientific fidelity.
NaN/undefined image samples remain legal. Scaled/nullable compressed-image table
columns and unknown codecs/extensions return explicit unsupported outcomes.
Present FITS checksums are verified before payload decode; decoded-size preflight
still precedes checksum I/O. See the
[compressed-FITS resource implementation](../docs/CompressedFitsResourceImplementation.md).
The adapted HCOMPRESS algorithm's upstream notices are included in `licenses/`.

`MemoryBudget::used_bytes()` and `peak_bytes()` report reservation accounting;
`ValidationReport::peak_reserved_bytes()` reports the call high-water mark. These
are not RSS measurements or OS memory reservations. Exact buffer allocations use
`try_reserve_exact`; XML/BTreeMap/codec internals and small bookkeeping allocations
can still allocate infallibly. Metadata/backend allowances are conservative,
not native allocator interception. Allocator overhead, thread stacks, OS cache and
returned reports are outside the working allowance, so arbitrary OOM recovery or
a hard process-memory ceiling is not promised.

The [resource implementation design](../docs/ValidationResourceImplementation.md)
records scope, backend evidence and performance targets. Consumers still need
scheduler/readiness integration, real-capture/platform verification and measured
foreground responsiveness before enabling this as a monitoring release gate.

The cancellation flag is checked between parser steps, reads and decode chunks.
A blocked OS read or an individual external codec call cannot be interrupted.
Managed FITS loops also check cancellation during entropy/transform work.
Size and modification time are compared on both the open handle and pathname
before return; Unix also compares device/inode identity. Other platforms
currently use size/time only. These observations cannot detect every concurrent
write, and a producer may resume after a successful call. Consumers must compare
the returned stamp and recheck readiness before operating on the file.

Neither level proves producer completion or scientific correctness. Correctly
sized preallocated data can pass; absent a checksum, arbitrary changes to raw
pixel bytes can also pass full validation. NaNs/undefined samples are not rejected.
This is container/payload validation, not exhaustive FITS keyword, XISF metadata,
ICC profile, color-space, or XML-signature conformance/authentication.

FITS random groups, unknown HDU extensions and externally wrapped files (such
as whole-file gzip) return `Unsupported`. FITS validation opens literal local
paths without CFITSIO filter interpretation. XISF external
block locations, foreign XML element namespaces, DTDs, and external entities
are unsupported; the validator does not fetch external resources. Namespace-less
XISF headers are accepted for producer compatibility. Existing image loaders and
metadata APIs still use the coordinated native backend and retain its platform
limitations; validation's managed allocation contract does not cover those loaders.

Tests generate temporary containers, exercise native CFITSIO compression and
checksums, and use published SHA test vectors. Broader capture compatibility and
Windows/Linux execution remain required before enabling this validator as a
release monitoring gate. Existing loading APIs are unchanged.

The optional [`astro-bench` runner](../astro-bench/README.md) generates bounded
synthetic validator/I/O workloads with serial/fixed-concurrency measurements.
See the [initial baseline](../docs/benchmarks/2026-09-06-m4-max/README.md) before
the managed-reservation and streaming changes. The subsequent
[completion investigation](../docs/benchmarks/2026-09-07-m4-max-completion/README.md)
records extended GZIP scaling, profiles, cancellation, a responsiveness proxy and
real captures on three supplied volumes, including 714 MiB ordinary FITS files.
The [managed compressed-FITS measurements](../docs/benchmarks/2026-09-08-managed-fits/README.md)
add Rice/PLIO/HCOMPRESS scaling and tile-memory evidence plus cooperative shutdown
probes. See [release verification](../RELEASING.md) for current platform CI scope.
These measurements do not establish release defaults or a hard process-memory ceiling.

Format references: [FITS 4.0](https://fits.gsfc.nasa.gov/standard40/fits_standard40aa.pdf),
[XISF 1.0](https://www.pixinsight.com/doc/docs/XISF-1.0-spec/XISF-1.0-spec.html),
and [CFITSIO compression](https://heasarc.gsfc.nasa.gov/docs/software/fitsio/c/c_user/node41.html).
