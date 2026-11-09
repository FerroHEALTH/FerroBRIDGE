- The `[auth]`, `[<lane>.credentials]` and console `[oidc]` and `[session]`
  tables take the keys of `ferrohealth-smart` 0.15 and
  `ferrohealth-session` 0.9: every span of time is a `Duration` with no unit
  in its key (`clock_skew`, `jwks_max_age`, `jwks_refresh_interval`,
  `timeout`, `introspection.cache_max_age`, `[auth.basic] cache_ttl`,
  `refresh_margin`, `previous_key_overlap`, `sign_in_timeout`,
  `idle_timeout`, `absolute_timeout`, and `max_onward_lifetime` such as
  `"1h"`), and the outbound TLS of each is one `tls` table
  (`trust_roots_file`, `client_identity_file` or `client_identity`,
  `min_version`) in place of `ca_file`, `client_certificate_file` and
  `client_key_file`. The old keys are refused as unknown keys.
