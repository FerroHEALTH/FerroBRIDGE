<!-- This file describes vendored third-party material; the bytes beside it
     keep their upstream licence, not the licence of this repository. -->

# Provenance: the CKM medication archetypes

Ten published archetypes of the openEHR Clinical Knowledge Manager,
vendored verbatim by `scripts/vendor/ckm-medication.sh`
(.claude/rules/vendored-inputs.md). Never edit a file here: change the pin in
docs/VERSIONS.md and re-run the script.

- Source: <https://github.com/openEHR/CKM-mirror>, the mirror of the international CKM
- Pin: commit `c3fdc43dcf0eb7ef0cfe60f548fcdebc24c70094`
- Tree digest (sha256 over the sorted per-file `sha256  path` listing,
  `PROVENANCE.md` excluded): `7b35cf709834019cadae6a92f8df1cd00ed928406e0bc0726ad67a5b8622a7d3`
- Fetched: 2026-11-02
- Upstream licence: Creative Commons Attribution-ShareAlike 3.0 Unported, the
  mirror's `LICENSE` file at the same commit, vendored beside this file
- Use: the archetypes FerroBRIDGE's medication template specialises or fills its slots with, read by
  the FerroBRIDGE test suites and never shipped in a crate (the testkit crate
  is `publish = false`)

Files:

- `openEHR-EHR-COMPOSITION.encounter.v1.adl` from `local/archetypes/composition/openEHR-EHR-COMPOSITION.encounter.v1.adl`, sha256 `f334e2d3b7238c634cf753d19ab6e7900e8ffc4baa4b0c9614be4d55168d866c`
- `openEHR-EHR-INSTRUCTION.medication_order.v3.adl` from `local/archetypes/entry/instruction/openEHR-EHR-INSTRUCTION.medication_order.v3.adl`, sha256 `206c2a07fa1a7197ebc9ed7a29a418ecfc9cb6b63fa2aaf811e780ee69df9e9b`
- `openEHR-EHR-ACTION.medication.v1.adl` from `local/archetypes/entry/action/openEHR-EHR-ACTION.medication.v1.adl`, sha256 `7e2484d4aec5ad8c5ceb5edb466a82b43b45396fbe3abaa4ee4a298b8e6437bc`
- `openEHR-EHR-CLUSTER.medication.v2.adl` from `local/archetypes/cluster/openEHR-EHR-CLUSTER.medication.v2.adl`, sha256 `8ea80b21e16b8f7dbbd7d9215b55619bf5c3670813ffa3cd07198745cb9a5bb9`
- `openEHR-EHR-CLUSTER.therapeutic_direction.v1.adl` from `local/archetypes/cluster/openEHR-EHR-CLUSTER.therapeutic_direction.v1.adl`, sha256 `d507ee1d0458c6b3089c9108c66a20e33dc78eb437f002e6bf69855ce406e86a`
- `openEHR-EHR-CLUSTER.dosage.v2.adl` from `local/archetypes/cluster/openEHR-EHR-CLUSTER.dosage.v2.adl`, sha256 `5b497db6b93eceffe4589d01c8a2771c6dd00fa3a3768b11e2a4a94d99982db9`
- `openEHR-EHR-CLUSTER.timing_daily.v1.adl` from `local/archetypes/cluster/openEHR-EHR-CLUSTER.timing_daily.v1.adl`, sha256 `b2ac788c50c76dcf641f59c8dcf75874557df8640712631fcc988197ba39605b`
- `openEHR-EHR-CLUSTER.medication_authorisation.v0.adl` from `local/archetypes/cluster/openEHR-EHR-CLUSTER.medication_authorisation.v0.adl`, sha256 `920f4769badd87df62af444846560e6bed940c10ed9b358c026c3c22f025567b`
- `openEHR-EHR-CLUSTER.medication_order_summary.v0.adl` from `local/archetypes/cluster/openEHR-EHR-CLUSTER.medication_order_summary.v0.adl`, sha256 `d83990ffb5fd4f2b76ab30b6b18364e1157df742abd481cbfc166b1e01c9caec`
- `openEHR-EHR-CLUSTER.medication_supply_amount.v0.adl` from `local/archetypes/cluster/openEHR-EHR-CLUSTER.medication_supply_amount.v0.adl`, sha256 `a5dd6492402310e3b58fa392ef2af68bd09589bc77c9b6bc21011e3232e33df4`
