# RavenSky Astro 0.6.2

This patch fixes metadata extraction and pixel loading for CFITSIO
tiled-compressed FITS images. In N.I.N.A. `GZIP_2` captures, the primary HDU can
be empty while the compressed image and observational headers are in an image
extension. The loader and metadata parser now select that logical image; header
values from the selected HDU take precedence, with primary-header fallback when
they are absent. Logical dimensions and pixel type are reported correctly.

The `dump_fits_metadata` example shows which image HDU was selected. The new
`astro_io::fits::select_image_hdu` helper exposes the same selection to callers.
Regression tests include the supplied compressed N.I.N.A. captures and an
ordinary uncompressed primary-image control.

Published crates: `astro-io`, `astro-metadata`, `astro-metrics`, and
`ravensky-astro`, all at `0.6.2`. `astro-bench` remains unpublished.

See [CHANGELOG.md](https://github.com/dostergaard/ravensky-astro/blob/v0.6.2/CHANGELOG.md)
for the change list. AstroMuninn's
standalone builds continue to use published `0.6.1` until its dependency update
in the separate product release step.
