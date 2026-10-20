<!-- This file describes vendored third-party material; the bytes beside it
     keep their upstream licence, not the licence of this repository. -->

# Provenance: the KDS_Person_Pseudonymisiert operational template

One file, vendored verbatim by `scripts/vendor/kds-person-pseudonymisiert-opt.sh`
(.claude/rules/vendored-inputs.md). Never edit a file here: change the pin in
docs/VERSIONS.md and re-run the script.

- Source: <https://github.com/openFHIR/openfhir>
- Pin: commit `5e4d68007518fcddae1907b333a86f495791aa53`, path `core/src/test/resources/kds/person/KDS_Person_Pseudonymisiert.opt`
- File: `KDS_Person_Pseudonymisiert.opt`, 170301 bytes, sha256 `e08f61d94ca2d49ac36230a789517dbeb83ea36b5e41627a5b6ead7f11c282a0`
- Fetched: 2026-10-20
- Upstream licence: Apache License 2.0, the repository's `LICENSE` file at the
  same commit, vendored beside this file. The template itself carries an empty
  licence detail, so its content is used on the terms the repository above
  redistributes it under.
- Use: a test fixture for the FHIR round trip, read by the FerroBRIDGE test
  suites and never shipped in a crate (the testkit crate is `publish = false`).
