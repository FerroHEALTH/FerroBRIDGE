- `eehrxf`'s shipped laboratory mapping writes each result's
  `effective[x]`, from the time of the laboratory test result's event, and
  its `performer` absent with the FHIR R4 `data-absent-reason` extension,
  because `KDS_Laborbericht` holds no performing laboratory. HL7 Europe
  Laboratory Report 2.0.0 `Observation-resultslab-eu-lab` requires both.
  `Document::with_mapped_report` keeps the run's `OperationOutcome` out of
  the document, which reaches every entry from its `Composition`.
