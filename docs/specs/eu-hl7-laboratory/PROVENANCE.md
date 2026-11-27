<!-- This file describes vendored third-party material; the bytes beside it
     keep their upstream licence, not the licence of this repository. -->

# Provenance: the HL7 Europe Laboratory Report guide hl7.fhir.eu.laboratory

Vendored by `scripts/vendor/eehrxf.sh`, each artefact pinned by its
URL and the sha256 of its bytes. Never edit a file here: change the pins in
the script and the pin-set digest in docs/VERSIONS.md, and re-run the script.

- Pin-set digest (sha256 over the sorted `mode  file  url  sha256` lines of
  the pins): `d33d96d7d27d9ea869d3a1ce045e87482c8f383b04a81156f8ca694bcb904d27`
- Fetched: 2026-11-27, with the User-Agent `ferrobridge-vendor (scripts/vendor)`
- Artefacts: 1 committed, 0 cache only, 0 needing
  manual retrieval
- Files in this directory: 1 besides this one, each verbatim as the
  publisher serves it
- Tree digest (sha256 over the sorted per-file `sha256  path` listing,
  `PROVENANCE.md` excluded): `330bb7cf02dd473e750e74537932471468849d40930d911588c543e276775179`
- Read by: #1314 (the eehrxf tests, crates/eehrxf/tests/it/, which read
  the exchange-format proxies the component is designed against)

The European laboratory report guide, canonical
`http://hl7.eu/fhir/laboratory`, at the version the eehrxf component reads,
committed under CC0-1.0.

## Artefacts

### `hl7.fhir.eu.laboratory-2.1.0.tgz` (committed)

- Source: <https://packages.fhir.org/hl7.fhir.eu.laboratory/2.1.0>
- Version: 2.1.0 (FHIR 4.0.1, 2026-10-16)
- sha256: `87019c396f4cff934ec5230ce1f97d76b79b40591be08161e694ad661129379c`
- Licence: CC0-1.0: the license of the package manifest and of the ImplementationGuide resource
- Note: The registry tarball as served, package manifest included; the archive is committed whole so its sha256 stays the pin. The version MyHealth@EU Laboratory 9.2.0 depends on, which the component reads.
