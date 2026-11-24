<!-- This file describes vendored third-party material; the bytes beside it
     keep their upstream licence, not the licence of this repository. -->

# Provenance: the HL7 FHIRPath test suite for FHIR R4

The suite and the input resources its tests name, vendored verbatim by
`scripts/vendor/fhirpath-tests.sh` (.claude/rules/vendored-inputs.md). Never
edit a file here: change the pin in docs/VERSIONS.md and re-run the script.

- Source: <https://github.com/FHIR/fhir-test-cases>
- Pin: commit `8a68d8ec9a27da34faf24748276b5088762e3457`
- Fetched: 2026-11-24
- Upstream licence: Apache License 2.0, the repository's `LICENSE.txt` file at the
  same commit, vendored beside this file
- Layout: the upstream paths, unchanged: `r4/fhirpath/tests-fhir-r4.xml` with its 1227 tests, and
  the 16 input resources it names under `r4/`
- Files: 18
- Tree digest (sha256 over the sorted per-file `sha256  path` listing,
  `PROVENANCE.md` excluded): `d4e84006502dc8270bfd45e1da3ff1c52f63bbf9956b81c228bc188be28dbaeb`
- Use: the `fhirpath-r4` conformance corpus of the `ferrobridge-fhirpath`
  crate, read by its test suite and never shipped in the crate.
