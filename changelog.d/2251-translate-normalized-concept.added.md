- `fhir-types` 0.4.0 declares the terminology ecosystem's
  `match.normalized-concept` part of `ConceptMap/$translate` for R4, R4B, R5
  and R6 (tx-ecosystem IG 1.11.0, `$translate`, The result, §207), and
  `ferrobridge-term` 0.23.0 reads it into `Match::normalized_concept`: the
  target concept in its code system's normal form when the map states it in
  another form.
