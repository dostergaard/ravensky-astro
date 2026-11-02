# RavenSky Astro 0.6.2 handoff

The `v0.6.2` GitHub release and all four public crates are published. Release
source is `0d8c57a053cf922cbe8075fc77e82b69192f9aeb`. The changelog,
installation examples, and release notes are in that tagged source. See
`EVIDENCE.md` for tests, CI, package audit, publication order, and registry
archive hashes.

`astro-bench` remains unpublished. AstroMuninn still requests public `0.6.1`
crates, so a standalone product build does not yet include the compressed-FITS
metadata fix. The next authorized step is a separate AstroMuninn dependency
update, validation, and product release. All four hosted docs.rs API pages are
available, and local Rustdoc with denied warnings passed.
