- `fhirconnect` 0.1.246 puts the FHIRconnect language (`model`, `resolve`,
  `engine`, `operations`) behind a default `mapping` feature. With
  `default-features = false` the crate is the FHIR path model (`tree`) over
  `fhir-types`, with no openEHR crate and no `jsonschema`. A consumer that
  turns default features off and uses the language adds
  `features = ["mapping"]`.
