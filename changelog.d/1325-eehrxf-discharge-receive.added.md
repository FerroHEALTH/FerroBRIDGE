- `eehrxf` receives a discharge report: with the `discharge` feature,
  `ReceivedDocument::category` names `Category::Discharge` for a document
  whose `Composition.type` carries LOINC `34105-7`, the code
  `composition-eu-hdr` of HL7 Europe Hospital Discharge Report 1.0.0 fixes,
  and `Category::profiles` names `bundle-eu-hdr` and `composition-eu-hdr`
  for `ReceivedDocument::check`.
