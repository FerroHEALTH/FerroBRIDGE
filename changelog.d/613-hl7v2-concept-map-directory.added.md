- The operator API answers `GET /operator/hl7v2/concept-maps`, the HL7 v2
  face's ConceptMap directory: every map the face runs with its kind, what it
  maps from and to, whether it is the guide's or a supplement's, its rows and
  the messages it served since the bridge started. The operator console shows
  it on the HL7 v2 page's ConceptMaps tab. `ferrobridge-hl7v2` gives each
  `Map` its `title` and its `ends()`, and each `Mapped` the maps it
  `applied()`; a consumer that builds a `Map` by its fields adds `title`.
