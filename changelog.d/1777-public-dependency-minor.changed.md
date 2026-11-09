- `openehr-mapping-core`, `fhirconnect`, `eehrxf` and `ferrobridge-hl7v2`
  move to 0.2.0: their public API exposes `openehr-*` 0.0.136 types, which
  Cargo reads as incompatible with the 0.0.135 types of their 0.1 releases,
  and `ferrobridge-hl7v2` and `eehrxf` expose `fhirconnect` types. A
  consumer requires `openehr-mapping-core = "0.2"`, `fhirconnect = "0.2"`,
  `eehrxf = "0.2"` or `ferrobridge-hl7v2 = "0.2"` and builds against
  `openehr-*` 0.0.136. From here on a crate whose public dependency moves to
  an incompatible version bumps its minor.
