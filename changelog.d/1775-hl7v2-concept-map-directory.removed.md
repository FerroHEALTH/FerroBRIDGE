- The `[hl7v2]` keys `concept_maps` and `supplements`: the bridge carries the
  v2-to-FHIR structure maps and reads no ConceptMap directory, and the
  configuration refuses either key as unknown. The operator API's
  `GET /operator/hl7v2/concept-maps` document drops its `directory` and
  `supplements` members, and the console's HL7 v2 settings and ConceptMaps
  tab drop them too.
- From `ferrobridge-hl7v2`: `Corpus` with `Corpus::load`, `Corpus::shipped`
  and `Corpus::supplement`, `map::translate::Translator` with its offline
  translator, `map::guide`, `map::supplement`, and the `shipped-maps` and
  `terminology-server` features.
