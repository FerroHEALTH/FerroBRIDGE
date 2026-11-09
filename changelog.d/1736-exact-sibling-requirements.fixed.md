- Every published crate of the line requires its workspace siblings exactly
  (`=X.Y.Z`), so a crate published from here always builds against the
  sibling it was released with. A range let `ferrobridge-validator` 0.1.151,
  already on crates.io, resolve `ferrobridge-term` 0.1.146, whose breaking
  change it never saw. `eehrxf`, `ferrobridge-fhirpath`,
  `ferrobridge-hl7v2`, `ferrobridge-term`, `ferrobridge-validator` and
  `fhirconnect` move one patch for the new requirements, and
  `crate-version-guard` refuses a caret or tilde between workspace crates.
