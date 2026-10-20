<!-- This file describes vendored third-party material; the bytes beside it
     keep their upstream licence, not the licence of this repository. -->

# Provenance: the KDS_Fall_einfach operational template

One file, vendored verbatim by `scripts/vendor/kds-fall-einfach-opt.sh`
(.claude/rules/vendored-inputs.md). Never edit a file here: change the pin in
docs/VERSIONS.md and re-run the script.

- Source: <https://github.com/openFHIR/openfhir>
- Pin: commit `5e4d68007518fcddae1907b333a86f495791aa53`, path `core/src/test/resources/kds/fall/KDS_Fall_einfach.opt`
- File: `KDS_Fall_einfach.opt`, 349420 bytes, sha256 `db9c34415b699e5e7fae8e57f41c4ef063d0f0b9bdba636538e5397012a115c5`
- Fetched: 2026-10-20
- Upstream licence: Apache License 2.0, the repository's `LICENSE` file at the
  same commit, vendored beside this file. The template itself carries an empty
  licence detail, so its content is used on the terms the repository above
  redistributes it under.
- Use: a test fixture for the FHIR round trip, read by the FerroBRIDGE test
  suites and never shipped in a crate (the testkit crate is `publish = false`).
