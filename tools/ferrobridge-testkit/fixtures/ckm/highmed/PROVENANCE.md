<!-- This file describes vendored third-party material; the bytes beside it
     keep their upstream licence, not the licence of this repository. -->

# Provenance: the HiGHmed CKM archetypes of the FHIRconnect mapping library

5 published archetypes of the HiGHmed Clinical Knowledge Manager,
vendored verbatim by `scripts/vendor/ckm-highmed.sh`
(.claude/rules/vendored-inputs.md). Never edit a file here: change the pin in
docs/VERSIONS.md and re-run the script.

- Source: <https://github.com/openEHR-de/CKM-mirror>, the mirror of the German CKM
- Pin: commit `adc22deddb96d5e557186fe595f52ef287510633`
- Tree digest (sha256 over the sorted per-file `sha256  path` listing,
  `PROVENANCE.md` excluded): `08cb53b55345d628d66ba3e1c5a83891a988a9cc0f21025eeff3367e9ff4478f`
- Fetched: 2026-11-05
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
- `openEHR-EHR-CLUSTER.medication_status_fhir.v0.adl` from `local/archetypes/cluster/openEHR-EHR-CLUSTER.medication_status_fhir.v0.adl`, sha256 `5639f0bad50b7ba4fabbf682600cfaf9c73bdab714c40dc9c2db19e0b3d00865`
- `openEHR-EHR-CLUSTER.observation_status_fhir.v1.adl` from `local/archetypes/cluster/openEHR-EHR-CLUSTER.observation_status_fhir.v1.adl`, sha256 `68a565ec1522f09c74ce359d52b0b4815588b86e830ee26f5e424ecd007ea473`
- `openEHR-EHR-CLUSTER.organization.v0.adl` from `local/archetypes/cluster/openEHR-EHR-CLUSTER.organization.v0.adl`, sha256 `683327918e72aea7d6ce22208f9411e0d6d6bcac40a2b5dbb5c492a5d072cb1d`
