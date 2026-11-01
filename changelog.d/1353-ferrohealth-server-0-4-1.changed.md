- FerroBRIDGE runs on `ferrohealth-server` 0.4.1, which keeps a product's
  own fallback and the layers around it when the stack merges its routes
  (FerroHEALTH/FerroSYS#199). FerroBRIDGE was not exposed to the regression
  0.4.1 fixes: no release ran on an affected version, and the facade guard
  and the operator API guard only the routes they serve, so an anonymous
  request to a path no route matches answers `404` with no challenge before
  and after, while a route under `/fhir` answers `401`.
