- **The bridge writes the composition's `language`, `territory` and
  `composer`, the `_feeder_audit`, `_link:i`, `_participation:i`,
  `_other_participation:i`, `_provider` and `_health_care_facility`
  families, and a role's `performer` whole, under `|raw`** (#618, #1000),
  through the `openehr-sdt` 0.0.99 builder. A composer that carries
  identifiers, and a provider or facility with an external reference, now
  reach the composition whole, where the `ctx/` keys and the suffix
  spellings left those parts out or refused them. A party builds through
  `build_party`, with each member of a value at the root passed as
  `<root>/<attribute>|raw`. The last local openEHR spellings of #618 are
  gone. `fhirconnect` 0.1.197, `openehr-mapping-core` 0.1.132,
  `ferrobridge-hl7v2` 0.1.151.
