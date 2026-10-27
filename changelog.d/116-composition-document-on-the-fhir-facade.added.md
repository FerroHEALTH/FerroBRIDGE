- **`Composition/$document` on the FHIR facade** (#116, the R4 operation
  `Composition-document`). `GET /fhir/Composition/{id}/$document`, and
  `GET /fhir/Composition/$document?id={id}`, answer a document Bundle: an
  identifier derived from the composition version, the assembly timestamp,
  the Composition first and then every resource its context's outbound run
  creates, each with its read URL as `fullUrl`. The caller needs read on every
  type the document carries. `persist=true` and `graph` are refused with an
  `OperationOutcome`, and the operation is declared on `Composition` in the
  `CapabilityStatement`. The facade now binds a context whose start is a
  composition model, such as `Composition-eu-lab`, to the composition itself.
