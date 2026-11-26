- `ferrobridge-term` 0.23.0 drops its `naming-system` feature.
  `Client::identifier_systems` builds with the `client` feature, because
  `fhir-types` 0.3.0 carries `NamingSystem` in its `terminology` root set, so
  the identifier search no longer compiles every FHIR resource of every
  release the build enables. `ferrobridge-hl7v2`'s `map` feature names it no
  more.
