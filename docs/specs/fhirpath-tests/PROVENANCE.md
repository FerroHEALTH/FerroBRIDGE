<!-- This file describes vendored third-party material; the bytes beside it
     keep their upstream licence, not the licence of this repository. -->

# Provenance: the HL7 FHIRPath test suite for FHIR R4

The suite and the input resources its tests name, vendored verbatim by
`scripts/vendor/fhirpath-tests.sh` (.claude/rules/vendored-inputs.md). Never
edit a file here: change the pin in docs/VERSIONS.md and re-run the script.

- Source: <https://github.com/FHIR/fhir-test-cases>
- Pin: commit `4435b9d0815233e8bd9836d85bbb6cc522c19946`
- Fetched: 2026-11-03
- Upstream licence: Apache License 2.0, the repository's `LICENSE.txt` file at the
  same commit, vendored beside this file
- Layout: the upstream paths, unchanged: `r4/fhirpath/tests-fhir-r4.xml` with its 1193 tests, and
  the 15 input resources it names under `r4/`
- Files: 17
- Tree digest (sha256 over the sorted per-file `sha256  path` listing,
  `PROVENANCE.md` excluded): `1e8ec7fa00ed06f72971214c782c7c6d1a7384e804ad22b696fa23c162d8e8a2`
- Use: the `fhirpath-r4` conformance corpus of the `ferrobridge-fhirpath`
  crate, read by its test suite and never shipped in the crate.
