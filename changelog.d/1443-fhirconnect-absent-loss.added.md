- The FHIRconnect engine declares every element a run writes out of
  openEHR with only the FHIR R4 `data-absent-reason` extension as a loss,
  `Loss::Absent`, a `warning` in the run's `OperationOutcome` located at
  the element. `Loss` gains that variant, so a consumer that matches it
  exhaustively adds the arm.
