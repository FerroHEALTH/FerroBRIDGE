- The published crates follow Cargo SemVer: a compatible change bumps the
  patch, and a breaking change bumps the minor while a crate is 0.x, judged
  by `cargo-semver-checks` on every pull request, in the merge queue and in
  the release plan. Require a crate of the line by a caret range
  (`fhirconnect = "0.1"`) in place of an exact `=` pin; each crate's README
  states the range. `eehrxf`, `ferrobridge-fhirpath`, `ferrobridge-hl7v2`,
  `ferrobridge-term`, `ferrobridge-validator`, `fhir-search`, `fhir-types`,
  `fhirconnect`, `hl7v2-types` and `openehr-mapping-core` move one patch for
  the README section.
