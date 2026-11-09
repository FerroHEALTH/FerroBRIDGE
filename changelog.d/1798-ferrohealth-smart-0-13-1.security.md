- The bridge takes `ferrohealth-smart` 0.13.1 (GHSA-5qm7-v4fw-p2v9): a
  refusal from an issuer's key set, discovery or token introspection
  endpoint no longer keeps the upstream response body in its error, so no
  body reaches a log line. Introspection keeps only the bounded RFC 6749
  §5.2 `error` code.
