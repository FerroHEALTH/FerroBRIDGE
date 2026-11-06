- The flat `[auth] issuer = "…"` form, the section-level issuer settings
  (`audience`, `algorithms`, `clock_skew_seconds`, `jwks_max_age_seconds`,
  `jwks_refresh_interval_seconds`, `jwks_file`, `timeout_ms`, `mode`,
  `introspection`, `role_claims`), and `token_profile` at either level. Each
  fails `config check` naming its replacement. Upgrade step: write an
  `[[auth.issuer]]` table with its `kind` and move each key onto it; in place
  of `token_profile = "jwt"`, select a `profile` or set `token_types`,
  `required_claims` and the classification keys on the issuer table.
