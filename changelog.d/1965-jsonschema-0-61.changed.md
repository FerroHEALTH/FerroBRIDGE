- **`jsonschema` 0.61.0.** The FHIRconnect and OMOCL mapping schemas are
  checked with `jsonschema` 0.61.0. `jsonschema` is part of the public API of
  `openehr-mapping-core` (`schema::compile` returns a `jsonschema::Validator`)
  and `fhirconnect` (`Schemas::validator`), so both move to a new
  compatibility line, and so do `eehrxf` and `ferrobridge-hl7v2`, which
  expose them: a consumer requires `openehr-mapping-core = "0.5"`,
  `fhirconnect = "0.6"`, `eehrxf = "0.18"` and `ferrobridge-hl7v2 = "0.20"`,
  and takes the `jsonschema` 0.61 `Validator`.
