<!-- This file describes vendored third-party material; the bytes beside it
     keep their upstream licence, not the licence of this repository. -->

# Provenance: the KDS_Medikationseintrag operational template

One file, vendored verbatim by `scripts/vendor/kds-medikationseintrag-opt.sh`
(.claude/rules/vendored-inputs.md). Never edit a file here: change the pin in
docs/VERSIONS.md and re-run the script.

- Source: <https://github.com/openFHIR/openfhir>
- Pin: commit `5e4d68007518fcddae1907b333a86f495791aa53`, path `core/src/test/resources/kds/medikationseintrag/KDS_Medikationseintrag.opt`
- File: `KDS_Medikationseintrag.opt`, 565774 bytes, sha256 `fa042b4176db62e010ac4ab5d5cc741b3d797e64b6239f398e782ab6926d3876`
- Fetched: 2026-10-20
- Upstream licence: Apache License 2.0, the repository's `LICENSE` file at the
  same commit, vendored beside this file. The template itself carries an empty
  copyright detail and no licence detail, so its content is used on the terms
  the repository above redistributes it under.
- Use: a test fixture for the FHIR round trip, read by the FerroBRIDGE test
  suites and never shipped in a crate (the testkit crate is `publish = false`).
