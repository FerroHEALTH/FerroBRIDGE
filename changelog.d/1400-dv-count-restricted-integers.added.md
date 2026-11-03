- The FHIRconnect engine carries an openEHR `DV_COUNT` to and from the R4
  restricted integers `unsignedInt` and `positiveInt`, such as
  `MedicationRequest.dispenseRequest.numberOfRepeatsAllowed`. A count below
  the least value the type admits (0 or 1) is refused going out, never
  clamped. The FHIRconnect 1.0.0 `DV_COUNT` page names only `integer`, so
  this pairing is FerroBRIDGE's own reading of the R4 definitions.
  This breaks the `fhirconnect` API: `engine::fhir::FhirKind` and
  `FhirValue` gain the variants `UnsignedInt` and `PositiveInt`, so a
  consumer that matches either exhaustively adds the two arms.
