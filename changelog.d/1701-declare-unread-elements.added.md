- A `fhirconnect` run out of openEHR declares each archetype element the
  composition holds at the mapped occurrence that no mapping reads, as the new
  `Loss::Unread` warning naming the element's node id and openEHR path, so a
  served resource never drops recorded content with no trace. An element a
  `manual` entry carries through its `openehrCondition` counts as read, and an
  element a skipped mapping already names is declared once.
- The `eehrxf` discharge report mapping serves a problem's severity, date of
  resolution and diagnostic certainty (`provisional` or `confirmed`), a
  procedure's reasons, outcome and complications, and a discharge medicine's
  route and order start in its one `dosageInstruction`.
- The `eehrxf` imaging mapping serves each imaging diagnosis as a
  `DiagnosticReport.conclusionCode` and receives one back, and a served
  imaging report declares the findings, the differential diagnoses, the
  comparison findings, the clinical summary, the recommendations and the
  comments the composition records (#1700).
