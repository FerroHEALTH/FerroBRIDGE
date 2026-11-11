- `fhirconnect`: going into openEHR, a `reference` mapping skips a reference
  whose target is a resource of another type than its `resourceType`, and
  records no read for it, where it refused the whole run before. A repeating
  reference that admits several target types, such as R4
  `DiagnosticReport.performer`, now maps the targets of its type and leaves
  the others to the caller's accounting. FHIRconnect 1.0.0 is silent on such
  a target; this is FerroBRIDGE's own design, recorded in
  `docs/architecture.md`. A reference that resolves to nothing is still a
  declared skip, and the openEHR-to-FHIR direction is unchanged.
