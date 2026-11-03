<!-- This file describes vendored third-party material; the bytes beside it
     keep their upstream licence, not the licence of this repository. -->

# Provenance: the FHIRconnect specification source

Vendored verbatim by `scripts/vendor/fhirconnect.sh`
(.claude/rules/vendored-inputs.md). Never edit a file here: change the pin in
docs/VERSIONS.md and re-run the script.

- Source: <https://github.com/SevKohler/FHIRconnect-spec>
- Pin: commit `5046a6dd9ee34f5a1de152301778f302c55e3b7b`
- Fetched: 2026-11-02
- Upstream licence: Apache License 2.0, the repository's `LICENSE` file,
  vendored beside this file
- Layout: the upstream paths, unchanged
- Files: 75, of which 63 are `modules/ROOT/pages/**/*.adoc`
- Tree digest (sha256 over the sorted per-file `sha256  path` listing,
  `PROVENANCE.md` excluded): `5d96c05528e6e98f604c8096b3fb0b423c3893aa3ca28d3076d53fb76faed284`

## What is here

- `build/site/FHIRconnect/v1.0.0/_attachments/`: the two mapping schemas as the published v1.0.0 site renders
  them. These are the bytes docs/architecture.md section 2 records a sha256 for,
  and the script fails when either disagrees.
- `modules/ROOT/attachments/`: the same two schemas from the repository source at
  the pinned commit. They have moved past the rendered v1.0.0 release with
  the sub-context keys: `model-mapping.schema.json` gains `slotContext`,
  exclusive with `slotArchetype`, and `contextual-mapping.schema.json` gains
  `scope` and `contexts`.
- `modules/ROOT/pages/` and `modules/ROOT/nav.adoc`: the specification prose
  the design cites, and its navigation. The REST API chapter is
  `modules/ROOT/pages/engine/rest-api.adoc`.
- `rest/`: the REST API implementation guide source, the `ToFhir` and
  `ToOpenEhr` FSH operation definitions under `rest/input/fsh/operations/`
  with the pages and the configuration (`sushi-config.yaml`, `ig.ini`) that
  set their canonical base.

| Schema | sha256 (published v1.0.0) | sha256 (repository source) |
|---|---|---|
| `model-mapping.schema.json` | `f92c4902e6b779eecfd40ca4ea774441609db8ffb7163832efc99b562cdfeba4` | `8b803433bfbc90d515a227b3d695712a2ba21612dca56663a6ed2c6849b71008` |
| `contextual-mapping.schema.json` | `0402888b08a1ddfd457eb0e3c3f6c4bdac7f2339762b9553fce4c096c88f25d2` | `aa55cb7df7105de929b584ab682037e53ae33582fb4c9d533ce04eb24999ae35` |
