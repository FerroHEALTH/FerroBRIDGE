- `eehrxf` ships an HL7 Europe Patient Summary mapping project
  (`mappings::EPS`) over the openEHR International Patient Summary
  template. It maps each medication order of the template's medication
  summary section to a `MedicationStatement-eu-eps`, so a patient whose
  medicines a CDR records only as orders has them in the medication summary,
  and each medication action of its immunisations section to an
  `immunization-eu-core`. The patient summary crosswalk names these openEHR
  sources (`crosswalk::patient_summary::SOURCES`): an entry is placed by the
  template section that holds it, so a composition with a vaccine and a
  medicine puts each in its own section.
