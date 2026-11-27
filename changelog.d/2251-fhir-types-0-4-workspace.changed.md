- The bridge and its crates move to `fhir-types` 0.4.0. `ferrobridge-term`
  0.24.0 reads the terminology ecosystem's `match.normalized-concept` of a
  `ConceptMap/$translate` answer into `Match::normalized_concept`: the target
  concept in its code system's normal form when the map states it in another
  form, with `concept` still the target as the map states it (tx-ecosystem IG,
  `$translate`, The result, §207). `Match` gains the public field, so a
  consumer that builds one by its fields adds it. The crates that expose
  `fhir-types` or `ferrohealth-smart` take their next minor: `fhirconnect`
  0.15.0, `ferrobridge-fhirpath` 0.5.0, `ferrobridge-validator` 0.25.0,
  `ferrobridge-hl7v2` 0.39.0 and `eehrxf` 0.48.0, and the server moves to
  `ferrohealth-smart` 0.36.0, `ferrohealth-session` 0.33.0, `ihe-atna` and
  `ehds-logging` 0.31.0 and `ihe-iti` 0.28.0.
