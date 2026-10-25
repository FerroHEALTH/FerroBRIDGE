<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# The operator console

The FerroBRIDGE Console is a separate binary, `ferrobridge-console`, that
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
client_id = "ferrobridge-console"
client_secret_file = "/run/secrets/console-client-secret"
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

A signed-in operator reads a top bar with the search, the bridge's status
and the user menu, and a sidebar grouped Overview, Mappings, Bridge and
Console (a drawer on a narrow screen, a rail when you fold it). The status
is a word as well as a colour: `Bridge up` with the version beside it,
`Bridge refused 401` when the bridge refuses the operator's token,
`Bridge unreachable`, or `Bridge unknown` with the reason for any other
answer. The Mapping set entry counts the errors of the running load.
The user menu holds Your preferences, the theme and Sign out.

The console draws every page on its server and sends it whole. The
browser runs WebAssembly for the interactive parts alone (the menus, the
code editor, the JSON input), so every page and every action works before
that code arrives. Each action is a plain form post that the console
answers with the page it drew; the console registers no server function.

- **Dashboard** (`/`): the running load's contexts, its errors and
  warnings, the upstreams the bridge reaches, what needs attention, and
  the flow from the callers through the faces and the mapping set to the
  upstreams (`GET /health/info`, `GET /health/readiness`,
  `GET /operator/state`).
- **Mapping set** (`/mappings`): the load report and five tabs. Contexts
  lists each context with its resource type, template, models, search
  declaration, owner and state; OMOCL, Templates, Files and Diagnostics
  live at `/mappings/omocl`, `/mappings/templates`, `/mappings/files` and
  `/mappings/diagnostics`. The Files tab filters by path, language and
  owner, and pages its rows. More holds Reload the set, which asks first,
  then compiles the files on disk and replaces the running set when they
  compile.
- **A mapping file** (`/mappings/files/{path}`): the file as the bridge
  serves it, byte for byte, with numbered lines, highlighted YAML, its
  SHA-256 and the diagnostics beside it, each linked to its line. The
  Program tab (`?tab=program`) shows what the context compiled to.
- **A template** (`/mappings/templates/{id}`): where the template came
  from and the contexts that name it.
- **The editor** (`/mappings/edit/{path}`, or `/mappings/new` for a new
  file): see below.
- **Playground** (`/playground`, `/playground/toopenehr`,
  `/playground/validate`): runs `$tofhir`, `$toopenehr` and `$validate` as
  the operator, and keeps nothing.
- **FHIR facade** (`/facade`, `/facade/capability`, `/facade/security`,
  `/facade/settings`): the facade's state, the resource types its
  `CapabilityStatement` serves, the SMART discovery document, and its
  settings.
- **HL7 v2** (`/hl7v2`, `/hl7v2/settings`) and **OMOP ETL** (`/omop`,
  `/omop/settings`): whether the MLLP listener runs and where, whether an
  ETL is configured, and each face's settings.
- **Connections** (`/connections`, `/connections/{name}`,
  `/connections/{name}/settings`): each upstream with its host, how it
  signs in, the lanes that use it and its state from the bridge's
  readiness, and its settings with its credentials by reference.
- **Configuration** (`/configuration`, `/configuration/mappings`,
  `/configuration/core`): the settings no face or connection owns:
  logging, the mapping directories, and the server and who may call it.
- **Your preferences** (`/preferences`, `/preferences/language`,
  `/preferences/keyboard`): appearance, language and region, and keyboard
  hints.
- **About** (`/about`): the console's and the bridge's versions, the
  licence and the maker, Cadasto B.V.

The editor changes the console's own mapping files, which live under the
bridge's `[operator] mapping_dir`:

1. **Check** compiles the whole set with your draft in place of the file
   and lists every diagnostic. Nothing is written.
2. **Save** needs a one-line reason and a confirmation in the bar at the
   foot of the page. The bridge compiles the set with the draft again and
   writes the file only when it compiles, then reloads, so a refused draft
   leaves the disk and the running set as they were. The save carries the
   digest the file was opened at: when someone changed the file in the
   meantime, the bridge refuses with `412` and your draft stays in the
   editor. The draft is saved with LF line ends, whatever line ends the
   browser posts.

A provisioned file opens for a check only, or to save under a path of its
own: the console never saves over one. The bridge's audit log names who
saved which file, the two digests and the reason, never the content.

What you enter in the playground goes to the bridge in the request body
alone, never in an address or the browser's history, and the console logs
none of it. A `$toopenehr` answer shows the composition, with the issues
of the `OperationOutcome` the bridge returned beside it above the first
line. When the bridge's operations lane is off, the playground says so and
its run button is disabled.

The mapping counts, the Mapping set, the files and the editor read the
operator API, so they need `[operator] enabled = true` on the bridge and
an operator token that carries the operator scope. Without either, those
sections say the operator API is off, or show the bridge's refusal with
its status, while the rest of the page still draws.

Every Settings tab and the Configuration page read
`GET /operator/configuration`, the configuration the bridge started on,
so they are read-only: each row shows its value, its TOML key on hover,
and whether the value came from the environment or is the default. A
secret shows only whether it is set and the file it is read from, never
its value. How configuration works, at the end of the line that names the
file, says where each setting is shown and that a change is made in the
file and takes effect at the next start.

Your preferences keeps each choice in a cookie of its own for a year
(`ferrobridge_console_theme`, `_density`, `_motion`, `_language`,
`_week-start` and `_hints`, under the `__Host-` prefix when the cookies
are secure), and the server draws each page in them, so nothing is kept
in the browser's storage. Reset to defaults clears them all. The theme
offers light, dark and system, which follows the operating system, and is
set in the user menu and in Your preferences alone.

## Every screen

Each screen below is shown in the light theme at 1440 pixels wide, with a
link to the same screen in the dark theme. The browser journeys take them
against a running bridge with a synthetic mapping set, so what you read is
what the console draws.

### Signing in and out

![The sign-in card a visitor reads](img/console/sign-in.png)

The sign-in card a visitor reads. [Dark theme](img/console/sign-in-dark.png).

![The signed-out card](img/console/signed-out.png)

The card a signed-out operator reads.
[Dark theme](img/console/signed-out-dark.png).

### Dashboard

![The dashboard](img/console/dashboard.png)

The dashboard with the flow. How it is drawn, in the flow's head, opens
a short dialog with each state's line and what it means.
[Dark theme](img/console/dashboard-dark.png).

### Mapping set

![The Mapping set](img/console/mappings.png)

The Mapping set on its Contexts tab.
[Dark theme](img/console/mappings-dark.png).

![The OMOCL tab](img/console/mappings-omocl.png)

The OMOCL tab. [Dark theme](img/console/mappings-omocl-dark.png).

![The Templates tab](img/console/mappings-templates.png)

The Templates tab. [Dark theme](img/console/mappings-templates-dark.png).

![The Files tab](img/console/mappings-files.png)

The Files tab with its filters. [Dark theme](img/console/mappings-files-dark.png).

![The Diagnostics tab](img/console/mappings-diagnostics.png)

The Diagnostics tab. [Dark theme](img/console/mappings-diagnostics-dark.png).

![A reload of the set](img/console/mappings-reloaded.png)

The set after a reload. [Dark theme](img/console/mappings-reloaded-dark.png).

![A context mapping file](img/console/file-context.png)

A provisioned context mapping file with its diagnostics.
[Dark theme](img/console/file-context-dark.png).

![The program of a context](img/console/file-program.png)

The program a context compiled to.
[Dark theme](img/console/file-program-dark.png).

![A model mapping file](img/console/file-model.png)

A model mapping file. [Dark theme](img/console/file-model-dark.png).

![A template](img/console/template.png)

A template and the contexts that name it.
[Dark theme](img/console/template-dark.png).

### The editor

![A new file in the editor](img/console/edit-new.png)

A new console file. [Dark theme](img/console/edit-new-dark.png).

![A provisioned file in the editor](img/console/edit-provisioned.png)

A provisioned file, open to check or to save under a path of its own.
[Dark theme](img/console/edit-provisioned-dark.png).

![A draft whose check found errors](img/console/edit-checked.png)

A draft the set does not compile with, and its save blocked.
[Dark theme](img/console/edit-checked-dark.png).

![The confirmation of a save](img/console/edit-confirm.png)

The confirmation a save asks for. [Dark theme](img/console/edit-confirm-dark.png).

![A draft the bridge refused](img/console/edit-refused.png)

A draft the bridge refused to save, with the running set unchanged.
[Dark theme](img/console/edit-refused-dark.png).

![A draft the bridge saved](img/console/edit-saved.png)

A draft the bridge saved and reloaded the set with.
[Dark theme](img/console/edit-saved-dark.png).

### Playground

![The playground](img/console/playground.png)

The playground on its `$tofhir` tab. [Dark theme](img/console/playground-dark.png).

![The $toopenehr tab](img/console/playground-toopenehr.png)

The `$toopenehr` tab. [Dark theme](img/console/playground-toopenehr-dark.png).

![The validate tab](img/console/playground-validate.png)

The `$validate` tab. [Dark theme](img/console/playground-validate-dark.png).

![A conversion the bridge answered](img/console/playground-answered.png)

A `$toopenehr` the bridge answered, with its outcome's issues.
[Dark theme](img/console/playground-answered-dark.png).

![A conversion the bridge refused](img/console/playground-refused.png)

A `$tofhir` the bridge refused, with its status.
[Dark theme](img/console/playground-refused-dark.png).

![A bridge whose operations lane is off](img/console/playground-off.png)

A bridge whose operations lane is off.
[Dark theme](img/console/playground-off-dark.png).

### Bridge

![The FHIR facade](img/console/facade.png)

The FHIR facade. [Dark theme](img/console/facade-dark.png).

![The facade's capability](img/console/facade-capability.png)

The resource types the facade serves.
[Dark theme](img/console/facade-capability-dark.png).

![The facade's security](img/console/facade-security.png)

The SMART discovery document. [Dark theme](img/console/facade-security-dark.png).

![The facade's settings](img/console/facade-settings.png)

The facade's settings. [Dark theme](img/console/facade-settings-dark.png).

![HL7 v2](img/console/hl7v2.png)

The HL7 v2 face of a bridge that runs no listener.
[Dark theme](img/console/hl7v2-dark.png).

![The HL7 v2 settings](img/console/hl7v2-settings.png)

The HL7 v2 face's settings. [Dark theme](img/console/hl7v2-settings-dark.png).

![OMOP ETL](img/console/omop.png)

The OMOP ETL of a bridge with no ETL configured.
[Dark theme](img/console/omop-dark.png).

![The OMOP ETL settings](img/console/omop-settings.png)

The OMOP ETL's settings. [Dark theme](img/console/omop-settings-dark.png).

![Connections](img/console/connections.png)

The upstreams. [Dark theme](img/console/connections-dark.png).

![A connection](img/console/connection-cdr.png)

The openEHR CDR. [Dark theme](img/console/connection-cdr-dark.png).

![A connection's settings](img/console/connection-cdr-settings.png)

The openEHR CDR's settings, its credentials by reference.
[Dark theme](img/console/connection-cdr-settings-dark.png).

### Console

![Configuration](img/console/configuration.png)

Configuration, Logging. [Dark theme](img/console/configuration-dark.png).

![The Mappings tab of Configuration](img/console/configuration-mappings.png)

Configuration, Mappings. [Dark theme](img/console/configuration-mappings-dark.png).

![The Server and access tab of Configuration](img/console/configuration-core.png)

Configuration, Server and access.
[Dark theme](img/console/configuration-core-dark.png).

![Your preferences](img/console/preferences.png)

Your preferences, Appearance. [Dark theme](img/console/preferences-dark.png).

![Language and region](img/console/preferences-language.png)

Your preferences, Language and region.
[Dark theme](img/console/preferences-language-dark.png).

![The About page](img/console/about.png)

The About page. [Dark theme](img/console/about-dark.png).

![The page for an unknown path](img/console/not-found.png)

The page for a path the console does not serve.
[Dark theme](img/console/not-found-dark.png).

## Signing out

Sign out, in the user menu, is a `POST` to `/logout`. It ends the session
and, when `end_session_endpoint` is set, sends the browser to the provider
with the ID Token hint (OpenID Connect RP-Initiated Logout 1.0); without
one it lands on `/signed-out`. The console takes a `POST` only from its
own pages, by the browser's `Sec-Fetch-Site` header, or by `Origin` when
the browser sends no fetch metadata.

## Upgrading from `ferrobridge-viewer`

Up to 0.0.10 the console was named `ferrobridge-viewer`. From 0.0.11 it is
`ferrobridge-console`, and the old image is no longer built. When you
upgrade, change each old name to the new one:

| What | Up to 0.0.10 | From 0.0.11 |
|---|---|---|
| Image | `ghcr.io/ferrohealth/ferrobridge-viewer` | `ghcr.io/ferrohealth/ferrobridge-console` |
| Binary | `ferrobridge-viewer` | `ferrobridge-console` |
| Configuration file variable | `FERROBRIDGE_VIEWER_CONFIG` | `FERROBRIDGE_CONSOLE_CONFIG` |
| Environment prefix | `FERROBRIDGE_VIEWER__` | `FERROBRIDGE_CONSOLE__` |
| Compose profile and service | `viewer` | `console` |
| Compose secret file | `secrets/viewer_oidc_client_secret` | `secrets/console_oidc_client_secret` |
| Cookies | `ferrobridge_viewer_*` | `ferrobridge_console_*` |

The cookies change with the name, so every operator signs in again after
the upgrade.

The 0.0.11 console is also drawn anew on FerroUI 0.3.6, and its addresses
moved with it. A bookmark to an old address answers the page-not-found
page; follow its links, or use the new address:

| Up to 0.0.10 | From 0.0.11 |
|---|---|
| `/files` | `/mappings/files` |
| `/files/{path}` | `/mappings/files/{path}` |
| `/edit` | `/mappings/new` |
| `/edit/{path}` | `/mappings/edit/{path}` |
| `/validate` | `/playground/validate` |
| `/capability` | `/facade/capability` and `/facade/security` |

The theme toggle left the top bar: set the theme in the user menu or in
Your preferences. Each preference, the theme among them, is kept in a
cookie of its own for a year, so every operator chooses the theme again
after the upgrade. A sign-out with no `end_session_endpoint` lands on `/signed-out`
instead of the sign-in.
