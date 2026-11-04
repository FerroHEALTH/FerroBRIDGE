<!-- This file describes vendored third-party material; the bytes beside it
     keep their upstream licence, not the licence of this repository. -->

# Provenance: the CKM archetypes of the FHIRconnect mapping library

3 published archetypes of the openEHR Clinical Knowledge Manager,
vendored verbatim by `scripts/vendor/ckm-library.sh`
(.claude/rules/vendored-inputs.md). Never edit a file here: change the pin in
docs/VERSIONS.md and re-run the script.

- Source: <https://github.com/openEHR/CKM-mirror>, the mirror of the international CKM
- Pin: commit `c3fdc43dcf0eb7ef0cfe60f548fcdebc24c70094`
- Tree digest (sha256 over the sorted per-file `sha256  path` listing,
  `PROVENANCE.md` excluded): `afac18dba580e90606de7970c63daa59ae61f2b6d8f70c5fb124e602c99782a6`
- Fetched: 2026-11-04
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
- `openEHR-EHR-EVALUATION.gender.v1.adl` from `local/archetypes/entry/evaluation/openEHR-EHR-EVALUATION.gender.v1.adl`, sha256 `00fe97854efddf97374753b04494551a29a034eba78b1c13b1591752bbc5f5f9`
- `openEHR-EHR-CLUSTER.device.v1.adl` from `local/archetypes/cluster/openEHR-EHR-CLUSTER.device.v1.adl`, sha256 `0da92215f0d8603a36ed778143c1b767e2993739c6b31e2da91497589ca12efa`
