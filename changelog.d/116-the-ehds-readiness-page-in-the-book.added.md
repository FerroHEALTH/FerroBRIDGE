- **The EHDS readiness page in the book** (#116). It states the legal status
  plainly: Regulation (EU) 2025/327 names the six priority categories, the
  implementing act that fixes the exchange format is not adopted, and the HL7
  Europe guides are the current proxy, so nothing is a conformance claim. Its
  readiness matrix lists, per category, each target profile with its
  committed template and the round-trip test that proves the transform, and
  records the patient summary Composition, imaging and discharge reports as
  corpus gaps. A `fhirconnect` test generates the matrix from
  `docs/ehds/readiness.toml` and fails when a template, a context or a test it
  names is gone. `fhirconnect` 0.1.198 (with `ferrobridge-hl7v2` 0.1.153)
  takes `toml` as a test dependency.
