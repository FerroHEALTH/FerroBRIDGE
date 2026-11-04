- `openehr-mapping-core` owns what FHIRconnect and OMOCL share at the model
  layer: `schema::SchemaError`, `schema::compile` and `schema::validate`
  (the draft-07 compilation of a mapping schema and the step that turns each
  schema error into a positioned diagnostic), `diagnostic::LanguageCodes`
  (a language crate's code set, converted into a `DiagnosticCode`),
  `diagnostic::render_admitted` and `loader::in_path_order`. `fhirconnect`
  and `omocl` drop their own `SchemaError` and the `Display` of their
  `ModelCode`, and implement `LanguageCodes` for their code sets; every
  diagnostic code, position and message stays as it was.
