<!-- This file describes vendored third-party material; the bytes beside it
     keep their upstream licence, not the licence of this repository. -->

# Provenance: the HiGHmed CKM archetypes of the FHIRconnect mapping library

12 published archetypes of the HiGHmed Clinical Knowledge Manager,
vendored verbatim by `scripts/vendor/ckm-highmed.sh`
(.claude/rules/vendored-inputs.md). Never edit a file here: change the pin in
docs/VERSIONS.md and re-run the script.

- Source: <https://github.com/openEHR-de/CKM-mirror>, the mirror of the German CKM
- Pin: commit `adc22deddb96d5e557186fe595f52ef287510633`
- Tree digest (sha256 over the sorted per-file `sha256  path` listing,
  `PROVENANCE.md` excluded): `63cd90f0a9dbb7b3eb46f678c390dd09344774ed26231844c49f1c328fa4b4c8`
- Fetched: 2026-11-17
- Upstream licence: Creative Commons Attribution-ShareAlike, 3.0 or 4.0 as
  each archetype states in its `description` `licence`; the mirror
  carries no `LICENSE` file
- Use: the archetypes the FHIRconnect mapping library's model mappings
  declare, which the templates FerroBRIDGE authors under
  `tools/ferrobridge-testkit/fixtures/adl2/library/` specialise or fill
  their slots with so each mapping compiles in full (#1546), read by the
  FerroBRIDGE test suites and never shipped in a crate (the testkit crate is
  `publish = false`)

Files:

- `openEHR-EHR-COMPOSITION.fall.v1.adl` from `local/archetypes/composition/openEHR-EHR-COMPOSITION.fall.v1.adl`, sha256 `d92965e8eb5720798bf53e69e3d78f6ed28e6821d857ffae63969062d30d1abd`
- `openEHR-EHR-COMPOSITION.person.v0.adl` from `local/archetypes/composition/openEHR-EHR-COMPOSITION.person.v0.adl`, sha256 `265d4fcfc863ad62a528675784fe3f2992542d0330dff8f17c2bee78be01a0ba`
- `openEHR-EHR-CLUSTER.entry_category.v0.adl` from `local/archetypes/cluster/openEHR-EHR-CLUSTER.entry_category.v0.adl`, sha256 `2a6a0ef8c39280b8b3624f347680d3b9686260294254f5bb73c6e6b49618558f`
- `openEHR-EHR-CLUSTER.identifier_fhir.v0.adl` from `local/archetypes/cluster/openEHR-EHR-CLUSTER.identifier_fhir.v0.adl`, sha256 `9a235a1ca852825a5bc6fda007bb168f1c1b918fdb4abd5c93c1b92ef39757a8`
- `openEHR-EHR-CLUSTER.medication_status_fhir.v0.adl` from `local/archetypes/cluster/openEHR-EHR-CLUSTER.medication_status_fhir.v0.adl`, sha256 `5639f0bad50b7ba4fabbf682600cfaf9c73bdab714c40dc9c2db19e0b3d00865`
- `openEHR-EHR-CLUSTER.observation_status_fhir.v1.adl` from `local/archetypes/cluster/openEHR-EHR-CLUSTER.observation_status_fhir.v1.adl`, sha256 `68a565ec1522f09c74ce359d52b0b4815588b86e830ee26f5e424ecd007ea473`
- `openEHR-EHR-CLUSTER.organization.v0.adl` from `local/archetypes/cluster/openEHR-EHR-CLUSTER.organization.v0.adl`, sha256 `683327918e72aea7d6ce22208f9411e0d6d6bcac40a2b5dbb5c492a5d072cb1d`
- `openEHR-EHR-ADMIN_ENTRY.versicherungsinformationen.v0.adl` from `local/archetypes/entry/admin_entry/openEHR-EHR-ADMIN_ENTRY.versicherungsinformationen.v0.adl`, sha256 `177f4f58bf4154e9a3b656f845e116c6f86bec392eeb7c2e6618a6beebc10da2`
- `openEHR-EHR-ADMIN_ENTRY.episode_institution_local.v0.adl` from `local/archetypes/entry/admin_entry/openEHR-EHR-ADMIN_ENTRY.episode_institution_local.v0.adl`, sha256 `f66decfc4da533ffe68c4d46126cd144d7e44bdb8ebc4182b9805b0249bceec8`
- `openEHR-EHR-CLUSTER.death_details.v1.adl` from `local/archetypes/cluster/openEHR-EHR-CLUSTER.death_details.v1.adl`, sha256 `38c6b9a946ab76d288c02a1ae26002666444343afa68e4209b91055716a59b67`
- `openEHR-EHR-CLUSTER.person_birth_data_iso.v0.adl` from `local/archetypes/cluster/openEHR-EHR-CLUSTER.person_birth_data_iso.v0.adl`, sha256 `0157a657583dfc939058d321d69a967cd137e203a6daba9ec9bbc671c170f311`
- `openEHR-EHR-CLUSTER.location.v1.adl` from `local/archetypes/cluster/openEHR-EHR-CLUSTER.location.v1.adl`, sha256 `2b031a9cb605b59c192b3ea235e62591acb8a60ec586dc116560ae9f1de5297f`
