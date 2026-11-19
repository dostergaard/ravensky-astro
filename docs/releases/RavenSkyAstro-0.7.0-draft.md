# RavenSky Astro 0.7.0 release notes — draft

`astro-io`, `astro-metadata`, `astro-metrics`, and `ravensky-astro` are proposed
for a coordinated 0.7.0 release. These notes are a publication candidate, not
an announcement that the crates are available. `astro-bench` remains a local
benchmark tool and is not published.

## New library APIs

- `astro_io::typed` inventories FITS and XISF containers, reads UInt16 image
  chunks, writes the qualified monochrome UInt16 subset, reports codec
  capability, and compares exact UInt16 pixel values after readback.
- `astro_metadata::fidelity` classifies metadata impact, projects supported
  native records, compares semantic metadata readback, and prepares truthful
  provenance fields when supplied with preliminary verification evidence.
- Existing `astro-metrics` and facade APIs remain available. Their 0.7.0
  packages select the matching 0.7.0 shared-crate graph.

The FITS backend uses CFITSIO. The XISF writer follows the supported subset of
the [official XISF 1.0 Revision 2, document 1.02 (October 2026)](https://pixinsight.com/doc/docs/XISF-1.0-spec/XISF-1.0-spec.html).
The official specification is linked for reference and is not distributed in
the crates.

## Capability boundary

Reading, validating, inventorying, extracting metadata, writing, compressing,
comparing pixels, and verifying a cross-format conversion are separate
operations. The new exact writer/reader path handles two-dimensional
monochrome UInt16 images under its declared limits. It does not establish a
bit-exact Float32 or Float64 writer, support every FITS extension or XISF
property, or guarantee that all source metadata maps to another format.
Callers must examine the impact ledger, explicitly decide any allowable
metadata loss, and verify the final file. The crate APIs do not approve a loss
or make an application transaction atomic.

XISF equal-length compressed subblocks are interpreted as raw according to
Revision 2. Older nonconformant ambiguous encodings may now be rejected.
Existing FITS/XISF read-only APIs remain available.

## Upgrading

Use matching 0.7.0 versions for direct dependencies on `astro-io`,
`astro-metadata`, `astro-metrics`, and `ravensky-astro`. The release uses the
Rust 1.94 minimum and preserves MIT OR Apache-2.0 licensing. Applications
using the typed writer should review the exact UInt16 and metadata capability
contracts before enabling conversion paths. No registry release occurs until
the publication checkpoint is approved.
