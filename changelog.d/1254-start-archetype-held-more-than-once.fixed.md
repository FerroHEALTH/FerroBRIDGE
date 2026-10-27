- **A composition holding a context's start archetype more than once maps
  each occurrence on its own** (#1254). The FHIRconnect engine mixed the
  entries into one resource, taking a field from whichever entry had it. The
  new `traverse::to_fhir_each` maps every occurrence to its own resource, with
  the context's condition checked per occurrence, and `traverse::to_fhir`
  maps the first and reports the rest as the `unmapped-occurrences` warning.
