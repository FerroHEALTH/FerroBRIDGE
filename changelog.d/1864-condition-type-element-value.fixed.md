- `fhirconnect` evaluates an `openehrCondition` with `operator: type` against
  the class of the data value its attribute holds, the value of an
  `ELEMENT` included, as `basics/Conditions.adoc` §openehrCondition writes
  it. It tested the node's declared class, so an `onsetDateTime` of the
  library's adverse reaction risk model went into openEHR and was lost
  coming out.
