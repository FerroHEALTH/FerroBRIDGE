- The `OperationOutcome` a FHIR route answers when its handler fails names
  the request id the response echoes in `X-Request-Id`, and the server's own
  id beside it when the client sent its own, so the client finds the id it
  sent and the operator the one the log names. It used to name the server's
  id alone.
