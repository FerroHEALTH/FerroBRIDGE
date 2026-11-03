<!-- This file describes vendored third-party material; the bytes beside it
     keep their upstream licence, not the licence of this repository. -->

# Provenance: the FHIRconnect mapping library

Every YAML mapping file, the upstream licence and the templates a test reads,
vendored verbatim by
`scripts/vendor/fhirconnect-mapping-lib.sh`
(.claude/rules/vendored-inputs.md). Never edit a file here: change the pin in
docs/VERSIONS.md and re-run the script.

- Source: <https://github.com/SevKohler/FHIRconnect-mapping-lib>
- Pin: commit `2ba7cf5dc9c37d135e06571ae0a8e58b702b3471`
- Fetched: 2026-11-03
- Upstream licence: Apache License 2.0, the repository's `LICENSE` file,
  vendored beside this file
- Layout: the upstream paths, unchanged; the example resources, the
  third-party IG packages, the templates no test reads and the two documents
  saved from HL7 Confluence (no stated licence) are left out
- Files: 132, of which 129 are YAML mapping files
- Tree digest (sha256 over the sorted per-file `sha256  path` listing,
  `PROVENANCE.md` excluded): `91f97fe012e249d7f8e66f48663f2e48f0ca1f86664b1c341a33c2632112f22f`

This corpus is evidence, never an oracle. The FHIRconnect specification decides
what a mapping file may contain; these files show what published mappings look
like, including where they disagree with the published schema
(docs/architecture.md section 4.8).
