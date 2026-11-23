- A free-text `DV_IDENTIFIER.assigner` maps to `Identifier.assigner.display`
  and back, in place of a `Reference.reference` no FHIR document resolves, so
  `Patient/$summary` serves a patient whose identifier names its assigner in
  words; a literal reference keeps `reference` and the `system::value` form
  keeps `identifier`. `fhirconnect` 0.7.1, with `ferrobridge-hl7v2` 0.25.2
  and `eehrxf` 0.23.3 on it.
