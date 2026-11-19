- The FHIR facade serves SMART App State (SMART App Launch 2.2.0, Persisting
  App State) behind `[facade.app_state]`: `Basic` at the base becomes the App
  State endpoint with create, update and delete under `If-Match` and the search
  by `code` and `subject`, kept in a store of its own; the discovery document
  lists `smart-app-state` and the `CapabilityStatement` instantiates
  `smart-app-state-server`. Granular `?code=` scopes on `Basic` are honoured.
