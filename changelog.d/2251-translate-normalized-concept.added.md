- `fhir-types` 0.4.0 declares the terminology ecosystem's
  `match.normalized-concept` part of `ConceptMap/$translate` for R4, R4B, R5
  and R6 (tx-ecosystem IG, `$translate`, The result, §207): the target concept
  as a `Coding` in its code system's normal form when the map states it in
  another form. The new public field breaks a struct literal of the `match`
  part, so a consumer that builds one adds `normalized_concept`.
