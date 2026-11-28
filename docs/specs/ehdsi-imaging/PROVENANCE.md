<!-- This file describes vendored third-party material; the bytes beside it
     keep their upstream licence, not the licence of this repository. -->

# Provenance: the MyHealth@EU Imaging FHIR IG myhealth.eu.fhir.imaging

Vendored by `scripts/vendor/eehrxf.sh`, each artefact pinned by its
URL and the sha256 of its bytes. Never edit a file here: change the pins in
the script and the pin-set digest in docs/VERSIONS.md, and re-run the script.

- Pin-set digest (sha256 over the sorted `mode  file  url  sha256` lines of
  the pins): `857b9eda8e46fc3145d5c4e3907803566c226c00f11a977d78b5d67920472a4e`
- Fetched: 2026-11-28, with the User-Agent `ferrobridge-vendor (scripts/vendor)`
- Artefacts: 1 committed, 0 cache only, 0 needing
  manual retrieval
- Files in this directory: 1 besides this one, each verbatim as the
  publisher serves it
- Tree digest (sha256 over the sorted per-file `sha256  path` listing,
  `PROVENANCE.md` excluded): `cc8a944eff6d1ccddba9cd59450634aa731f0236ee846d408344acf84c27090d`
- Read by: #2403 (the MyHealth@EU binding test,
  crates/eehrxf/tests/it/terminology/myhealth_bindings.rs)

The MyHealth@EU imaging report and study on FHIR in its 9.1.1 CI build,
canonical `http://fhir.ehdsi.eu/imaging`, committed under CC0-1.0, whose
required bindings the translation binding sets are held to.

## Artefacts

### `myhealth.eu.fhir.imaging-9.1.1.tgz` (committed)

- Source: <https://build-fhir.ehdsi.eu/imaging/package.tgz>
- Version: 9.1.1, a CI build (FHIR 4.0.1, built 2026-05-05)
- sha256: `286eebc30bc4be9c95b2478152e1dce95a6f23bc31f42f4587fd846ae04a982d`
- Licence: CC0-1.0: the license of the package manifest and of the ImplementationGuide resource
- Note: The registry tarball as served, package manifest included; the archive is committed whole so its sha256 stays the pin. A CI build that build-fhir.ehdsi.eu overwrites in place, so the sha256 is the pin. Its dependency hl7.fhir.eu.imaging 1.0.0-alpha is published nowhere (#2371), so it stays outside the profile validator's package set.
