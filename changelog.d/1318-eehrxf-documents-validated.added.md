- **HL7's FHIR Validator checks the documents the `eehrxf` suites write.** The
  CI `fhir-validator` job runs it offline over every patient summary document
  the suites emit under `EEHRXF_EMIT_DIR`, against the HL7 Europe Patient
  Summary and Base and Core profiles and the MyHealth@EU Master Value Sets
  Catalogue. It fails on an error, a fatal issue or a claimed profile it
  could not find, and keeps its results as the `eehrxf-fhir-validation`
  artifact (`scripts/checks/fhir-validator.sh`).
