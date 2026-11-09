- `fhirconnect` 0.1.249 converts a composition out of openEHR when a mapping
  reads a value its node does not hold and a later mapping writes the same
  repeating FHIR element. The empty mapping took the first instance, so the
  run refused with an occurrence gap: a KDS vital status composition with a
  composer and no health care facility did not convert to an `Observation`.
