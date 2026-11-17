- A FHIRconnect mapping of a FHIR time onto a single `EVENT` that the template
  folds into its `OBSERVATION` now compiles and runs, as `EVENT.adoc` pairs a
  `dateTime` with a `POINT_EVENT` and a `Period` with an `INTERVAL_EVENT`. The
  engine writes and reads the event through the `time` leaf the Web Template
  keeps at the event's path, with the class, width and function in the object
  that holds it. The library's `medication_statement.v0.yml` no longer stops
  at line 44 against `KDS_Medikationseintrag`. `openehr-mapping-core` gains
  `WebTemplateIndex::read_holder` and `amend_holder`, and `fhirconnect` gains
  `EventTime::in_folded_event` and `is_folded`.
