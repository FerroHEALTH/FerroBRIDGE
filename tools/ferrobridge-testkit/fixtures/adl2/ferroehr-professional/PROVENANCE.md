<!-- This file describes vendored third-party material; the bytes beside it
     keep their upstream licence, not the licence of this repository. -->

# Provenance: the FerroEHR professional PERSON and ROLE definitions

Two ADL 2 archetypes, vendored verbatim by `scripts/vendor/ferroehr-professional.sh`
(.claude/rules/vendored-inputs.md), with the provenance their publisher keeps
beside them as `UPSTREAM-PROVENANCE.md`. Never edit a file here: change the
pins in docs/VERSIONS.md and re-run the script.

- Source: <https://github.com/FerroHEALTH/FerroEHR>, the definitions the reference CDR loads
  before it serves
- Pin: commit `7d85f0a1d2b86ca5bf2d43025090e6a102d99f74` (FerroEHR v5.7.0)
- File: `openEHR-DEMOGRAPHIC-PERSON.healthcare_professional.v1.0.0.adls`, 4403 bytes, sha256 `22e0004006afb36e903841afecaa2541c7624f3f1d6e03dfeb5c9354ee370c7f`, path `deploy/definitions/professional/openEHR-DEMOGRAPHIC-PERSON.healthcare_professional.v1.0.0.adls`
- File: `openEHR-DEMOGRAPHIC-ROLE.healthcare_professional.v1.0.0.adls`, 2067 bytes, sha256 `a9cb003e2238dd4ba8d69b5184fc917102b538b198344d9507f303215b9a3fec`, path `deploy/definitions/professional/openEHR-DEMOGRAPHIC-ROLE.healthcare_professional.v1.0.0.adls`
- Fetched: 2026-11-22
- Licence: each archetype states its own in `other_details["licence"]`, the
  Creative Commons Attribution-ShareAlike 3.0 License, as an adaptation of the
  openEHR CKM archetypes its `derived_from` entry and `UPSTREAM-PROVENANCE.md`
  name (copyright openEHR Foundation).
- Use: test fixtures for a party context over an id-coded ADL 2 template and
  for the party binding of the reference CDR, read by the FerroBRIDGE test
  suites and never shipped in a crate (the testkit crate is
  `publish = false`).
