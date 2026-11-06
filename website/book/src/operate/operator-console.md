<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# The operator console

The FerroBRIDGE Console is a separate binary, `ferrobridge-console`, that
serves a web console in front of the bridge. Its server calls the bridge
over the bridge's public HTTP surface with the signed-in operator's own
access token, so the bridge checks that token as it checks any other
client's. No specification governs the console: it is FerroBRIDGE's own
design.

## Configuring and checking it

The console reads its configuration the way the bridge does, through the
FerroHEALTH family's loader: its defaults, the file `--config` or
`FERROBRIDGE_CONSOLE_CONFIG` names, `FERROBRIDGE_CONSOLE__<SECTION>__<KEY>`
variables over it, and `--set key=value` pairs last. A secret is set inline
or through its `<key>_file` sibling, whose trailing line ending is removed.
`[telemetry]` is the family's console section, and `RUST_LOG` sets its
`filter`. `ferrobridge-console config check` prints the running configuration
with every secret as `***` and exits 78 when the console would not start.
`ferrobridge-console healthcheck` asks the console on loopback for
`GET /health/readiness` and exits 0 only on `200`; the image's `HEALTHCHECK`
runs it.

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

With `issuer` alone the console discovers the provider's endpoints when it
starts (OpenID Connect Discovery 1.0), and each endpoint you write takes the
place of the discovered one. `config check` never contacts the provider: it
checks the written keys, and builds the sign-in only when the authorization,
token and key-set endpoints are all written. `revocation_endpoint` names
where a sign-out revokes the operator's token (RFC 7009), `ca_file` adds PEM
trust roots for a provider behind a private certificate authority, and
`timeout_s` (default `10`) bounds one call to the provider.

The access token the provider issues is the one the bridge receives, so its
audience and scopes must be ones the bridge's `[auth]` accepts, the console's
client id must be listed in the issuer's `first_party_clients`, and the
operator API needs the operator scope (see
[The operator API](operator-api.md)).

## Sessions

The sign-in and the session are those of `ferrohealth-session`, the crate
every FerroHEALTH console signs in with. The browser holds one `HttpOnly`,
`SameSite=Lax` cookie, under the `__Host-` prefix, and never a token. With
`offline_access = true` the provider issues a refresh token, and the console
refreshes the access token on its server shortly before it expires. A
refresh the provider refuses ends the session, and the operator signs in
again; a provider that does not answer leaves the session for the next
request to try again. Without a refresh token the session ends when its
access token expires. `[session]` sets the store, the bounds and the
timeouts:

| Key | Default | Meaning |
|---|---|---|
| `store` | `"memory"` | `memory` keeps sessions in the console's process; `sealed` keeps each in one encrypted cookie |
| `key`, `key_file` | none | The sealing key of a `sealed` store: base64 of at least 64 bytes, the same on every replica; refused beside `memory` |
| `secure_cookie` | `true` | Cookies carry `Secure` and the `__Host-` prefix; `false` only for a console on `http` loopback |
| `sign_in_timeout_s` | `300` | How long a started sign-in waits for the provider |
| `max_sign_ins` | `1000` | Pending sign-ins a `memory` store holds; a new one drops the oldest |
| `idle_timeout_s` | `1800` | How long a session lives without activity |
| `absolute_timeout_s` | `43200` | How long a session lives from its sign-in; at least `idle_timeout_s` |
| `max_sessions` | `10000` | Sessions a `memory` store holds; a sign-in past it is refused |

A `memory` store loses its sessions on a restart, so every operator signs in
again, and replicas behind a load balancer need sticky sessions. A `sealed`
store needs neither, and refuses `offline_access`, which would put a refresh
token in a browser cookie.

A signed-in page carries FerroUI's session watch. It asks `GET /session`,
which reads what the session has left and never counts as activity, and
warns two minutes before the idle timeout with Stay signed in, which posts
to `/session` to extend it. When the session ends, every tab of the console
leaves for the sign-in, which signs in and comes back to the page the
operator was on (`/login?return=<page>`).

## The console's pages

A signed-in operator reads a top bar with the search, the bridge's status
and the user menu, and a sidebar grouped Overview, Mappings, Bridge and
Console (a drawer on a narrow screen, a rail when you fold it). The status
is a word as well as a colour: `Bridge up` with the version beside it,
`Bridge refused 401` when the bridge refuses the operator's token,
`Bridge unreachable`, or `Bridge unknown` with the reason for any other
answer. The Mapping set entry counts the errors of the running load.
The user menu holds Your preferences, the theme and Sign out.

The search finds contexts by name or resource type, mapping files by path
and templates by id, and never takes a patient identifier. As you type, a
panel opens under the field with the matches grouped by kind, five of each
at most; the arrow keys walk them, Enter opens the chosen one, and Enter
with none chosen, or "See all results", opens the search page, which lists
every match in the page.

The console draws every page on its server and sends it whole: a link,
the search, a tab or an action loads one complete page, and the browser's
back and forward buttons walk the pages you visited. A few small scripts
add behaviour on top, such as the editor's highlighting as you type, the
search's suggestions and the menus and dialogs, and every page and every
action works without them: each action is a plain form post that the
console answers with the page it drew.

- **Dashboard** (`/`): the running load's contexts, its errors and
  warnings, the upstreams the bridge reaches, what needs attention, and
  the flow from the callers through the faces and the mapping set to the
  upstreams (`GET /health/info`, `GET /operator/readiness` or, with the
  operator API off, `GET /health/readiness`,
  `GET /operator/state`).
- **Mapping set** (`/mappings`): the load report and six tabs. Contexts
  lists each context with its resource type, template, models, search
  declaration, owner and state; OMOCL, Templates, Files, Diagnostics and
  Revisions live at `/mappings/omocl`, `/mappings/templates`,
  `/mappings/files`, `/mappings/diagnostics` and `/mappings/revisions`.
  Revisions lists every recorded state of the console's files, newest
  first, with what recorded it, when, who and why
  (`GET /operator/revisions`). The Files tab filters by path, language and
  owner, and pages its rows. More holds Reload the set, which asks first,
  then compiles the files on disk and replaces the running set when they
  compile.
- **A mapping file** (`/mappings/files/{path}`): the file as the bridge
  serves it, byte for byte, with numbered lines, highlighted YAML, its
  SHA-256 and the diagnostics beside it, each linked to its line. The
  Program tab (`?tab=program`) shows what the context compiled to.
- **A template** (`/mappings/templates/{id}`): where the template came
  from and the contexts that name it.
- **A revision** (`/mappings/revisions/{id}`): who recorded it, when and
  why, every file that differs from the running revision with its lines as
  a unified diff, and the provisioned files, which are in no revision and
  read-only. A revision that does not run docks Roll back to this revision
  (Apply this revision for an imported one) with its reason; the bridge
  compiles the set with its files first, changes nothing when it does not
  compile, and refuses the apply when another one ran since the page drew
  (`POST /operator/revisions/{id}/apply`).
  Export as ZIP downloads the revision's console files
  (`GET /operator/revisions/{id}/export`).
- **Import a mapping set** (`/mappings/import`, from the Mapping set's More
  menu): a ZIP as an export wrote it, at most 2 MB, with the reason. The
  bridge checks the set with its files as a dry run and saves them as a
  revision that does not run, linked to its page, where Apply this
  revision runs it; a set that does not compile shows its diagnostics and
  nothing is saved. More also holds Export the running set.
- **The editor** (`/mappings/edit/{path}`, or `/mappings/new` for a new
  file): see below.
- **Playground** (`/playground`, `/playground/toopenehr`,
  `/playground/validate`): runs `$tofhir`, `$toopenehr` and `$validate` as
  the operator, and keeps nothing. On the `$tofhir` tab, Example holds one
  synthetic composition per template of the running set, generated from
  its Web Template (`GET /operator/templates/{id}/example`); choosing one
  loads it into the composition with its template chosen.
- **FHIR facade** (`/facade`, `/facade/capability`, `/facade/security`,
  `/facade/settings`): the facade's state, the resource types its
  `CapabilityStatement` serves, the SMART discovery document, and its
  settings.
- **HL7 v2** (`/hl7v2`, `/hl7v2/concept-maps`, `/hl7v2/settings`) and
  **OMOP ETL** (`/omop`, `/omop/runs`, `/omop/settings`): whether the MLLP
  listener runs and where, whether an ETL is configured, and each face's
  settings. The ConceptMaps tab lists every map the HL7 v2 face runs, from
  `GET /operator/hl7v2/concept-maps`: its title and id, its kind, what it
  maps from and to, whether it is the guide's or a supplement's, its rows
  and the messages it served since the bridge started. The OMOP ETL's Runs
  tab lists the runs the bridge schema keeps, from `GET /operator/etl/runs`,
  each with its job, when it ran and its counts, and links each to its
  report (`/omop/runs/{id}`): the compositions it read, the rows it wrote by
  table, what its deletion check found, and every record it refused with
  the reason.
- **Events** (`/events`): every change-feed consumer and the AMQP topology
  of the [events lane](../integrate/events.md), from `GET /operator/events`:
  each consumer's source and reader, its position in each stream (an outbox
  `seq` of the reference CDR, or the watermark the bridge keeps for the AQL
  source), its state, what it published since the bridge started, and its
  last failure with the cause, never an event body; then the events each
  consumer parked in the [dead-letter
  store](../integrate/change-feed.md#poisoned-events-and-the-dead-letter-store)
  with their cause and passes, each with a Replay button that posts
  `POST /operator/events/{consumer}/parked/{id}/replay` and comes back to
  the page, where the event reads "Replay requested" until its consumer
  offers it again; then the exchange, the routing-key grammar and the bound
  queues. The topology changes in the configuration. A bridge with no
  consumer says so in one line.
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

![The Revisions tab](img/console/mappings-revisions.png)

The Revisions tab after a save. [Dark theme](img/console/mappings-revisions-dark.png).

![A revision](img/console/revision.png)

The baseline revision with its diff against the running one and the
rollback docked. [Dark theme](img/console/revision-dark.png).

![The import page](img/console/mappings-import.png)

The import page. [Dark theme](img/console/mappings-import-dark.png).

![An imported archive](img/console/mappings-imported.png)

An archive checked and saved as a revision.
[Dark theme](img/console/mappings-imported-dark.png).

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

![An example composition](img/console/playground-example.png)

The playground with an example composition of the template.
[Dark theme](img/console/playground-example-dark.png).

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

The HL7 v2 face and its MLLP listener.
[Dark theme](img/console/hl7v2-dark.png).

![The HL7 v2 ConceptMaps](img/console/hl7v2-concept-maps.png)

The ConceptMaps the HL7 v2 face runs, with the messages each served.
[Dark theme](img/console/hl7v2-concept-maps-dark.png).

![The HL7 v2 settings](img/console/hl7v2-settings.png)

The HL7 v2 face's settings. [Dark theme](img/console/hl7v2-settings-dark.png).

![OMOP ETL](img/console/omop.png)

The OMOP ETL of a bridge with no ETL configured.
[Dark theme](img/console/omop-dark.png).

![The OMOP ETL settings](img/console/omop-settings.png)

The OMOP ETL's settings. [Dark theme](img/console/omop-settings-dark.png).

![The OMOP ETL runs](img/console/omop-runs.png)

The runs the bridge schema keeps. [Dark theme](img/console/omop-runs-dark.png).

![One OMOP ETL run](img/console/omop-run.png)

The report of one run, with the record it refused.
[Dark theme](img/console/omop-run-dark.png).

![Events](img/console/events.png)

The Events page of a bridge whose events lane is off.
[Dark theme](img/console/events-dark.png).

![A degraded events consumer](img/console/events-degraded.png)

The Events page of a bridge whose broker cannot be reached: the consumer
reads degraded with its cause, its cursor stays where it was, and an event
it parked waits for a replay.
[Dark theme](img/console/events-degraded-dark.png).

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

![The search page](img/console/search.png)

The search page, every match of the words searched for, one card per kind.
[Dark theme](img/console/search-dark.png).

![The About page](img/console/about.png)

The About page. [Dark theme](img/console/about-dark.png).

![The page for an unknown path](img/console/not-found.png)

The page for a path the console does not serve.
[Dark theme](img/console/not-found-dark.png).

## Signing out

Sign out, in the user menu, is a `POST` to `/logout`. It ends the session
and, when `end_session_endpoint` is set, sends the browser to the provider
with the ID Token hint (OpenID Connect RP-Initiated Logout 1.0); without
one it lands on `/signed-out`, whether or not the console has a provider.
When the provider has a revocation endpoint, the sign-out revokes the
operator's refresh token, or its access token when it has none (RFC 7009).
The console takes a `POST` only from its
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
