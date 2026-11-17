<!-- This file describes vendored third-party material; the bytes beside it
     keep their upstream licence, not the licence of this repository. -->

# Provenance: the FerroEHR professional PERSON definition

One ADL 2 archetype, vendored verbatim by `scripts/vendor/ferroehr-professional.sh`
(.claude/rules/vendored-inputs.md), with the provenance its publisher keeps
beside it as `UPSTREAM-PROVENANCE.md`. Never edit a file here: change the pin
in docs/VERSIONS.md and re-run the script.

- Source: <https://github.com/FerroHEALTH/FerroEHR>, the definitions the reference CDR loads
  before it serves
- Pin: commit `7d85f0a1d2b86ca5bf2d43025090e6a102d99f74` (FerroEHR v5.7.0), path `deploy/definitions/professional/openEHR-DEMOGRAPHIC-PERSON.healthcare_professional.v1.0.0.adls`
- File: `openEHR-DEMOGRAPHIC-PERSON.healthcare_professional.v1.0.0.adls`, 4403 bytes, sha256 `22e0004006afb36e903841afecaa2541c7624f3f1d6e03dfeb5c9354ee370c7f`
- Fetched: 2026-11-17
- Licence: the archetype states its own in `other_details["licence"]`, the
  Creative Commons Attribution-ShareAlike 3.0 License, as an adaptation of the
  openEHR CKM archetypes its `derived_from` entry and `UPSTREAM-PROVENANCE.md`
  name (copyright openEHR Foundation).
- Use: a test fixture for a party context over an id-coded ADL 2 template,
  read by the FerroBRIDGE test suites and never shipped in a crate (the
  testkit crate is `publish = false`).
