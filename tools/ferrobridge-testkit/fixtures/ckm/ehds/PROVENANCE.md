<!-- This file describes vendored third-party material; the bytes beside it
     keep their upstream licence, not the licence of this repository. -->

# Provenance: the CKM EHDS archetypes

8 published archetypes of the openEHR Clinical Knowledge Manager,
vendored verbatim by `scripts/vendor/ckm-ehds.sh`
(.claude/rules/vendored-inputs.md). Never edit a file here: change the pin in
docs/VERSIONS.md and re-run the script.

- Source: <https://github.com/openEHR/CKM-mirror>, the mirror of the international CKM
- Pin: commit `a5e27fa133e77a6d176e676d95276a58b81b15c2`
- Tree digest (sha256 over the sorted per-file `sha256  path` listing,
  `PROVENANCE.md` excluded): `68a0452fcbcb386ea81fcc9fd33af3ec7250fc257d3cec521d3189e502349f9c`
- Fetched: 2026-11-25
- Upstream licence: Creative Commons Attribution-ShareAlike 3.0 Unported, the
  mirror's `LICENSE` file at the same commit, vendored beside this file
- Use: the entries FerroBRIDGE's EHDS record template holds for the draft
  EEHRxF Annex X data groups, read by the FerroBRIDGE test suites and never
  shipped in a crate (the testkit crate is `publish = false`)

Files:

- `openEHR-EHR-OBSERVATION.body_weight.v2.adl` from `local/archetypes/entry/observation/openEHR-EHR-OBSERVATION.body_weight.v2.adl`, sha256 `0f61613a6f79125dfb60acade4cafa1342af5c1fd4893178555f9a9dea4b9958`
- `openEHR-EHR-EVALUATION.precaution.v1.adl` from `local/archetypes/entry/evaluation/openEHR-EHR-EVALUATION.precaution.v1.adl`, sha256 `24c509a51c58fb9c20ccb3386234aa43ee723f198984d3e7044e1ca89e0d4815`
- `openEHR-EHR-EVALUATION.advance_care_directive.v2.adl` from `local/archetypes/entry/evaluation/openEHR-EHR-EVALUATION.advance_care_directive.v2.adl`, sha256 `2b24711ad7132d3e61a41437546c625f4de612e2c5af6db5357f84bfab2d12b7`
- `openEHR-EHR-CLUSTER.media_file.v1.adl` from `local/archetypes/cluster/openEHR-EHR-CLUSTER.media_file.v1.adl`, sha256 `284e6b1d6421d926e213385529c7eea9d22004f57cb064930f0a5f2ccdb24234`
- `openEHR-EHR-INSTRUCTION.care_plan_request.v0.adl` from `local/archetypes/entry/instruction/openEHR-EHR-INSTRUCTION.care_plan_request.v0.adl`, sha256 `4d597bb7dfb96e0ba2129d5da3197b412b72d932f76d25e275ee04c7be8ab64e`
- `openEHR-EHR-OBSERVATION.pregnancy_assertion.v0.adl` from `local/archetypes/entry/observation/openEHR-EHR-OBSERVATION.pregnancy_assertion.v0.adl`, sha256 `91305289e90168c14a218e924dbe2c7fe4e7aa4513c253f49e42bb98ac4a6918`
- `openEHR-EHR-EVALUATION.obstetric_summary.v1.adl` from `local/archetypes/entry/evaluation/openEHR-EHR-EVALUATION.obstetric_summary.v1.adl`, sha256 `e0d9819eff16fa60c399ed1436cca3f1e74b2b275096434f99db4eab11d86328`
- `openEHR-EHR-ADMIN_ENTRY.travel_event.v0.adl` from `local/archetypes/entry/admin_entry/openEHR-EHR-ADMIN_ENTRY.travel_event.v0.adl`, sha256 `26ab0bf58a5795f722abedf6b0d4949b1df2b5dcb5918768c47363a9dd17522b`
