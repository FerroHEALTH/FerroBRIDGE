<!-- This file describes vendored third-party material; the bytes beside it
     keep their upstream licence, not the licence of this repository. -->

# Provenance: the HL7 FHIRPath test suite for FHIR R4

The suite and the input resources its tests name, vendored verbatim by
`scripts/vendor/fhirpath-tests.sh` (.claude/rules/vendored-inputs.md). Never
edit a file here: change the pin in docs/VERSIONS.md and re-run the script.

- Source: <https://github.com/FHIR/fhir-test-cases>
- Pin: commit `9b57d9564f3f7e0343c020942155df13e56ef7a7`
- Fetched: 2026-10-15
- Upstream licence: Apache License 2.0, the repository's `LICENSE.txt` file at the
  same commit, vendored beside this file
- Layout: the upstream paths, unchanged: `r4/fhirpath/tests-fhir-r4.xml` with its 992 tests, and
  the 11 input resources it names under `r4/`
- Files: 13
- Tree digest (sha256 over the sorted per-file `sha256  path` listing,
  `PROVENANCE.md` excluded): `af9848ea9d046b288289adf64f199d4b4d0bdfebf6c000496b6e7f69a4b0dc48`
- Use: the `fhirpath-r4` conformance corpus of the `ferrobridge-fhirpath`
  crate, read by its test suite and never shipped in the crate.
