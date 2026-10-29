- **Problem details for every error outside the FHIR routes** (#1333,
  RFC 9457). The operator API answers every refusal as
  `application/problem+json` with the status it answered before, where it
  answered a JSON object with `error` and `message`. Each problem's `type`
  links to its section of the new Problem types page of the book, such as
  `https://ferrobridge.eu/docs/operate/problems.html#provisioned`, and a
  refusal whose status says all there is to say is `about:blank`. The
  stack's own `408`, `413`, `429`, `503` and panic `500` on a non-FHIR route
  are problems too, the request id as a `urn:uuid:` instance. No problem
  quotes a path, a mapping body, a reason, a token or an upstream answer; a
  `503` of a reload names its cause in the server log. The FHIR facade and
  the operations keep `OperationOutcome`. The operator console reads a
  problem's title and detail into the refusal it draws. Migration: a client
  of the operator API reads `type`, `title` and `detail` in place of `error`
  and `message`. The server takes `ferrohealth-server` 0.3.9.
