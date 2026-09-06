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

FITS loading, header access, metadata extraction and standalone compressed-image
validation share `astro_io::fits::backend`. The linked backend's
`is_reentrant()` capability controls admission: independent handles run concurrently
on reentrant builds; other builds admit one thread with nested calls supported.
Loaders wait for admission. The validator uses nonblocking admission and can return
`ResourceBusy`, including from standalone `validate_file`; retry outside the worker.
Structural FITS and XISF validation do not acquire this native gate.

Callers using `fitsio` directly must enclose open, operations, error handling and
close/drop in `with_cfitsio` or `try_with_cfitsio`. Do not let live handles escape
the closure. Borrowed-handle helpers protect their own operations, but cannot cover
the caller's separate opens/closes. Uncoordinated direct calls and independently
linked copies remain outside the gate. See the
[control design and native allocation audit](../docs/CfitsioControlImplementation.md).

## Windows FITS Path-Length Note

On Windows, FITS file access in AstroMuninn and the ravensky-astro FITS APIs depends on CFITSIO (via `fitsio` / `fitsio-sys`). CFITSIO currently opens disk files using its `fopen`-based path handling (`file_openfile`), which in this environment follows the classic Windows path-length boundary.

Use full FITS paths shorter than 260 characters (`< 260`). At 260 or more, FITS open calls may fail.

This limitation is specific to FITS access through CFITSIO. XISF handling is not affected.

## Installation

Add to your `Cargo.toml`:

```toml
[dependencies]
astro-io = "0.5.0"
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

## File validation (unreleased)

`astro_io::validation::validate_file` checks a local file read-only, independently
of the pixel loaders and metadata normalization. Format detection uses file
bytes. `ValidationOptions::default()` selects `Structural`; use
`.with_level(ValidationLevel::Full)` for the stronger check. The module rustdoc
contains a compilable usage example.

| Coverage | Structural | Full adds |
| --- | --- | --- |
| FITS primary/image HDUs, all standard sample widths and dimensions | Mandatory header order, checked sizes, complete padded HDU chain | Reads all physical bytes |
| FITS ASCII/binary tables | Column layout and variable-array heap descriptor bounds | Reads table/heap storage |
| FITS tiled images | Tile geometry, table/heap extents, declared decoded size | CFITSIO sectional decoding of RICE, GZIP, PLIO, HCOMPRESS and uncompressed tiles |
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
means no checksum protection was available. I/O counts exclude CFITSIO rereads.

Typed failures distinguish incomplete input, invalid structure, integrity
mismatch, unsupported features, resource limits, source changes, cancellation,
and I/O errors. Applications own quiet periods, retries, and presentation.

### Bounds and limitations

Defaults: 64 MiB combined header bytes, 256 MiB working allowance per call,
100,000 structures, and 64 GiB combined declared decoded data. The decoded limit
is total data checked, not a RAM allocation or physical-file-size limit.
`ValidationLimits` exposes each nonzero limit. Working limits now include retained
buffers and conservative parser/backend allowances, so a header/native stage can
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
| CFITSIO tiles | **Shared-budget full decoding returns `Unsupported`** until native allocation limits are enforceable. Native concurrency is coordinated across loaders. Structural checks work; standalone full calls retain an estimated tile/stored-data/native allowance that does not bound tile caches or malformed GZIP expansion |

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
A blocked OS read or an individual native/codec call cannot be interrupted.
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
as whole-file gzip) return `Unsupported`. Full compressed FITS paths containing
`[` or `]` are rejected to avoid CFITSIO filter interpretation. XISF external
block locations, foreign XML element namespaces, DTDs, and external entities
are unsupported; the validator does not fetch external resources. Namespace-less
XISF headers are accepted for producer compatibility. Native access is serialized
when the linked CFITSIO reports a non-reentrant build; the Windows FITS path-length
restriction above also applies to the native compressed-image path.

Tests generate temporary containers, exercise native CFITSIO compression and
checksums, and use published SHA test vectors. Real capture compatibility and
Windows/Linux execution remain required before enabling this validator as a
release monitoring gate. Existing loading APIs are unchanged.

The optional [`astro-bench` runner](../astro-bench/README.md) generates bounded
synthetic validator/I/O workloads with serial/fixed-concurrency measurements.
See the [initial baseline](../docs/benchmarks/2026-09-06-m4-max/README.md) before
the managed-reservation and streaming changes. These measurements do
not establish release defaults or a hard process-memory ceiling.

Format references: [FITS 4.0](https://fits.gsfc.nasa.gov/standard40/fits_standard40aa.pdf),
[XISF 1.0](https://www.pixinsight.com/doc/docs/XISF-1.0-spec/XISF-1.0-spec.html),
and [CFITSIO compression](https://heasarc.gsfc.nasa.gov/docs/software/fitsio/c/c_user/node41.html).
