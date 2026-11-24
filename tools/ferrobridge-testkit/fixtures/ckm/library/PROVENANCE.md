<!-- This file describes vendored third-party material; the bytes beside it
     keep their upstream licence, not the licence of this repository. -->

# Provenance: the CKM archetypes of the FHIRconnect mapping library

18 published archetypes of the openEHR Clinical Knowledge Manager,
vendored verbatim by `scripts/vendor/ckm-library.sh`
(.claude/rules/vendored-inputs.md). Never edit a file here: change the pin in
docs/VERSIONS.md and re-run the script.

- Source: <https://github.com/openEHR/CKM-mirror>, the mirror of the international CKM
- Pin: commit `c3fdc43dcf0eb7ef0cfe60f548fcdebc24c70094`
- Tree digest (sha256 over the sorted per-file `sha256  path` listing,
  `PROVENANCE.md` excluded): `14aec82252e7c236f1c2c93469cc8969c0c254c7e8d4b61771a0833a8fd7f2bf`
- Fetched: 2026-11-23
- Upstream licence: Creative Commons Attribution-ShareAlike 3.0 Unported, the
  mirror's `LICENSE` file at the same commit, vendored beside this file
- Use: the archetypes the FHIRconnect mapping library's model mappings
  declare, which the templates FerroBRIDGE authors under
  `tools/ferrobridge-testkit/fixtures/adl2/library/` specialise or fill
  their slots with so each mapping compiles in full (#1546), read by the
  FerroBRIDGE test suites and never shipped in a crate (the testkit crate is
  `publish = false`)

Files:

- `openEHR-EHR-COMPOSITION.report.v1.adl` from `local/archetypes/composition/openEHR-EHR-COMPOSITION.report.v1.adl`, sha256 `a9949448555587d2adff94927e40818954d62cefdb13ea2ed773c489917d46dc`
- `openEHR-EHR-COMPOSITION.report-result.v1.adl` from `local/archetypes/composition/openEHR-EHR-COMPOSITION.report-result.v1.adl`, sha256 `c68e587256f0cb46022f5a4ccc5c74099d41a75d143a20824dc1007b19aaddf2`
- `openEHR-EHR-COMPOSITION.medication_list.v1.adl` from `local/archetypes/composition/openEHR-EHR-COMPOSITION.medication_list.v1.adl`, sha256 `99b8d1aa4ccb10618f7d46fd1f8c572190f908b7188f9fc08591d096729252aa`
- `openEHR-EHR-EVALUATION.absence.v2.adl` from `local/archetypes/entry/evaluation/openEHR-EHR-EVALUATION.absence.v2.adl`, sha256 `5cb8a48ad69a9108679e46211a73a0b625e2b904eae5d5e27a559541f2b978cf`
- `openEHR-EHR-EVALUATION.exclusion_global.v1.adl` from `local/archetypes/entry/evaluation/openEHR-EHR-EVALUATION.exclusion_global.v1.adl`, sha256 `1acceb83c8b962abc10bb777eef32eb5bf7386a67890cbbfa19d10b988fd70e8`
- `openEHR-EHR-EVALUATION.gender.v1.adl` from `local/archetypes/entry/evaluation/openEHR-EHR-EVALUATION.gender.v1.adl`, sha256 `00fe97854efddf97374753b04494551a29a034eba78b1c13b1591752bbc5f5f9`
- `openEHR-EHR-EVALUATION.adverse_reaction_risk.v2.adl` from `local/archetypes/entry/evaluation/openEHR-EHR-EVALUATION.adverse_reaction_risk.v2.adl`, sha256 `661df61e13bd0f4ecfacd62349c175519c25a4c1ffd388f8ed81aa77249c1a1c`
- `openEHR-EHR-EVALUATION.device_summary.v0.adl` from `local/archetypes/entry/evaluation/openEHR-EHR-EVALUATION.device_summary.v0.adl`, sha256 `c3acc311a00cdf34db6fc43bff23456732001ea6671118b35169518a5bdab646`
- `openEHR-EHR-CLUSTER.device.v1.adl` from `local/archetypes/cluster/openEHR-EHR-CLUSTER.device.v1.adl`, sha256 `0da92215f0d8603a36ed778143c1b767e2993739c6b31e2da91497589ca12efa`
- `openEHR-EHR-CLUSTER.case_identification.v0.adl` from `local/archetypes/cluster/openEHR-EHR-CLUSTER.case_identification.v0.adl`, sha256 `57a81376648a098b70bfd500e4db3f62d1acd2c48d52535004fde8f1aea55121`
- `openEHR-EHR-CLUSTER.adverse_reaction_event.v1.adl` from `local/archetypes/cluster/openEHR-EHR-CLUSTER.adverse_reaction_event.v1.adl`, sha256 `e7b4aaa58db2c298e335edc99a5d08919848bdd1d2267b5901a074f70ad0f33d`
- `openEHR-EHR-CLUSTER.clinical_evidence.v1.adl` from `local/archetypes/cluster/openEHR-EHR-CLUSTER.clinical_evidence.v1.adl`, sha256 `dd63ccfdf05f3631c582276755ebf1f2ca083fa6c3d11419dad5a58d7594a23f`
- `openEHR-EHR-CLUSTER.organisation.v1.adl` from `local/archetypes/cluster/openEHR-EHR-CLUSTER.organisation.v1.adl`, sha256 `ed947e4085b72940e10aab3bc19d68ff8e677bd9494667a3732e4966db8cab8e`
- `openEHR-EHR-OBSERVATION.medication_statement.v0.adl` from `local/archetypes/entry/observation/openEHR-EHR-OBSERVATION.medication_statement.v0.adl`, sha256 `f02f5f3f126f1cfd2a888889056459d5f07292a3811b77dc6afd39c0d0b611cc`
- `openEHR-EHR-CLUSTER.timing_nondaily.v1.adl` from `local/archetypes/cluster/openEHR-EHR-CLUSTER.timing_nondaily.v1.adl`, sha256 `a5631cb0beffa4a1383c4923a16be9dab36782936b3300d0651590d24cf58cb8`
- `openEHR-EHR-CLUSTER.person.v1.adl` from `local/archetypes/cluster/openEHR-EHR-CLUSTER.person.v1.adl`, sha256 `69cc7845b3f4491102d32750c884bdda2593c42c869c14afeed17cd2e5c3b056`
- `openEHR-EHR-CLUSTER.address.v1.adl` from `local/archetypes/cluster/openEHR-EHR-CLUSTER.address.v1.adl`, sha256 `f3b7a6f8a96f98be8ea7f09e47359ec14d7b35fce07ed78225aa26738ce27bcc`
- `openEHR-EHR-CLUSTER.structured_name.v1.adl` from `local/archetypes/cluster/openEHR-EHR-CLUSTER.structured_name.v1.adl`, sha256 `9fcae353fa335726679dd8374a7fd5b5916d3f266ce41d3bea315db75e7e80d2`
