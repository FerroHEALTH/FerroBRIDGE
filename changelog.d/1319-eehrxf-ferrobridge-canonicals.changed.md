- **Breaking: the canonical URLs `eehrxf` defines move to `ferrobridge.eu`.**
  The archetype-local code systems are
  `https://ferrobridge.eu/fhir/CodeSystem/archetype-local/` followed by the
  archetype id, and the remaining-dispensations extension is
  `https://ferrobridge.eu/fhir/StructureDefinition/remaining-dispensations`,
  where they were under `ferrofed.eu`. The FerroBRIDGE site serves the
  extension's `StructureDefinition` at its URL and describes the code systems
  at their base. No compatibility alias reads the old URLs, since no
  production deployment holds a document written with them; a consumer
  that matches either URL updates it. `eehrxf` 0.1.130.
