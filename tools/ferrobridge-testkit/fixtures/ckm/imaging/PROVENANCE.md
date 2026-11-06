<!-- This file describes vendored third-party material; the bytes beside it
     keep their upstream licence, not the licence of this repository. -->

# Provenance: the CKM imaging archetypes

One published archetype of the openEHR Clinical Knowledge Manager,
vendored verbatim by `scripts/vendor/ckm-imaging.sh`
(.claude/rules/vendored-inputs.md). Never edit a file here: change the pin in
docs/VERSIONS.md and re-run the script.

- Source: <https://github.com/openEHR/CKM-mirror>, the mirror of the international CKM
- Pin: commit `c3fdc43dcf0eb7ef0cfe60f548fcdebc24c70094`
- Tree digest (sha256 over the sorted per-file `sha256  path` listing,
  `PROVENANCE.md` excluded): `e53e09672873105cd90a2503f96049159a520b797ad5458076a2630fb49ba6ca`
- Fetched: 2026-11-06
- Upstream licence: Creative Commons Attribution-ShareAlike 3.0 Unported, the
  mirror's `LICENSE` file at the same commit, vendored beside this file
- Use: the observation FerroBRIDGE's imaging report template holds, read
  by the FerroBRIDGE test suites and never shipped in a crate (the testkit crate
  is `publish = false`)

Files:

- `openEHR-EHR-OBSERVATION.imaging_exam_result.v1.adl` from `local/archetypes/entry/observation/openEHR-EHR-OBSERVATION.imaging_exam_result.v1.adl`, sha256 `a273990123f9f9bb213cab4fe91ec98b57c7127791520895550b087863e710d5`
