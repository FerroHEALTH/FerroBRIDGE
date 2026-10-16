<!-- This file describes vendored third-party material; the bytes beside it
     keep their upstream licence, not the licence of this repository. -->

# Provenance: the KDS_Person operational template

One file, vendored verbatim by `scripts/vendor/kds-person-opt.sh`
(.claude/rules/vendored-inputs.md). Never edit a file here: change the pin in
docs/VERSIONS.md and re-run the script.

- Source: <https://github.com/openFHIR/openfhir>
- Pin: commit `5e4d68007518fcddae1907b333a86f495791aa53`, path `core/src/test/resources/kds/person/KDS_Person.opt`
- File: `KDS_Person.opt`, 245899 bytes, sha256 `ef559dde455a36b6282a6f4b2e3ecd4f2e6130acd8be638ce73088739e0bf6c8`
- Fetched: 2026-10-15
- Upstream licence: Apache License 2.0, the repository's `LICENSE` file at the
  same commit, vendored beside this file. The template itself carries an empty
  licence detail and an empty copyright detail, so its content is used on the
  terms the repository above redistributes it under.
- Use: a test fixture for the FHIR round trip, read by the FerroBRIDGE test
  suites and never shipped in a crate (the testkit crate is `publish = false`).
