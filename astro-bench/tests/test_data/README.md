# Supplied format fixtures

These 14 small files were supplied by the workspace owner on 2026-09-07 and
approved for inclusion in the repository. They are compatibility/error fixtures,
not a representative throughput dataset. Large personal captures remain outside
Git. File bytes are retained unchanged.

| Files | Expected current shared-budget result |
|---|---|
| `blank.fits`, `checksum.fits`, `scale.fits`, `tdim.fits`, `test0.fits`, `variable_length_table.fits` | Structural and full success |
| `checksum_false.fits` | Structural success; full checksum integrity failure |
| `compressed_image.fits`, `compressed_float_bzero.fits`, `double_ext.fits` | Structural and full success through managed Rice validation |
| `random_groups.fits` | `Unsupported` random-groups layout |
| `test.xisf`, `2ch.xisf` | `Incomplete`: each 5,466-byte file declares a thumbnail attachment at offset 8,192 with 128,000 bytes |
| `verify.fits` | `InvalidStructure` |

Unsupported layouts remain recorded coverage gaps. These files must not be
silently discarded or counted as completed full-validation throughput. Expectations
should evolve deliberately when the validator adds the currently excluded formats.
Focused integration checks are in `../supplied_files.rs`.
