- The bridge and `ferrobridge-term` 0.1.146 take `ferrohealth-smart` 0.10.1,
  and the server takes `ihe-atna` and `ehds-logging` 0.6.0, whose FHIR Feed
  bearer is on the same line. `[cdr.credentials.onward]` and each
  `[[cdr.onward]]` table take `cache`: `false` holds no onward token, so
  every CDR call exchanges one at the issuer, with a warning at start, for a
  performance measurement and never for production. Under the `keycloak`
  profile, a Keycloak 26.8 impersonation token, which carries an RFC 8693
  `act` claim, is admitted as delegated only when the issuer's `delegates`
  lists the impersonator. A product that hands its `TokenSource` to
  `ferrobridge-term` moves to `ferrohealth-smart` 0.10 with it.
