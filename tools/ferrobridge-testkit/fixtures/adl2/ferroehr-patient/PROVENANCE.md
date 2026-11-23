<!-- This file describes vendored third-party material; the bytes beside it
     keep their upstream licence, not the licence of this repository. -->

# Provenance: the FerroEHR patient PERSON definition

One ADL 2 archetype, vendored verbatim by `scripts/vendor/ferroehr-patient.sh`
(.claude/rules/vendored-inputs.md), with the provenance its publisher keeps
beside it as `UPSTREAM-PROVENANCE.md`. Never edit a file here: change the
pin in docs/VERSIONS.md and re-run the script.

- Source: <https://github.com/FerroHEALTH/FerroEHR>, the definition the reference CDR loads
  before it serves
- Pin: commit `9ecc070f1801effc73a92d21b8096df3ed154b04` (FerroEHR main, ahead of v6.0.0)
- File: `openEHR-DEMOGRAPHIC-PERSON.patient.v1.0.0.adls`, 18005 bytes, sha256 `803899d103437837a955089b7fd5b7333522d35e80a0055cd3c1db9a7a35ae92`, path `deploy/definitions/patient/openEHR-DEMOGRAPHIC-PERSON.patient.v1.0.0.adls`
- Fetched: 2026-11-23
- Licence: the archetype states its own in `other_details["licence"]`, the
  Creative Commons Attribution-ShareAlike 3.0 License, as an adaptation of the
  openEHR CKM archetypes its `derived_from` entry and `UPSTREAM-PROVENANCE.md`
  name (copyright openEHR Foundation).
- Use: the test fixture of the Patient party context the patient summary
  reads its patient through, read by the FerroBRIDGE test suites and never
  shipped in a crate (the testkit crate is `publish = false`).
