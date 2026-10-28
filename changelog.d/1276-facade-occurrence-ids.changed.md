- **Breaking: the FHIR facade's resource ids are the deterministic ids
  `$tofhir` answers, and every occurrence of a start archetype is a resource
  of its own.** A resource id is now the hash FHIRconnect v1.0.0 recommends
  over the composition uid and the entry path (`engine/id-management.adoc`,
  §Deterministic IDs), the id `$tofhir` gives the same occurrence; an entry's
  `LOCATABLE.uid` no longer becomes its id. The facade finds the occurrences
  the way the engine does, so a start archetype inside a `SECTION`, at any
  depth, and an instance repeating below another entry each read, search and
  bind as their own resource, under an entry path such as
  `/content[openEHR-EHR-SECTION.adhoc.v1]/items[2]`. Every id the facade
  served before changes. The identity store writes schema 4 and refuses a
  file of an earlier schema at boot, naming it: remove the
  `[facade] identity_store` file to reset it, and the bridge creates it anew.
  `fhirconnect` 0.1.205 (with `ferrobridge-hl7v2` 0.1.163 and `eehrxf`
  0.1.129) adds `engine::traverse::occurrence`, which names the
  occurrences `to_fhir_each` maps without running the program.
