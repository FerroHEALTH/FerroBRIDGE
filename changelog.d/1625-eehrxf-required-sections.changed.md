- `eehrxf::document::Document::into_bundle` refuses a patient summary or an
  imaging report that lacks a section slice its composition profile
  requires (`min = 1` on `sectionProblems`, `sectionAllergies`,
  `sectionMedications`, `sectionProceduresHx` and `sectionMedicalDevices`
  of `composition-eu-eps`, and on `imagingstudy`, `order`, `history` and
  `procedure` of `CompositionEuImaging`), with the new
  `DocumentError::Lacking` naming the slice and the new
  `SectionRule::Required`.
