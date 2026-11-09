- `ferrobridge-hl7v2` 0.1.211 splits its default features into `map` and
  `terminology-server`. `map` alone translates table values offline from the
  corpus over `fhir-types`, `fhirconnect`'s FHIR path model and
  `ferrobridge-term`'s request and outcome types, with no openEHR crate, no
  HTTP client and no `jsonschema`; `terminology-server` adds
  `Translator::Server`. The defaults keep today's behaviour.
