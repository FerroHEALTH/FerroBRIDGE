- `eehrxf` 0.34.0 writes the MyHealth@EU vaccination history
  (`myhealth.eu.fhir.euvac` 10.0.0-preview, a preview) behind the new
  `vaccination` feature: the shipped `euvac` FHIRconnect project maps the
  immunisations of the patient summary template to
  `vaccination-myhealtheu-euvac`, and `VACCINATION_HISTORY_FORM` writes the
  `bundle-euvac-myhealtheu` document. HL7's FHIR Validator checks it against
  the pinned guide, and its README badge counts the result.
