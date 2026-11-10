- `fhirconnect` writes one alternative of a FHIR choice element at a time: a
  mapping that writes `value.as(Coding)` after another wrote the element's
  `valueCodeableConcept` replaces it, so a produced resource no longer carries
  two `value[x]` members, which FHIR R4 forbids (formats.html §Choice). The
  KDS case extension `KDS_fall_problem_diagnose` of the FHIRconnect mapping
  library now runs verbatim against the published `KDS_Fall_einfach`
  template, and the library passes 21 of 129.
