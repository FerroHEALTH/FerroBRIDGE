- The bridge and `ferrobridge-term` 0.1.147 take `ferrohealth-smart` 0.12.0,
  the operator console `ferrohealth-session` 0.6.0, and the server
  `ihe-atna` and `ehds-logging` 0.7.1, whose FHIR Feed bearer is on the same
  line, so the lock holds one `ferrohealth-smart`. `[cdr.credentials.onward]`
  and each `[[cdr.onward]]` table take `cache`: `false` holds no onward
  token, so every CDR call exchanges one at the issuer, with a warning at
  start, for a performance measurement and never for production. They also
  take `actor_assertion` (default `false`): `true` names the bridge's client
  as the actor of an exchange sent without an actor token, with an
  assertion signed by the `oauth` key. Under the `keycloak` profile, a
  Keycloak 26.8 impersonation token, which carries an RFC 8693 `act` claim,
  is admitted as delegated only when the issuer's `delegates` lists the
  impersonator. A product that hands its `TokenSource` to `ferrobridge-term`
  moves to `ferrohealth-smart` 0.12 with it.
