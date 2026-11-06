- Every trusted authorization server is an `[[auth.issuer]]` table that
  requires `kind` (`persons`, `patients`, `machines` or `federation`) and
  holds every issuer setting itself: `audience`, `algorithms`,
  `clock_skew_seconds`, the `jwks_*` keys, `timeout_ms`, `mode` with its
  `[auth.issuer.introspection]` table, and `role_claims`. Each table takes a
  `profile` (`rfc9068` by default, `entra`, `keycloak`, `okta` or `nl`) with
  any profile key set over it, and a person's token of a `persons` issuer is
  admitted only through a client listed in `first_party_clients` or
  `delegates`. An issuer with `mode = "introspection"` now sits beside
  issuers checked by their key sets. Upgrade step: write each issuer as an
  `[[auth.issuer]]` table with its `kind`, move every section-level issuer key
  onto each table it held for, list the clients your people sign in with
  (the operator console's included) in `first_party_clients`, and under the
  `keycloak` preset list each service account's `sub` in
  `app_only = [{ subjects = [...] }]`. `config check` names the key to move.
- `[auth] scopeless_subjects` entries are tables `{ issuer, sub }` and admit
  a machine's token alone. Upgrade step: rewrite each string entry, bare or
  `<issuer>#<sub>`, as a table naming its issuer.
- The audit line of every facade and operator request carries three fields
  more: the caller's `kind` (`person`, `patient`, `system`, `delegated` and
  the other kinds of `ferrohealth-smart`), and `binding` and `party`, which
  read `-` while the bridge binds no party. Upgrade step: a log pipeline that
  holds the field set adds the three.
- The quickstart `compose.yaml` mounts `./config` at `/etc/ferrobridge`
  read-only and passes `FERROBRIDGE_CONFIG` through, so the `[[auth.issuer]]`
  tables live in `config/ferrobridge.toml`; it no longer passes
  `FERROBRIDGE__AUTH__ISSUER` and `FERROBRIDGE__AUTH__AUDIENCE`. The operator
  console's Server and access tab counts the issuers and drops the Audience
  row, which is now each issuer's.
