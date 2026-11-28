- `eehrxf` writes each coding of a mapped resource whose system is an openEHR
  terminology id (`SNOMED-CT`, `LOINC`, `ICD10`) with the FHIR system URI that
  id names, in every form the shipped mappings write. A diagnosis recorded in
  SNOMED CT no longer leaves with `system` `SNOMED-CT`.
