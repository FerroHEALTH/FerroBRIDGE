- **The bridge takes the `openehr-*` crates at 0.0.99 and drops three local
  openEHR spellings** (#618). The archetype HRID of an ADL 2 template ETag
  and of an archetype node id now reads through `openehr-am`'s `FromStr` for
  `ArchetypeHrid`, so the server no longer depends on `openehr-adl`. A
  `TERMINOLOGY_ID` is built with `openehr-base`'s `TerminologyId::new`. The
  `change_type` and `lifecycle_state` codes and rubrics of a commit come from
  `openehr-term`'s named concepts, now a direct dependency. What the bridge
  writes is unchanged. `fhirconnect` 0.1.195, `openehr-mapping-core` 0.1.130
  and `ferrobridge-hl7v2` 0.1.149 carry the new pins. `fhirconnect` removes
  the public `fhirconnect::engine::lens::code_phrase::terminology_id`; a
  caller uses `TerminologyId::new(system, version)`.
