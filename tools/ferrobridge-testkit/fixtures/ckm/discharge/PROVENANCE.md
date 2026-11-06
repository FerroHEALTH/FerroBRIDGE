<!-- This file describes vendored third-party material; the bytes beside it
     keep their upstream licence, not the licence of this repository. -->

# Provenance: the CKM discharge archetypes

Three published archetypes of the openEHR Clinical Knowledge Manager,
vendored verbatim by `scripts/vendor/ckm-discharge.sh`
(.claude/rules/vendored-inputs.md). Never edit a file here: change the pin in
docs/VERSIONS.md and re-run the script.

- Source: <https://github.com/openEHR/CKM-mirror>, the mirror of the international CKM
- Pin: commit `c3fdc43dcf0eb7ef0cfe60f548fcdebc24c70094`
- Tree digest (sha256 over the sorted per-file `sha256  path` listing,
  `PROVENANCE.md` excluded): `d46b60528c41db623f5aada90b6a2b2d51aa494d21451626ddb56b12e2ad9930`
- Fetched: 2026-11-06
- Upstream licence: Creative Commons Attribution-ShareAlike 3.0 Unported, the
  mirror's `LICENSE` file at the same commit, vendored beside this file
- Use: the archetypes FerroBRIDGE's discharge report template specialises or
  holds in its sections, beside the CKM medication set, read by
  the FerroBRIDGE test suites and never shipped in a crate (the testkit crate
  is `publish = false`)

Files:

- `openEHR-EHR-COMPOSITION.transfer_summary.v1.adl` from `local/archetypes/composition/openEHR-EHR-COMPOSITION.transfer_summary.v1.adl`, sha256 `8cfb8ec37925d85c683ad5e2803e16c14bbb51a6d6cec606308c9f8b6a8856ad`
- `openEHR-EHR-EVALUATION.problem_diagnosis.v1.adl` from `local/archetypes/entry/evaluation/openEHR-EHR-EVALUATION.problem_diagnosis.v1.adl`, sha256 `8462a3b811a440c2b01451f0da48c7d297d53b5dc4d45c4722b13321e645ce1f`
- `openEHR-EHR-ACTION.procedure.v1.adl` from `local/archetypes/entry/action/openEHR-EHR-ACTION.procedure.v1.adl`, sha256 `a913b4ef4121bfd0e2c4fa975205fd588c01322cb1526eae128eced1bb1ddf11`
