- **Breaking for stored resource ids: `$tofhir` ids follow the FHIRconnect
  deterministic id** (#1276). A resource the identity seam and the mapping
  give no id takes the hash `engine/id-management.adoc` §Deterministic IDs
  recommends, over the composition's `object_id` and the entry path of its
  occurrence, as 52 lowercase base32 characters, in place of the
  composition's `object_id` and the `<uid>.2` form. An id a caller stored
  from an earlier `$tofhir` answer no longer matches; map the composition
  again to read the new id. The identity seam's request carries the entry
  path, and `fhirconnect::engine::identity` and
  `WebTemplateIndex::entry_path` are the shared derivation the FHIR facade
  adopts next.
