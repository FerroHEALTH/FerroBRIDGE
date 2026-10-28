<!-- This file describes vendored third-party material; the bytes beside it
     keep their upstream licence, not the licence of this repository. -->

# Provenance: the HL7 France FR Core guide hl7.fhir.fr.core

Vendored by `scripts/vendor/eehrxf.sh`, each artefact pinned by its
URL and the sha256 of its bytes. Never edit a file here: change the pins in
the script and the pin-set digest in docs/VERSIONS.md, and re-run the script.

- Pin-set digest (sha256 over the sorted `mode  file  url  sha256` lines of
  the pins): `a2806a8c83c2b0b85a4b8c2cfd12effc5d37c3c2c0c2670406b840532698510a`
- Fetched: 2026-10-28, with the User-Agent `ferrobridge-vendor (scripts/vendor)`
- Artefacts: 1 committed, 0 cache only, 0 needing
  manual retrieval
- Files in this directory: 1 besides this one, each verbatim as the
  publisher serves it
- Tree digest (sha256 over the sorted per-file `sha256  path` listing,
  `PROVENANCE.md` excluded): `ae3dd4492d86a3ea980021552a9045b81ccc7482e0a2c6348d9a7623d4caa857`
- Read by: #1314 (the eehrxf tests, crates/eehrxf/tests/it/, which read
  the exchange-format proxies the component is designed against)

FR Core 2.2.0 as Simplifier serves it, whose value sets include SNOMED CT
at an edition and release, committed under CC0-1.0.

## Artefacts

### `hl7.fhir.fr.core-2.2.0.tgz` (committed)

- Source: <https://packages.simplifier.net/hl7.fhir.fr.core/2.2.0>
- Version: 2.2.0
- sha256: `085e61bf579829a6d5520c6f388d763d27ba400f9f6ca548bc88da5545ba13f9`
- Licence: CC0-1.0: the license of the package manifest
- Note: The registry tarball as served, package manifest included; the archive is committed whole so its sha256 stays the pin. Its value sets include SNOMED CT at a version, the form the component's version check reads.
