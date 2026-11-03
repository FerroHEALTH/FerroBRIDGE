- `eehrxf` maps laboratory reports between openEHR and the HL7 Europe
  Laboratory Report 2.0.0 document through FerroBRIDGE's FHIRconnect
  laboratory project over `KDS_Laborbericht`. The crate now ships its
  FHIRconnect projects under `mappings/` in its package, and
  `eehrxf::mappings` embeds them (`MPD`, `LABORATORY`, `Project::write_to`).
  `Document::with_mapped_report` takes the report, results, specimens and
  `Provenance` of one mapping run into a laboratory report document, and
  `receive` names a received `Bundle-eu-lab` a laboratory report and holds
  it to the `Bundle-eu-lab` and `Composition-eu-lab` profiles.
  `DocumentError` gains `Unmapped`. The `DiagnosticReport-eu-lab` context
  no longer loads the mapping library's `report_result.v1.DiagnosticReport`
  model, whose mappings it overwrote to none.
