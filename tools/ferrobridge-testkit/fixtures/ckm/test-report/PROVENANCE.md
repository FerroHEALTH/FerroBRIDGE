<!-- This file describes vendored third-party material; the bytes beside it
     keep their upstream licence, not the licence of this repository. -->

# Provenance: the CKM medical test report archetypes

Two published archetypes of the openEHR Clinical Knowledge Manager,
vendored verbatim by `scripts/vendor/ckm-test-report.sh`
(.claude/rules/vendored-inputs.md). Never edit a file here: change the pin in
docs/VERSIONS.md and re-run the script.

- Source: <https://github.com/openEHR/CKM-mirror>, the mirror of the international CKM
- Pin: commit `c3fdc43dcf0eb7ef0cfe60f548fcdebc24c70094`
- Tree digest (sha256 over the sorted per-file `sha256  path` listing,
  `PROVENANCE.md` excluded): `3951cd552fad71638246a88648aa82c78a31e1e83071dd6116a19ceb1b95bd0b`
- Fetched: 2026-11-24
- Upstream licence: Creative Commons Attribution-ShareAlike 3.0 Unported, the
  mirror's `LICENSE` file at the same commit, vendored beside this file
- Use: the observations FerroBRIDGE's medical test report template holds, read
  by the FerroBRIDGE test suites and never shipped in a crate (the testkit crate
  is `publish = false`)

Files:

- `openEHR-EHR-OBSERVATION.ecg_result.v1.adl` from `local/archetypes/entry/observation/openEHR-EHR-OBSERVATION.ecg_result.v1.adl`, sha256 `f444e27cb6e08bc64b18f4c7376444ea1e7181074d74d193e2ffcb92adda8960`
- `openEHR-EHR-OBSERVATION.spirometry_result.v2.adl` from `local/archetypes/entry/observation/openEHR-EHR-OBSERVATION.spirometry_result.v2.adl`, sha256 `3d1261f71155fb05b1e37ba488daef4c5a43fc710d2895057f458a4ffb3c1adc`
