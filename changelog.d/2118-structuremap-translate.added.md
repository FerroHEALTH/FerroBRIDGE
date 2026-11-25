- The StructureMap engine runs the `translate` transform through the
  terminology server's `ConceptMap/$translate` and writes the `code`,
  `system`, `display`, `Coding` or `CodeableConcept` the map asks for. A
  refused call, no translation or matches naming two concepts stop the map
  with a typed error.
