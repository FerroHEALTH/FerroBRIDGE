- **`$tofhir` answers one resource per occurrence of a repeated start
  archetype** (#1271). A composition holding two diagnoses answers two
  Conditions under one `Provenance`, each with a stable id derived from the
  composition and its occurrence, and an occurrence the context's condition
  does not admit is counted in the `OperationOutcome`. The identity seam's
  request names the occurrence, and two resources taking one id refuse the
  call.
