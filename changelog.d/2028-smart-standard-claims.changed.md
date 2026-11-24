- Breaking: the bridge trusts an authorization server, FerroSMART among
  them, with an `[auth.issuer.<name>]` table that names only its `issuer`,
  and reads every caller from the token's standard claims, as
  `ferrohealth-smart` 0.34 does: a client credentials token, whose `sub` is
  its `client_id`, is a machine, and any other token is a user named by the
  resource type of its `fhirUser`, with its `patient` claim as the launch
  context and an RFC 8693 `act` as the actor. The authorization server's
  client registry is the one list of clients: a token whose `aud` names the
  bridge admits its client. A token's `typ` is `at+jwt` or
  `application/at+jwt` and `scope` is required. The onward lane carries
  every user at its SMART issuer and another party's professional at its
  `federation` issuer, and the onward grant is set by the onward table's
  profile keys (`onward_grant`, `onward_indicator`, `onward_parameters`,
  `onward_client_id`). The crates move to `ferrobridge-term` and
  `ferrobridge-validator` 0.18.0 and `ferrobridge-hl7v2` and `eehrxf` 0.26.0,
  and the server and the console to `ferrohealth-smart` 0.34.0 and
  `ferrohealth-session` 0.29.0. Upgrade: delete every key listed under
  Removed and keep `kind = "federation"` only on another party's issuer.
