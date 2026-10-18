<!-- This file describes vendored third-party material; the bytes beside it
     keep their upstream licence, not the licence of this repository. -->

# Provenance: the FHIRconnect specification source

Vendored verbatim by `scripts/vendor/fhirconnect.sh`
(.claude/rules/vendored-inputs.md). Never edit a file here: change the pin in
docs/VERSIONS.md and re-run the script.

- Source: <https://github.com/SevKohler/FHIRconnect-spec>
- Pin: commit `e587164102921c73eea262a18f3457b18e4a2a40`
- Fetched: 2026-10-18
- Upstream licence: Apache License 2.0, the repository's `LICENSE` file,
  vendored beside this file
- Layout: the upstream paths, unchanged
- Files: 75, of which 63 are `modules/ROOT/pages/**/*.adoc`
- Tree digest (sha256 over the sorted per-file `sha256  path` listing,
  `PROVENANCE.md` excluded): `883ebbd874f2109da75c4bcc514dd2fa0a388a9dc60aa61e8f3893f103388f90`

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
| `model-mapping.schema.json` | `f92c4902e6b779eecfd40ca4ea774441609db8ffb7163832efc99b562cdfeba4` | `1c24d58b08e8315b7dcdc17aa65543a07c41bede5295bb0aff60b9af367f87c9` |
| `contextual-mapping.schema.json` | `0402888b08a1ddfd457eb0e3c3f6c4bdac7f2339762b9553fce4c096c88f25d2` | `aa55cb7df7105de929b584ab682037e53ae33582fb4c9d533ce04eb24999ae35` |
