<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# The operator console

The FerroBRIDGE Console is a separate binary, `ferrobridge-viewer`, that
serves a web console in front of the bridge. Its server calls the bridge
over the bridge's public HTTP surface with the signed-in operator's own
access token, so the bridge checks that token as it checks any other
client's. No specification governs the console: it is FerroBRIDGE's own
design.

## Signing in

Operators sign in at your OpenID Provider. The console is an OAuth 2.0
client that uses the authorization code grant with PKCE (`S256`, RFC 7636)
and checks the ID Token against the provider's JWK Set, its issuer, its
audience and the sign-in's `nonce` (OpenID Connect Core 1.0 §3.1.3.7).
Register the console at the provider with:

- the redirect URI `https://<console>/auth/callback`;
- the post-logout redirect URI, when you set one;
- a client secret, when you run the console as a confidential client.

Then configure `[oidc]`:

```toml
[oidc]
issuer = "https://idp.example.org/realms/ferrobridge"
authorization_endpoint = "https://idp.example.org/realms/ferrobridge/protocol/openid-connect/auth"
token_endpoint = "https://idp.example.org/realms/ferrobridge/protocol/openid-connect/token"
jwks_uri = "https://idp.example.org/realms/ferrobridge/protocol/openid-connect/certs"
client_id = "ferrobridge-viewer"
client_secret_file = "/run/secrets/viewer-client-secret"
redirect_uri = "https://console.example.org/auth/callback"
offline_access = true
end_session_endpoint = "https://idp.example.org/realms/ferrobridge/protocol/openid-connect/logout"
post_logout_redirect_uri = "https://console.example.org/"
```

The access token the provider issues is the one the bridge receives, so its
audience and scopes must be ones the bridge's `[auth]` accepts, and the
operator API needs the operator scope (see
[The operator API](operator-api.md)).

## Sessions

The session lives on the console's server. The browser holds one opaque
`HttpOnly`, `SameSite=Lax` cookie, under the `__Host-` prefix, and never a
token. With `offline_access = true` the provider issues a refresh token,
and the console refreshes the access token on its server shortly before it
expires. A refresh the provider refuses ends the session, and the operator
signs in again; a provider that does not answer leaves the session for the
next request to try again. `[session]` bounds the pools and sets the
timeouts:

| Key | Default | Meaning |
|---|---|---|
| `secure_cookie` | `true` | Cookies carry `Secure` and the `__Host-` prefix; `false` only for a console on `http` loopback |
| `sign_in_timeout_s` | `300` | How long a started sign-in waits for the provider |
| `max_sign_ins` | `1000` | Pending sign-ins held at once; a new one drops the oldest |
| `idle_timeout_s` | `1800` | How long a session lives without a request |
| `absolute_timeout_s` | `43200` | How long a session lives at most |
| `max_sessions` | `10000` | Sessions held at once; a sign-in past it is refused |

Sessions are held in the console's memory, so a restart signs every
operator out, and replicas behind a load balancer need sticky sessions.

## The console's pages

A signed-in operator reads a top bar with the bridge's status and version,
the navigation grouped Overview, Mappings, Tools and Bridge (a drawer on a
narrow screen), and an operator menu with the sign-out. The status is a
word as well as a colour: `Bridge UP` with the version, `Bridge refused`
with the status the bridge answered, or `Bridge DOWN` when the console
cannot reach it. The About page names the console's version and its maker,
Cadasto B.V.

The theme switch offers light, dark and system, which follows the
operating system. The choice is kept in a cookie and the server draws each
page in it, so nothing is kept in the browser's storage. Both themes meet
WCAG 2.2 AA contrast.

## Signing out

The console's sign-out is a `POST` to `/logout`. It ends the session and,
when `end_session_endpoint` is set, sends the browser to the provider with
the ID Token hint (OpenID Connect RP-Initiated Logout 1.0). The console
takes a `POST` only from its own pages, by the browser's `Sec-Fetch-Site`
header, or by `Origin` when the browser sends no fetch metadata.
