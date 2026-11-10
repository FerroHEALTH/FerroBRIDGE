- `eehrxf`: mapping a received imaging report with the `IMAGING` project
  declares in its `OperationOutcome` each section none of whose entries the
  project carries, such as the order, history and procedure sections, and
  each element of the report or its study that the composition does not
  hold, such as the performing organization (`DiagnosticReport.performer`)
  and a study note. Each is an `incomplete` warning at the element's
  FHIRPath. The composition's original content still keeps every one of
  them.
