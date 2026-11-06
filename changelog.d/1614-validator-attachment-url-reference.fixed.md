- `ferrobridge-validator`: an `Attachment.url` that resolves to an entry of a
  document Bundle, by its `fullUrl` or relative to the holding entry's base,
  now counts as a reference to that entry for the R4 documents.html §3.3.1
  rule that a document holds only what its Composition references directly
  or indirectly. A url that names no entry marks nothing. This is our reading
  where the page is not explicit. The HL7 Europe Imaging Report
  semi-structured example, whose PDF Binary is named only by
  `DiagnosticReport.presentedForm.url`, now validates.
