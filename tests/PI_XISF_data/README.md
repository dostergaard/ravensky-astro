# PixInsight XISF interoperability corpus

This directory contains a local real-writer XISF corpus used to verify RavenSky's XISF reading, metadata extraction, and conformance-validation behavior against files produced by PixInsight.

The binary `.xisf` files are intentionally **not stored in Git** because the complete local corpus is approximately 822 MiB (862 MB). The accompanying manifest, diagnostic header/XML dumps, and this README are intended to preserve enough provenance and structural information for the corpus to remain useful and reproducible.

## Purpose

The corpus supplements RavenSky's synthetic XISF conformance fixtures with files produced by real PixInsight versions.

It is intended to answer questions such as:

- Can RavenSky validate XISF files written by supported PixInsight releases?
- Can the compatibility loader read the mono `UInt16` subset it claims to support?
- Does compressed real-writer data exercise the same `zstd+sh` behavior covered by synthetic tests?
- What XISF XML, storage descriptors, metadata, and optional structures did PixInsight actually write?
- Do adjacent PixInsight releases differ in ways relevant to RavenSky interoperability?

This is an interoperability corpus, not a complete XISF specification test suite. Synthetic tests remain responsible for edge cases that are difficult or impossible to obtain from ordinary writer output.

## Writer versions

The corpus currently contains files written by:

- **PixInsight Core 1.9.4 Lockhart**
  - ARM64
  - build 1695
  - 2026-06-21
- **PixInsight 1.9.5 Lockhart**

Each version was used to save the same three source-image classes under compressed and uncompressed conditions.

## Corpus matrix

| Writer | Source | Type | Storage |
| --- | --- | --- | --- |
| PixInsight 1.9.4 | Large mono | UInt16, 1 channel | Uncompressed |
| PixInsight 1.9.4 | Large mono | UInt16, 1 channel | Zstandard + Shuffle |
| PixInsight 1.9.4 | Small mono | UInt16, 1 channel | Uncompressed |
| PixInsight 1.9.4 | Small mono | UInt16, 1 channel | Zstandard + Shuffle |
| PixInsight 1.9.4 | Small OSC | UInt16, 3 channels | Uncompressed |
| PixInsight 1.9.4 | Small OSC | UInt16, 3 channels | Zstandard + Shuffle |
| PixInsight 1.9.5 | Large mono | UInt16, 1 channel | Uncompressed |
| PixInsight 1.9.5 | Large mono | UInt16, 1 channel | Zstandard + Shuffle |
| PixInsight 1.9.5 | Small mono | UInt16, 1 channel | Uncompressed |
| PixInsight 1.9.5 | Small mono | UInt16, 1 channel | Zstandard + Shuffle |
| PixInsight 1.9.5 | Small OSC | UInt16, 3 channels | Uncompressed |
| PixInsight 1.9.5 | Small OSC | UInt16, 3 channels | Zstandard + Shuffle |

The corresponding files use names of the form:

```text
pi_1.9.4_large_mono_u16_compressed.xisf
pi_1.9.4_large_mono_u16_uncompressed.xisf
pi_1.9.4_small_mono_u16_compressed.xisf
pi_1.9.4_small_mono_u16_uncompressed.xisf
pi_1.9.4_small_osc_compressed.xisf
pi_1.9.4_small_osc_uncompressed.xisf

pi_1.9.5_large_mono_u16_compressed.xisf
pi_1.9.5_large_mono_u16_uncompressed.xisf
pi_1.9.5_small_mono_u16_compressed.xisf
pi_1.9.5_small_mono_u16_uncompressed.xisf
pi_1.9.5_small_osc_compressed.xisf
pi_1.9.5_small_osc_uncompressed.xisf
```

`PI-xisf-samples.csv` is the machine-readable manifest for the corpus.

## Save settings and provenance

The compressed files were saved from PixInsight with:

- compression enabled;
- **Zstandard** selected as the compression method;
- **Shuffle enabled**.

Shuffle is checked by default in the PixInsight XISF save-options dialog. For these `UInt16` images, both tested PixInsight versions wrote the main compressed image block as:

```text
zstd+sh:<decoded-size>:2
```

where the shuffle item width of `2` corresponds to the two-byte `UInt16` samples.

The uncompressed files were saved with XISF image compression disabled.

### Important limitation

Not every PixInsight Save As option was recorded when the corpus was originally created.

Some PixInsight save options are sticky between saves and others are not. In addition, the two PixInsight versions were run separately. Consequently, differences involving items such as:

- checksum selection;
- thumbnail generation;
- image identifiers;
- other optional save-state values

must **not** automatically be interpreted as differences between PixInsight 1.9.4 and 1.9.5.

PixInsight image IDs are assigned manually in the application and are stored in the XISF file. Differences in image IDs in this corpus therefore have no writer-version significance.

The corpus should be used primarily for structural and interoperability observations that can be verified directly from the files.

## Observed writer behavior

The current corpus establishes several useful real-writer facts.

### Compression

Both PixInsight 1.9.4 and 1.9.5 produced `zstd+sh` image storage when the files were saved with Zstandard compression and Shuffle enabled.

For the three source classes, the compressed descriptors observed were consistent with the geometry-derived uncompressed byte counts and an item size of two bytes.

### Byte order

The inspected image elements omit the `byteOrder` attribute.

These files therefore exercise the XISF-defined omitted-byte-order behavior rather than an explicitly declared byte order.

RavenSky's explicit big-endian support continues to be exercised by synthetic conformance fixtures; this corpus does not provide a writer-produced big-endian example.

### Subblocks

No `subblocks` attribute has been observed in this corpus.

RavenSky's subblock parsing, accounting, exact-frame enforcement, and whole-block unshuffle behavior therefore remain covered primarily by synthetic conformance tests.

### OSC images

The OSC samples are three-channel RGB XISF images.

They are useful real-writer validation and metadata fixtures even though RavenSky's current `load_xisf()` compatibility API intentionally supports only single-channel `UInt16` image decoding.

## Expected RavenSky behavior

The expected behavior for this corpus is:

| Fixture class | XISF validation | `load_xisf()` |
| --- | --- | --- |
| Mono UInt16, uncompressed | Pass | Pass |
| Mono UInt16, `zstd+sh` | Pass | Pass |
| OSC/RGB UInt16, uncompressed | Pass | Unsupported by compatibility loader |
| OSC/RGB UInt16, `zstd+sh` | Pass | Unsupported by compatibility loader |

A failure outside these expectations may indicate:

- an interoperability regression;
- a change in PixInsight writer behavior;
- a RavenSky conformance defect; or
- a corpus-provenance/settings issue that should be investigated before changing production code.

## Diagnostic `.headers` files

The text `.headers` files are generated with RavenSky's `dump_xisf_metadata` example.

They contain:

1. the existing normalized semantic metadata;
2. a raw XISF format summary using RavenSky's shared XISF inspection path; and
3. the complete XISF XML header, pretty-printed for human inspection.

The XML dump is especially useful because it exposes writer-produced elements and attributes that RavenSky may not currently model semantically.

Pretty printing changes formatting whitespace but is intended to preserve the XML elements, attributes, namespaces, comments, text, ordering, and other content.

The `.headers` files can be retained in Git even though the corresponding binary XISF files are ignored.

## Regenerating diagnostic dumps

From the `ravensky-astro` workspace root:

```sh
for f in tests/PI_XISF_data/*.xisf; do
    cargo run -p ravensky-astro --example dump_xisf_metadata -- "$f" \
        > "${f}.headers"
done
```

The diagnostic utility uses RavenSky's shared XISF envelope and structural parsing path. It does not maintain an independent semantic XISF parser.

## Binary corpus storage

The `.xisf` files are excluded from Git because of their size.

The repository should track the supporting material needed to identify and understand the local corpus, including:

- this `README.md`;
- `PI-xisf-samples.csv`;
- generated `.headers` files;
- future hashes or other provenance records.

Do not add the large `.xisf` binaries to normal Git history.

## Relationship to XISF Revision 1 work

This corpus was created after RavenSky's XISF 1.0 Revision 1 Stage 1–3 implementation work.

It provides post-implementation real-writer interoperability evidence, particularly for compressed `zstd+sh` image data from PixInsight 1.9.4 and 1.9.5.

It does not replace the synthetic Revision 1 conformance suite. The synthetic tests remain necessary for cases not represented here, including:

- explicit big-endian images;
- subblocks;
- malformed or concatenated Zstandard frames;
- skippable-frame rejection;
- exact decoded-length failures;
- foreign extension placement;
- overflow and resource-limit behavior;
- intentionally malformed XML or storage descriptors.

Future writer-produced fixtures can be added when they provide coverage that is not already represented by this corpus.
