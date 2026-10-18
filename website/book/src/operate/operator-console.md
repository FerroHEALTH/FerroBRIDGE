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

Three screens show the engine state:

- **Dashboard** (`/`): the bridge's version, build facts and pins
  (`GET /health/info`), its lanes and whether each listener is up, its
  readiness indicators (`GET /health/readiness`), and the counts of the
  contexts, models, templates and OMOCL files its running load compiled,
  with its errors and warnings (`GET /operator/state`).
- **Capability** (`/capability`): the `CapabilityStatement` the facade
  serves, as cards per resource, and the SMART discovery document.
- **Mappings overview** (`/mappings`): each context with its template,
  profile, resource type, models, extensions and search declaration, the
  templates and where each came from, the OMOCL files, and every
  diagnostic.

The **Files** page (`/files`) lists every mapping file the bridge reads,
with its owner (provisioned, or saved by the console), whether the console
may write it, and how many diagnostics it has. Following one shows the file
as the bridge serves it, byte for byte, with numbered lines, highlighted
YAML, its SHA-256, and the diagnostics beside it, each linked to its line.

The **editor** (`/edit/{path}`, or `/edit` for a new file) changes the
console's own mapping files, which live under the bridge's
`[operator] mapping_dir`:

1. **Check** compiles the whole set with your draft in place of the file
   and lists every diagnostic. Nothing is written.
2. **Save** needs a one-line reason and a confirmation. The bridge compiles
   the set with the draft again and writes the file only when it compiles,
   then reloads, so a refused draft leaves the disk and the running set as
   they were. The save carries the digest the file was opened at: when
   someone changed the file in the meantime, the bridge refuses with `412`
   and your draft stays in the editor.
3. **Reload** compiles the files on disk and replaces the running set when
   they compile.

A provisioned file opens for a check only: the console never saves over
one. The bridge's audit log names who saved which file, the two digests and
the reason, never the content.

Two tools run the bridge's operations as the operator, and keep nothing:

- **Transform playground** (`/playground`): `$tofhir` turns a canonical or
  FLAT composition into the `Bundle` the running programs map it to, with
  an optional template id and `context.patient` reference. `$toopenehr`
  turns a `Bundle` into a canonical or FLAT composition.
- **Validation dry run** (`/validate`): `$validate` on a resource type the
  `CapabilityStatement` declares it on, which commits nothing. The issues
  appear errors first.

What you enter goes to the bridge in the request body alone, never in an
address or the browser's history, and the console logs none of it. The
operations lane must be on for the playground; when it is off, the
playground shows the bridge's `503` and the reason the bridge gave.

The mapping counts, the overview and the files read the operator API, so they need
`[operator] enabled = true` on the bridge and an operator token that
carries the operator scope. Without either, those sections show the
bridge's refusal, its status and its diagnostics, while the rest of the
screen still draws.

The theme switch offers light, dark and system, which follows the
operating system. The choice is kept in a cookie and the server draws each
page in it, so nothing is kept in the browser's storage. Both themes meet
WCAG 2.2 AA contrast.

## Every screen

Each screen below is shown in the light theme, with a link to the same
screen in the dark theme. The browser journeys take them against a running
bridge with a synthetic mapping set, so what you read is what the console
draws.

### Signing in

![The sign-in card a visitor reads](img/console/sign-in.png)

The sign-in card a visitor reads. [Dark theme](img/console/sign-in-dark.png).

### Engine state

![The dashboard](img/console/dashboard.png)

The dashboard: the bridge, its lanes, its pins, its readiness and its
mapping counts. [Dark theme](img/console/dashboard-dark.png).

![The mappings overview](img/console/mappings.png)

The mappings overview: each context, the templates, the OMOCL files and the
diagnostics. [Dark theme](img/console/mappings-dark.png).

![The capability](img/console/capability.png)

The `CapabilityStatement` and the SMART discovery document.
[Dark theme](img/console/capability-dark.png).

### Mapping files

![The file list](img/console/files.png)

Every mapping file the bridge reads. [Dark theme](img/console/files-dark.png).

![A context mapping file](img/console/file-context.png)

A context mapping file, with its lines and its diagnostics.
[Dark theme](img/console/file-context-dark.png).

![A model mapping file](img/console/file-model.png)

A model mapping file. [Dark theme](img/console/file-model-dark.png).

### The editor

![A new file in the editor](img/console/edit-new.png)

A new console file. [Dark theme](img/console/edit-new-dark.png).

![A provisioned file in the editor](img/console/edit-provisioned.png)

A provisioned file, which the console checks and never saves over.
[Dark theme](img/console/edit-provisioned-dark.png).

![A draft the bridge refused](img/console/edit-refused.png)

A draft that does not compile: nothing is written and the running set is
unchanged. [Dark theme](img/console/edit-refused-dark.png).

![A draft the bridge saved](img/console/edit-saved.png)

A saved draft, with the reload's report and the file's new SHA-256.
[Dark theme](img/console/edit-saved-dark.png).

### Tools

![The transform playground](img/console/playground.png)

The transform playground. [Dark theme](img/console/playground-dark.png).

![A transform the bridge answered](img/console/playground-answered.png)

A `$toopenehr` the bridge answered with a composition.
[Dark theme](img/console/playground-answered-dark.png).

![A transform the bridge refused](img/console/playground-refused.png)

A `$tofhir` the bridge refused, with its status and its reason.
[Dark theme](img/console/playground-refused-dark.png).

![A bridge whose operations lane is off](img/console/playground-off.png)

A bridge whose operations lane is off answers `503`.
[Dark theme](img/console/playground-off-dark.png).

![The validation dry run](img/console/validate.png)

The validation dry run over the types the `CapabilityStatement` declares
`$validate` on. [Dark theme](img/console/validate-dark.png).

### Elsewhere

![The About page](img/console/about.png)

The About page. [Dark theme](img/console/about-dark.png).

![The page for an unknown path](img/console/not-found.png)

The page for a path the console does not serve, answered `404`.
[Dark theme](img/console/not-found-dark.png).

## Signing out

The console's sign-out is a `POST` to `/logout`. It ends the session and,
when `end_session_endpoint` is set, sends the browser to the provider with
the ID Token hint (OpenID Connect RP-Initiated Logout 1.0). The console
takes a `POST` only from its own pages, by the browser's `Sec-Fetch-Site`
header, or by `Origin` when the browser sends no fetch metadata.
