- **Breaking: a resource a run creates takes one id on `$tofhir` and on the
  FHIR facade, and keeps it in every composition version.** A further
  `hierarchy.split` occurrence and a `reference` target are named by
  `fhirconnect::engine::identity::created_id`, a hash of the composition's
  version container, the entry path of the occurrence that creates them, and
  their type, the parent's type, the creating mapping, their openEHR
  positions and a split's `unique` values. `$tofhir` framed the full version
  id into the hash, so each new version renamed every created resource, and
  the facade derived a different id of its own. Every created id either
  surface answered before changes. `$tofhir` now maps each occurrence on its
  own (`engine::traverse::occurrence::to_fhir_at`) with an identity sink
  anchored at it.
  `fhirconnect` 0.1.206 (with `ferrobridge-hl7v2` 0.1.164 and `eehrxf`
  0.1.131).
