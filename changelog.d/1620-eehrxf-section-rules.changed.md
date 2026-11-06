- `eehrxf::document::Document::into_bundle` holds the sections to the
  section invariants of each HL7 Europe composition profile and refuses a
  document that breaks one: a patient summary section of `sectionProblems`,
  `sectionAllergies` or `sectionMedications` with no entry and no empty
  reason (`composition-eu-eps` `ips-comp-1`), an imaging report section with
  neither (`CompositionEuImaging` `eu-imaging-composition-1`), each with the
  new `DocumentError::Unreasoned` naming the section, and a discharge report
  with neither a discharge summary nor a hospital course section
  (`composition-eu-hdr` `cmp-hdr-2`), with the new `DocumentError::Absent`.
  The new `Form::rules` field lists a form's `SectionRule`s, so a consumer
  that writes a `Form` literal adds it; one that assembles such a document
  gives each empty section an `EmptyReason` with `Section::empty_because`.
