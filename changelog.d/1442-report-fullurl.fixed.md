- `eehrxf` reserves the `fullUrl` of a document's report among its entries,
  so the `Bundle` keeps FHIR R4 `bdl-7`: a resource added later under the
  report's `DiagnosticReport/{id}` takes the next free id, and
  `Document::with_report` refuses a report whose `fullUrl` an entry holds
  already as the new `DocumentError::Collision`.
