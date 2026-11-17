- The FHIR facade answers cross-origin requests, as SMART App Launch 2.2.0
  asks of a server that supports browser apps: `GET /fhir/metadata` and the
  SMART discovery document answer any origin with
  `Access-Control-Allow-Origin: *`, and the REST routes under `/fhir` answer
  the origins the new `[facade.cors] allowed_origins` lists, by name with
  `Vary: Origin`, preflight included, and never with a wildcard or
  `Access-Control-Allow-Credentials`. A refusal of the guard, such as the
  `401` of a missing token, is readable by an admitted origin.
