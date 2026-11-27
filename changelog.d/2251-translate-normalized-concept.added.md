- `fhir-types` 0.4.0 carries three more parameters of the terminology
  ecosystem overlay for R4, R4B, R5 and R6. `ConceptMap/$translate` gains the
  `match.normalized-concept` part (tx-ecosystem IG, `$translate`, The result,
  §207): the target concept as a `Coding` in its code system's normal form
  when the map states it in another form. `CodeSystem/$lookup` gains the
  `used-supplement` output (`0..*`, `canonical`; requirements, CodeSystem
  supplements), which reports a global language pack that contributed a
  display or a designation. R4 and R4B `$translate` gain the lowercase
  `targetcode` input (`0..1`, `code`; requirements, Reverse translation), the
  R4 spelling of the reverse direction beside `reverse`. The new public
  fields break a struct literal of these contracts, so a consumer that builds
  one adds the new fields.
