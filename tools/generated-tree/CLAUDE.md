# `generated-tree`: the generated-tree writer and drift check (tooling)

The one owner of what `tools/fhir-codegen` and `tools/omop-cdm-codegen`
share about a generated tree (#1417): writing the rendered files, formatting
them with the pinned `rustfmt`, reading them back, and comparing them with
the committed tree for the drift check (`.claude/rules/codegen.md`: the
output is byte-deterministic and a drift check fails on any diff).

- **What a generator decides stays in the generator:** which files it
  renders, which paths `rustfmt` is given, where each file lands in the
  crate, and which directories it owns whole.
- **Deterministic.** Every map is a `BTreeMap` and the drift list is sorted.
- **Gates:** `cargo clippy -p generated-tree --all-targets -- -D warnings`,
  `cargo nextest run -p generated-tree`, and the drift checks of both
  generators.
