- **`jsonschema` 0.59.0.** The FHIRconnect and OMOCL mapping schemas are
  checked with `jsonschema` 0.59.0, which refuses a value whose regex match
  the engine cannot finish where 0.58 accepted it under `not` or
  `patternProperties`. `fhirconnect` 0.1.204 and `ferrobridge-hl7v2`
  0.1.162 follow their requirements.
