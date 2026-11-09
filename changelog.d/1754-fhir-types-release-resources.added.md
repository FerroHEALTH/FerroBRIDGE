- `fhir-types` has one resources feature per release: `stu3-resources`,
  `r4-resources`, `r4b-resources`, `r5-resources` and `r6-resources`. Each
  turns on every resource of that release alone, while `resources` still
  turns on every resource of each enabled release. `fhirconnect`,
  `ferrobridge-hl7v2`, `eehrxf`, `ferrobridge-fhirpath`,
  `ferrobridge-validator` and the bridge now ask only for the releases whose
  resources they use. A build that adds a release for terminology alone, as
  `ferrobridge-term` does with STU3 and R4B, then compiles none of that
  release's resources.
