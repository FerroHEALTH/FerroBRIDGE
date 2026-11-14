- The facade exchanges a patient's openEHR access directives as HL7 FHIR
  `Consent` (R4, and R5 under `fhirVersion=5.0`), IHE BPPC acknowledgements
  and IHE APPC Privacy Consent Documents, behind the new `[facade.consent]`
  table (`enabled`, `extensions_url`, `default_access`), which needs the
  onward lane: the directives are read and written under the token exchanged
  for the facade's caller, never under the bridge's own credential. `POST`, `GET
  [base]/Consent/{id}` and `GET [base]/Consent?patient=` are served and listed
  in the `CapabilityStatement`, beside the type-level operations
  `Consent/$bppc` and `Consent/$appc`. An import reads the CDR's `EHR_ACCESS`
  through its `GET /ehr/{ehr_id}/ehr_access` extension and commits the
  directives as the next version in a CONTRIBUTION; the catalogue of the
  controller's `ACCESS_POLICY` is read under `extensions_url`. A construct with no
  openEHR form is refused with `422` naming the element, and every attribute
  with no core FHIR element travels in an extension under
  `https://ferrobridge.eu/fhir/StructureDefinition/`.
