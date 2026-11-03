- The operator API answers a synthetic example composition of each template
  the running set compiled against at `GET /operator/templates/{id}/example`,
  generated from the template's Web Template, validated against it and the
  same at every request. A template with no example answers the new
  `no-example` problem. The operator console's playground offers it from the
  Example menu of the `$tofhir` tab, so a mapping can be tried without
  hand-written input. `openehr-mapping-core` 0.1.138 adds
  `WebTemplateIndex::example_composition` and `PathError::NotComposition`.
