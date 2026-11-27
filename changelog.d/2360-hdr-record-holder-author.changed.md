- The HL7 Europe Hospital Discharge Report form takes the record's authors,
  a `Practitioner`, `PractitionerRole` or `Organization`, and refuses the
  gateway `Device` as `Composition.author`, which MyHealth@EU HDR 9.2.0
  `composition-hdr-myhealtheu` does not admit, so the HL7 Europe report is
  also the MyHealth@EU one where the two profiles agree. `eehrxf` moves to
  0.48.0.
