- **`jsonschema` 0.60.0.** The FHIRconnect and OMOCL mapping schemas are
  checked with `jsonschema` 0.60.0, which runs an unfinished regex match once
  per validation and keeps the error evaluation path linear in the `$ref`
  depth; what a mapping file passes or fails is unchanged. `fhirconnect`
  0.1.207, `ferrobridge-hl7v2` 0.1.165 and `eehrxf` 0.1.132 follow their
  requirements.
