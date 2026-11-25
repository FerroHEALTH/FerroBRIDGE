<!-- This file describes vendored third-party material; the bytes beside it
     keep their upstream licence, not the licence of this repository. -->

# Provenance: the CKM EHDS archetypes

1 published archetypes of the openEHR Clinical Knowledge Manager,
vendored verbatim by `scripts/vendor/ckm-ehds.sh`
(.claude/rules/vendored-inputs.md). Never edit a file here: change the pin in
docs/VERSIONS.md and re-run the script.

- Source: <https://github.com/openEHR/CKM-mirror>, the mirror of the international CKM
- Pin: commit `c3fdc43dcf0eb7ef0cfe60f548fcdebc24c70094`
- Tree digest (sha256 over the sorted per-file `sha256  path` listing,
  `PROVENANCE.md` excluded): `f6b1aa21784c5b36d02dd18a35e3645454a1642fc9c444a73ea9f6cdbe3ca5c1`
- Fetched: 2026-11-25
- Upstream licence: Creative Commons Attribution-ShareAlike 3.0 Unported, the
  mirror's `LICENSE` file at the same commit, vendored beside this file
- Use: the entries FerroBRIDGE's EHDS record template holds for the draft
  EEHRxF Annex X data groups, read by the FerroBRIDGE test suites and never
  shipped in a crate (the testkit crate is `publish = false`)

Files:

- `openEHR-EHR-OBSERVATION.body_weight.v2.adl` from `local/archetypes/entry/observation/openEHR-EHR-OBSERVATION.body_weight.v2.adl`, sha256 `0f61613a6f79125dfb60acade4cafa1342af5c1fd4893178555f9a9dea4b9958`
