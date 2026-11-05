<!-- This file describes vendored third-party material; the bytes beside it
     keep their upstream licence, not the licence of this repository. -->

# Provenance: the CKM archetypes of the FHIRconnect mapping library

8 published archetypes of the openEHR Clinical Knowledge Manager,
vendored verbatim by `scripts/vendor/ckm-library.sh`
(.claude/rules/vendored-inputs.md). Never edit a file here: change the pin in
docs/VERSIONS.md and re-run the script.

- Source: <https://github.com/openEHR/CKM-mirror>, the mirror of the international CKM
- Pin: commit `c3fdc43dcf0eb7ef0cfe60f548fcdebc24c70094`
- Tree digest (sha256 over the sorted per-file `sha256  path` listing,
  `PROVENANCE.md` excluded): `466dfd14575f82cc18d999c268b4cc4fe36e1635259f50dea50a352d884b437c`
- Fetched: 2026-11-05
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
- `openEHR-EHR-CLUSTER.device.v1.adl` from `local/archetypes/cluster/openEHR-EHR-CLUSTER.device.v1.adl`, sha256 `0da92215f0d8603a36ed778143c1b767e2993739c6b31e2da91497589ca12efa`
- `openEHR-EHR-CLUSTER.case_identification.v0.adl` from `local/archetypes/cluster/openEHR-EHR-CLUSTER.case_identification.v0.adl`, sha256 `57a81376648a098b70bfd500e4db3f62d1acd2c48d52535004fde8f1aea55121`
