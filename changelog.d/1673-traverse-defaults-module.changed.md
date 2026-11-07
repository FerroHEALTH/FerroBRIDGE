- `fhirconnect`: `Defaults` and `ComposerId` move to
  `fhirconnect::engine::traverse::defaults`. A consumer imports
  `fhirconnect::engine::traverse::defaults::Defaults` and
  `fhirconnect::engine::traverse::defaults::ComposerId` in place of the
  `engine::traverse` paths, which no longer resolve.
