<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# ferrobridge-console

The FerroBRIDGE Console, the operator console: the `ferrobridge-console`
binary, a Leptos app rendered on the server and hydrated in the browser, with
a backend-for-frontend in front of the bridge.

Part of [FerroBRIDGE](https://ferrobridge.eu), a pure-Rust bridge between
openEHR and two interoperability targets: HL7 FHIR through the FHIRconnect
specification, and the OMOP Common Data Model through the OMOCL
specification. This crate is never published to crates.io.

The console is one more client of the bridge's public HTTP surface. Its
server calls the bridge over HTTP and never compiles or runs a mapping
itself. Built so far: the health route, the shell (a top bar with the
bridge's status and version, the navigation, the light, dark and system
themes, the About page), the engine-state screens (the dashboard, the
capability screen and the mappings overview), the mapping files shown
verbatim with their diagnostics, the mapping editor (check, save under
`If-Match` with a reason, reload), the transform playground and the
validation dry run, under the browser security
headers, operator
sign-in with OpenID Connect, the configuration
with its environment overrides and `_file` secrets, the typed bridge client
that calls the bridge with the operator's own access token, and the check
that holds the crate to the two workspace crates it may link.

## Sign-in

Operators sign in at an OpenID Provider with the authorization code grant
and PKCE (`S256`). `GET /login` starts a sign-in, the provider sends the
operator back to `/auth/callback`, and the console checks the ID Token
against the provider's JWK Set, its issuer, its audience and the sign-in's
`nonce` before it begins a session. The session lives on the console's
server; the browser holds one opaque `HttpOnly`, `SameSite=Lax` cookie,
under the `__Host-` prefix when it is `Secure`, and never a token. The
console refreshes the access token before it expires, one refresh per
session at a time, and keeps a rotated refresh token. `POST /logout` ends
the session and, when the provider publishes one, sends the operator to its
end-session endpoint with the ID Token hint. Every request other than
`GET`, `HEAD` and `OPTIONS` is taken only from the console's own pages.

## The subcommands

| Command | What it does |
|---|---|
| `ferrobridge-console serve` | Serves the console until the process is asked to stop |
| `ferrobridge-console config check` | Reads and resolves the configuration and exits, binding nothing |
| `ferrobridge-console healthcheck` | Asks the console on this host for `GET /health`; exits `0` only on `200` |

`--config <PATH>` names the configuration file, or `FERROBRIDGE_CONSOLE_CONFIG`
does. A refused configuration exits `78`.

## Configuration

```toml
[server]
listen = "0.0.0.0:3000"
site_root = "/site"

[bridge]
base_url = "https://bridge.example.org/"
timeout_ms = 30000

[session]
secure_cookie = true
sign_in_timeout_s = 300
max_sign_ins = 1000
idle_timeout_s = 1800
absolute_timeout_s = 43200
max_sessions = 10000

[oidc]
issuer = "https://idp.example.org/realms/ferrobridge"
authorization_endpoint = "https://idp.example.org/realms/ferrobridge/protocol/openid-connect/auth"
token_endpoint = "https://idp.example.org/realms/ferrobridge/protocol/openid-connect/token"
jwks_uri = "https://idp.example.org/realms/ferrobridge/protocol/openid-connect/certs"
client_id = "ferrobridge-console"
client_secret_file = "/run/secrets/console-client-secret"
redirect_uri = "https://console.example.org/auth/callback"
scopes = ["openid", "profile"]
offline_access = true
end_session_endpoint = "https://idp.example.org/realms/ferrobridge/protocol/openid-connect/logout"
post_logout_redirect_uri = "https://console.example.org/"
```

Without `[oidc]` the console offers no sign-in. The provider's endpoints
must be `https`, or `http` on loopback. `secure_cookie = false` is refused
unless `redirect_uri` is `http` on loopback. `offline_access = true` asks
the provider for a refresh token; list it there, never in `scopes`.

Every key can be set from the environment as
`FERROBRIDGE_CONSOLE__<SECTION>__<KEY>`, for example
`FERROBRIDGE_CONSOLE__BRIDGE__BASE_URL`, and the environment wins over the
file. A secret is set inline or read from the file its `<key>_file` sibling
names, never both.

## Building

Build it with cargo-leptos, from the repository root:

```sh
scripts/release/console-tools.sh "$HOME/.cache/ferrobridge-console-tools"
export PATH="$HOME/.cache/ferrobridge-console-tools/bin:$PATH"
scripts/release/console-site.sh --release
cargo build --release -p ferrobridge-console
```

The first line installs `wasm-opt` at its pinned release, checked against
its SHA-256, and the site script refuses to run cargo-leptos unless the one
on `PATH` matches those digests, so it never downloads it itself. That writes the site bundle under `target/site` and
the server binary under `target/release`.

## Licence

Business Source License 1.1 (`LICENSE`): free for every non-production use
and for non-commercial production use; a commercial licence for other
production use; Apache License 2.0 four years after each version.
