- The imaging report's `Composition.type` carries LOINC `18748-4`
  "Diagnostic imaging study" beside the report's own code, so the document
  meets the `required` binding of the MyHealth@EU imaging guide to
  `eHDSIDocumentTypes` as well as the HL7 Europe Imaging one. `Kind::Report`
  takes the codes a form's type carries beside the report's (`beside`), and
  `eehrxf::document::imaging::MYHEALTHEU_IMAGING_TYPE` names this one.
