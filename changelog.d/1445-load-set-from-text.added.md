- `fhirconnect::model::load::load_set_from_text` loads a FHIRconnect mapping
  set from `(name, text)` pairs through the same schema, lowering and
  semantic checks as `load_set`, and `eehrxf`'s
  `Mapping::compile_project` compiles a shipped `mappings::Project` from
  its embedded text with no file written.
