- Breaking for a `ferrobridge-hl7v2` consumer (0.2.0): the v2-to-FHIR
  message, segment and data type maps are generated into the crate at build
  time from the pinned `hl7.fhir.uv.v2mappings` 1.0.0 package, with
  FerroBRIDGE's structure corrections as generator inputs, and
  `map::maps::Maps::corrected()` (or `Maps::guide()`, the guide alone)
  replaces `Corpus`. Every table value is translated by
  `ConceptMap/$translate` on a terminology server: `map::map`,
  `map::single::segment` and `map::inverse::segment` take an
  `Option<&ferrobridge_term::client::Client>`, and the `map` feature carries
  the client. A table map's `TableGroup` names its source and target systems
  only.
- FerroBRIDGE's value maps for the HL7 v2 face (the corrected copy of
  `table-hl70202-to-contact-point-system` as
  `table-hl70202-equipment-to-contact-point-system`, the added
  `table-hl70202-to-contact-point-use` and the FHIR-to-v2 maps the PID
  inverse uses) ship as the FHIR package `terminology/hl7v2-values/`, each
  under an id of its own and a url of `https://ferrobridge.eu/fhir/v2mappings/`. Load it into the
  terminology server beside the guide's package; the HL7 v2 face page of the
  book says how.
