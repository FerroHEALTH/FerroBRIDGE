- The operator console signs operators in and keeps their sessions through
  `ferrohealth-session` 0.1.1, the sign-in crate of every FerroHEALTH
  console, in place of its own copy. The routes stay `GET /login`,
  `GET /auth/callback` and `POST /logout`; `GET /login` takes
  `?return=<page>` and a completed sign-in lands on that page of the
  console, and the sign-in card's button carries the page the operator
  asked for. `[oidc]` takes `issuer` alone and discovers
  `authorization_endpoint`, `token_endpoint`, `jwks_uri`,
  `end_session_endpoint` and the new `revocation_endpoint` when the console
  starts, each written one taking the place of the discovered one, with the
  new keys `ca_file` and `timeout_s` (default `10`); `config check`
  contacts no provider. `[session]` adds `store` (`memory`, the default, or
  `sealed`, one encrypted cookie per session) with `key` or `key_file`, and
  refuses an `absolute_timeout_s` shorter than `idle_timeout_s`. A sign-out
  revokes the operator's token at a provider that has a revocation
  endpoint, and lands on `/signed-out` on a console with no provider in
  place of a `503`. A session without a refresh token ends when its access
  token expires. A cookie that names no live session is answered with its
  removal. The cookie names are unchanged.
  Upgrade: no key is renamed; a configuration whose `absolute_timeout_s` is
  below its `idle_timeout_s` is refused at start and needs one of the two
  changed, and `config check` reports it.
