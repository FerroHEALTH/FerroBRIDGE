<!-- This file describes vendored third-party material; the bytes beside it
     keep their upstream licence, not the licence of this repository. -->

# Provenance: the Studienteilnahme operational template

One file, vendored verbatim by `scripts/vendor/kds-studienteilnahme-opt.sh`
(.claude/rules/vendored-inputs.md). Never edit a file here: change the pin in
docs/VERSIONS.md and re-run the script.

- Source: <https://github.com/openFHIR/openfhir>
- Pin: commit `5e4d68007518fcddae1907b333a86f495791aa53`, path `core/src/test/resources/kds/studienteilnahme/Studienteilnahme.opt`
- File: `Studienteilnahme.opt`, 217847 bytes, sha256 `4e1e919e18f0f527661201819d3ed4b3ae2cab5cff88724edd70b7462d484004`
- Fetched: 2026-10-20
- Upstream licence: Apache License 2.0, the repository's `LICENSE` file at the
  same commit, vendored beside this file. The template itself states the Creative
  Commons Attribution-ShareAlike 3.0 licence with the openEHR Foundation as
  custodian organisation, which also permits redistribution with attribution.
- Use: a test fixture for the FHIR round trip, read by the FerroBRIDGE test
  suites and never shipped in a crate (the testkit crate is `publish = false`).
