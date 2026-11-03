- `eehrxf::mapping::identified` names every entry of the `collection` Bundle
  a mapping run answers by the `fullUrl` `{base}/{type}/{id}`, giving the
  run's `Provenance` a free id, so the prescription or dispensation a system
  exchanges identifies each resource it holds (FHIR R4
  `Bundle.entry.fullUrl`). The CI `fhir-validator` job now runs HL7's FHIR
  Validator over the mapped MPD prescription and dispensation against
  `hl7.fhir.eu.mpd` 1.0.0.
