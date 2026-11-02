# RavenSky Astro 0.6.2 release plan

## Objective
Publish the compressed-FITS metadata fix as a patch release of the four public
RavenSky Astro crates, with a matching GitHub tag and release.

## Constraints and current state
`0.6.1` is the latest crates.io version of `astro-io`, `astro-metadata`,
`astro-metrics`, and `ravensky-astro`; no GitHub `v0.6.2` release exists.
`astro-bench` stays unpublished. AstroMuninn remains at the published `0.6.1`
dependencies until its separately requested second release step.

## Design and affected areas
Bump the workspace version and all local public-crate dependency requirements
to `0.6.2`. Update installation examples and the changelog. Existing library
behavior and APIs are unchanged beyond the already merged compressed-FITS fix.
Publish in dependency order: `astro-io`, `astro-metadata`, `astro-metrics`, then
`ravensky-astro`. Package/CI checks precede publication. GitHub release uses the
tagged source and changelog notes.

## Execution and verification
1. Update version, lockfile, changelog, and installation documentation.
2. Run formatting, Clippy, tests including the real compressed-FITS fixture, release
   examples, docs, and package checks for the four public crates.
3. Commit and push source. Confirm the GitHub CI checks for that commit.
4. Create and push annotated `v0.6.2`, then publish the GitHub release.
5. Publish crates to crates.io in dependency order and verify registry versions.

## Risks and open questions
Registry publication is irreversible. Stop on any failed package/CI check rather
than publishing a partial, unvalidated release. The GitHub release may briefly
precede crates.io availability, matching the requested order. A standalone
AstroMuninn build will require an explicit dependency update in step two.
