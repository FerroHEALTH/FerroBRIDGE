- The imaging and laboratory report sections whose HL7 Europe code the
  MyHealth@EU `eHDSISection` value set lacks carry the `eHDSISection`
  concept that applies beside it, as FHIR R4 terminologies §4.1.5.3 requires
  at an `extensible` binding: LOINC `30954-2` for the imaging study and every
  laboratory study-type section, and LOINC `29554-3` for the imaging
  procedure section. `Slot` gains the `ehdsi` field and `Slot::with_ehdsi`;
  `eehrxf` moves to 0.49.0.
