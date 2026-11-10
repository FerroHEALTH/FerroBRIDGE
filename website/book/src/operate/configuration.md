<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# Configuring the server

FerroBRIDGE reads its defaults, an optional TOML file, environment variables
over it, and the `--set key=value` pairs of the command line last, through the
FerroHEALTH family's loader (`ferrohealth-config`), so every FerroHEALTH
product reads its configuration the same way. Nothing else configures the
process. No specification governs this; it is FerroBRIDGE's own design.

<!-- toc -->

## Where the file comes from

`ferrobridge serve --config /etc/ferrobridge/config.toml` names the file. With
no flag, `FERROBRIDGE_CONFIG` names it. With neither, the defaults below stand
and the server serves with no upstream configured. `--set` sets one key over
the file and the environment, and repeats:

```sh
ferrobridge serve --set server.listen=0.0.0.0:8080 --set telemetry.format=json
```

## Checking a configuration before a restart

`ferrobridge config check` reads the configuration exactly as `serve` would,
prints the running tree one key per line with where each value came from (the
default, the file, the environment or a `--set`), with every secret shown as
`***`, and then resolves it as `serve` does. It exits 0 when `serve` would
start, and 78 with every refusal on stderr when it would not:

```sh
ferrobridge config check --config /etc/ferrobridge/config.toml
```

## How an environment variable maps to a key

Prefix `FERROBRIDGE__`, then the dotted key with two underscores between
levels. Case does not matter; single underscores inside a key name stay as
they are.

```text
FERROBRIDGE__SERVER__LISTEN            -> [server] listen
FERROBRIDGE__CDR__ENDPOINT__URL             -> [cdr] base_url
FERROBRIDGE__CDR__ENDPOINT__RETRY__MAX_ATTEMPTS  -> [cdr.endpoint.retry] max_attempts
```

A value takes the type of the key it sets. A string key takes the text as it
is, whatever its characters: `FERROBRIDGE__CDR__ENDPOINT__CREDENTIALS__USER=12345` sets
the user name `12345`, and `true` or `1.5` stay text too. Quotes are part of
the value, so do not add any. A number, boolean or array key reads the text as
TOML syntax: `5` is the number 5, `true` is a boolean and `["a","b"]` is an
array. Text that does not read as the key's type is refused at boot, naming
the key.

## The variables

Every key below is settable in the file and as `FERROBRIDGE__<SECTION>__<KEY>`.
A key marked secret also accepts a `<key>_file` sibling naming a file the
server reads once at boot, with its trailing line ending removed; an empty
file is refused.

### `[server]`

The section is the FerroHEALTH family's (`ferrohealth-server`).

| Key | Default | Meaning |
|---|---|---|
| `listen` | `127.0.0.1:8080` | The socket address to bind |
| `base_path` | empty | The path every route is served under: empty, or starting with `/` and not ending with one |
| `request_timeout` | `"30s"` | How long one request may take before the server answers `408` (`504` under `/fhir`) |
| `header_read_timeout` | `"30s"` | How long a client may take to send a request head, counted from accept or from the first byte of each later head on a kept-alive connection; a client that has not sent one by then, one that sent nothing included, is disconnected |
| `idle_timeout` | `"2min"` | How long a kept-alive HTTP/1.1 connection may wait for its next request before the server closes it with a clean end of stream |
| `body_limit` | `"1MiB"` | The largest request body read before the server answers `413` |
| `drain_delay` | `"0s"` | How long readiness stays withdrawn after `SIGTERM` or `SIGINT` before the listener closes |
| `shutdown_timeout` | `"10s"` | How long the requests in flight may take once the listener closed |
| `max_concurrent_requests` | unset | How many requests the server serves at once; past it a request is answered `503` with `Retry-After`, and the health routes are never refused |
| `retry_after` | `"1s"` | The seconds a refused request is told to wait |
| `trusted_proxies` | `[]` | The reverse proxies, each an address or a CIDR block, whose forwarded client address is believed |
| `forwarded_header` | `forwarded` | The header a trusted proxy names the client in: `forwarded` or `x-forwarded-for` |
| `compression` | `true` | Whether a response is compressed with gzip or Brotli when the client accepts one |
| `compression_min_size` | `"1KiB"` | The smallest response body that is compressed |

`[server.tls]` serves the listener over TLS: `certificate_file` and
`key_file` (PEM), `client_auth` (`none`, `optional` or `required`) with
`client_ca_file`, `reload_interval` (unset, only `SIGHUP` reads the files
again), `healthcheck_identity_file` for `ferrobridge healthcheck` against a
listener that asks a client certificate, and `min_version` (`"1.3"` unless
lowered to `"1.2"`). Without it the listener speaks plain HTTP behind a proxy
that terminates TLS. `[server.caller_rate]` is refused: the bridge limits no
caller's rate; bound the server with `max_concurrent_requests`.

### `[telemetry]`

| Key | Default | Meaning |
|---|---|---|
| `format` | `auto` | `auto`, `json` or `pretty`; `auto` is `pretty` on a terminal and `json` otherwise |
| `filter` | `info,hyper=warn,tower=warn,h2=warn` | A `RUST_LOG`-style directive; a directive that does not parse refuses the start, naming the key |
| `logged_query_parameters` | `[]` | The query parameter names whose values may reach the request log, each value cut at 64 characters |
| `environment` | unset | The deployment environment, such as `production` |

The section is the FerroHEALTH family's (`ferrohealth-telemetry`).

Nothing else about a request is logged. Add a name to
`logged_query_parameters` only for a parameter you know carries no identifying
content.

`[telemetry.otlp]` exports the traces and the metrics to an OpenTelemetry
collector over OTLP/gRPC:

| Key | Default | Meaning |
|---|---|---|
| `endpoint` | none | The collector's OTLP/gRPC endpoint; `https://` speaks TLS, and the value is masked in `config check` |
| `traces` | `true` | Whether the spans are exported |
| `metrics` | `true` | Whether the metrics are pushed |
| `sample_ratio` | `1.0` | The share of the traces that are sampled |
| `tls.trust_roots_file` | unset | A PEM bundle of trust roots beside the platform's |
| `tls.client_identity_file` | unset | The client certificate chain and its key in one PEM, for mutual TLS (or `tls.client_identity` inline) |
| `tls.min_version` | `"1.3"` | The oldest TLS version spoken; `"1.2"` lowers it |

The bridge reads `tls` with the family's outbound TLS reader and dials an
`https://` collector through the platform verifier with those roots beside
it. TLS material beside an endpoint that is not `https://` refuses the start.

### The console

Two more variables set the console for one run, and they win over the file
and over `FERROBRIDGE__TELEMETRY__FORMAT` and `FERROBRIDGE__TELEMETRY__FILTER`:

| Variable | Sets | Values |
|---|---|---|
| `FERROBRIDGE_LOG_FORMAT` | `[telemetry] format` | `auto`, `json` or `pretty`; any other value refuses the start with exit 78 |
| `RUST_LOG` | `[telemetry] filter` | A `tracing` filter directive such as `debug,hyper=warn` |

```sh
FERROBRIDGE_LOG_FORMAT=pretty RUST_LOG=debug ferrobridge serve
```

`auto` writes `pretty` on a terminal and `json` anywhere else, so a container
or a log pipeline gets one JSON object per line with no configuration.
`pretty` writes colour on a terminal unless `NO_COLOR` is set. In `pretty`, a
carriage return or line feed inside a logged value is written as the two
characters `\r` or `\n`, so no value can start a second line; `json` escapes
it inside the string.

Under `pretty`, `ferrobridge serve` prints the family's banner to stdout
before the first log line: the wordmark, the version with the commit, the
build instant and the compiler, the product line, the maintainer and the site,
the pins this build serves, and each lane with the hosts it reaches. A host is the URL's
host and port and never its user information. Under `json` nothing but log
lines reaches stdout.

The first boot events describe the process:

| Event | Fields |
|---|---|
| `console` | `format` (the rendering in effect), `colour`, `filter` |
| `build` | `version`, `commit`, `built_at`, `rustc`, `openehr_crates`, `fhir_types` |
| `lane` | `lane` (`facade`, `operations`, `etl` or `terminology`), `enabled`, `identifiable`, and where set `cdr_host`, `terminology_host`, `cdm_host`, `cdm_schema`, `bridge_schema`, `contexts`, `programs`, `templates` |

`listening` is the last boot event of the lanes, and the
[usage report](usage-report.md)'s one line, `usage report ON` or
`usage report OFF` with the reason, follows it. The `etl` lane reports what
`ferrobridge etl run` would reach; `serve` never runs it.

`GET /health/info` answers the same build facts and pins as JSON, so a scraper
reads what the banner shows:

```json
{
  "product": "FerroBRIDGE",
  "version": "0.0.3",
  "build": {
    "commit": "0123456789abcdef0123456789abcdef01234567",
    "commit_short": "0123456789ab",
    "built_at": "2026-01-01T00:00:00Z",
    "rustc": "rustc 1.99.1 (58cf8f9c3 2026-10-13)"
  },
  "pins": [
    { "name": "FHIRconnect", "version": "v1.0.0" },
    { "name": "FHIR", "version": "R4 (4.0.1)" }
  ]
}
```

The pin list is shortened here; the route lists every pin the banner prints.
`built_at` is the `SOURCE_DATE_EPOCH` instant when the build sets one, and the
commit is `unknown` for a build outside a git checkout.

### `[cdr]`

The openEHR CDR lane. Absent, the bridge holds no CDR client and readiness
reports no CDR indicator. This section reaches identifiable data, and the
server says so once at start-up, naming the section and never a value.

| Key | Default | Secret | Meaning |
|---|---|---|---|
| `endpoint.url` | none, required | | The openEHR REST API root, usually ending in `/v1` |
| `endpoint.timeout` | `"30s"` | | How long one call may take, connection included |
| `endpoint.connect_timeout` | unset | | How long opening the connection may take; unset, only `timeout` bounds it |
| `endpoint.tls` | the platform's roots | | `trust_roots_file`, `client_identity_file` or `client_identity` (secret), and `min_version` (`"1.3"`): the TLS of the CDR's connection, for a CDR behind a private root or one that asks for a client certificate |
| `demographic` | `false` | | Whether the bridge reaches the CDR's ITS-REST 1.1.0 Demographic API, which the release marks `x-status: DEVELOPMENT` |

### `[cdr.endpoint.retry]` and `[terminology.endpoint.retry]`

| Key | Default | Meaning |
|---|---|---|
| `max_attempts` | `3` | How many times a call is sent at most, the first try included; `1` disables retry |
| `initial_backoff` | `"200ms"` | The delay before the second attempt |
| `max_backoff` | `"5s"` | The ceiling every later delay is clamped to |

For the CDR, the bridge calls through the generated ITS-REST client of the
`openehr-its` crate, and the three keys are that client's retry budget. Only a
`GET`, `PUT` or `DELETE` is sent again, after a failure to connect, a
timeout, or a `5xx` other than `501`; a `POST` is sent once. A `401`, a `403`
and a status the operation does not document are never retried.
`[cdr.endpoint] timeout` is the per-request timeout of the client's HTTP engine.

`[cdr] demographic` turns on the client of the ITS-REST 1.1.0 Demographic API,
DEVELOPMENT: the reads, creates, updates and deletes of `PERSON`,
`ORGANISATION` and `ROLE`, and the facade serves every party context's
resource type from it ([Party resources](../integrate/fhir-facade.md#party-resources)).
A party context with the key off is a boot refusal. A Release
1.1.0 `PARTY` may carry `reverse_relationships`, which the development line
removed; the bridge drops the member before it reads the party and counts the
entries it dropped. Each entry restates a relationship the demographic
repository already holds (RM Demographic IM Release 1.1.0 §2.2.1).

### `[cdr.endpoint.credentials]` and `[terminology.endpoint.credentials]`

The bridge authenticates to an upstream in one of two modes: RFC 7617 basic
authentication with `user` and `password`, or a token it obtains itself with
the OAuth 2.0 client credentials grant
([`[cdr.endpoint.credentials.oauth]` and `[terminology.endpoint.credentials.oauth]`](#cdrendpointcredentialsoauth-and-terminologyendpointcredentialsoauth)). It takes no static
bearer token: a `bearer_token` key is an unknown key and refuses the start.
An inline value beside its `_file` sibling is a boot error too.

| Key | Default | Secret | Meaning |
|---|---|---|---|
| `user` | none | | The user-id of RFC 7617 basic authentication, written inline; it is no secret, and it may hold no colon (RFC 7617 §2) |
| `password` | none | yes | The password of RFC 7617 basic authentication; use `password_file` to read it from a file |

The table is the one `ferrohealth-smart`, the FerroHEALTH resource-server
crate, defines for every upstream of every product. A `password` without a
`user` is a boot error, and so is a `user_file` key: the user-id is read
inline only.

The user and the password become the `Authorization` header of every call,
`Basic` over `user:password`.

### `[cdr.endpoint.credentials.oauth]` and `[terminology.endpoint.credentials.oauth]`

The bridge can obtain its token for the CDR, and for the terminology server,
itself with the OAuth 2.0 client credentials grant (RFC 6749 §4.4) and renew
it before it expires. The bridge always reaches an upstream under its own
token for that upstream's audience and never forwards a caller's token,
because an access token names the resource server it is for (RFC 9068 §4).
Setting the section beside `user` is a boot error. The two sections take the
same keys, and each lane keeps its own token.

Use this section when the FHIR facade and the CDR are separate audiences of
one authorization server. Callers then hold tokens for the FHIR audience, and
the bridge obtains and renews its own for the CDR's. Set `audience` or
`resource` to whichever parameter that authorization server reads to select
the CDR. A terminology server that checks tokens, such as one that validates
SMART tokens on writes, takes the same arrangement under
`[terminology.endpoint.credentials.oauth]`.

| Key | Default | Secret | Meaning |
|---|---|---|---|
| `token_endpoint` | none | | The token endpoint URL (RFC 6749 §3.2) |
| `issuer` | none | | The issuer identifier, an `https` URL with no query or fragment (RFC 8414 §2), or plain `http` on a loopback host; the token endpoint is read from its metadata |
| `client_id` | none, required | | The client identifier (RFC 6749 §2.2) |
| `client_auth` | `client_secret_basic` | | `client_secret_basic` sends the id and the secret as HTTP Basic, `client_secret_post` sends them in the request body (RFC 6749 §2.3.1), `private_key_jwt` sends a client assertion signed with `private_key_file` (RFC 7523 §2.2) |
| `client_secret` | none | yes | The client secret, required under `client_secret_basic` and `client_secret_post`; use `client_secret_file` to read it from a file |
| `private_key_file` | none | yes | The PEM file of the private key a `private_key_jwt` assertion is signed with |
| `kid` | none | | The `kid` the assertion names; the RFC 7638 thumbprint of the key when unset |
| `assertion_alg` | by key type | | The algorithm the assertion is signed with, such as `ES384` |
| `scope` | none | | The `scope` parameter, space-delimited scope tokens (RFC 6749 §3.3); empty sends none |
| `audience` | none | | An `audience` parameter, for an authorization server that reads one; no specification defines it for this grant |
| `resource` | none | | The `resource` parameter, an absolute URI with no fragment (RFC 8707 §2) |
| `refresh_margin` | `"30s"` | | How long before the token expires a new one is fetched |
| `timeout` | `"10s"` | | How long one call to the authorization server may take |
| `tls.trust_roots_file`, `tls.client_identity_file`, `tls.min_version` | none, none, `"1.3"` | | The TLS to the authorization server: trust roots beside the platform's, the client certificate chain and key in one PEM for mutual TLS (or `tls.client_identity` inline), and the oldest TLS version spoken |

Set exactly one of `token_endpoint` and `issuer`; both, or neither, is a boot
error. With an `issuer`, the bridge reads the RFC 8414 metadata at
`/.well-known/oauth-authorization-server` inserted before the issuer's path,
and on a `404` the OpenID Connect discovery document at the issuer followed
by `/.well-known/openid-configuration`. The document's `issuer` must equal
the configured value exactly, or no token is requested (RFC 8414 §3.3).

The bridge fetches the token on the first call to that upstream, not at boot,
and every call to it shares it. It fetches a new one `refresh_margin` before `expires_in`
runs out; the margin is capped at half the token's lifetime, so a short-lived
token is still reused. A token without `expires_in` is reused until the
upstream refuses it. Calls that find no usable token at the same time share
one request to the token endpoint. When the upstream answers `401`, the bridge
drops the token, fetches a new one and sends the call once more, a `POST`
included, since a `401` means the upstream applied nothing (RFC 9110
§15.5.2). A second `401` reaches the caller as any refused credential does.

A token request that fails is an error naming the token endpoint's status
and, when the body carries one, the OAuth `error` and `error_description`
(RFC 6749 §5.2), for example `the token endpoint answered 401 Unauthorized:
invalid_client (unknown client)`. A failed fetch is not cached: the next call
asks again. The `cdr` or `terminology` readiness indicator reports down with
that reason while no token can be obtained. The bridge sends bearer tokens only; a token
response with another `token_type` is refused. Point `token_endpoint` and
`issuer` at `https` URLs outside a network you control, because the client
secret travels in the token request.

```toml
[cdr.endpoint.credentials.oauth]
issuer = "https://auth.example.org/realms/clinic"
client_id = "ferrobridge"
client_secret_file = "/run/secrets/cdr_client_secret"
scope = "openehr"
```

As environment variables, the same section reads
`FERROBRIDGE__CDR__ENDPOINT__CREDENTIALS__OAUTH__ISSUER`,
`FERROBRIDGE__CDR__ENDPOINT__CREDENTIALS__OAUTH__CLIENT_ID` and
`FERROBRIDGE__CDR__ENDPOINT__CREDENTIALS__OAUTH__CLIENT_SECRET_FILE`. The terminology
section reads the same names under `FERROBRIDGE__TERMINOLOGY__ENDPOINT__CREDENTIALS__OAUTH__`.

### `[cdr.endpoint.credentials.onward]`

With this table, each FHIR facade request reaches the CDR under the person
who made it. The bridge exchanges the caller's access token for a token of
the CDR's issuer that names the same person, and sends that token on every
CDR call of the request. The bridge authenticates the exchange with its own
client of `[cdr.endpoint.credentials.oauth]`, which the table requires and which must ask
for the CDR alone, by `resource` (RFC 8707 §2) or by `scope` (such as
`api://<cdr>/.default` under Entra ID), because a machine's request reaches the
CDR under that token; one naming neither refuses the start. Only the CDR
lane takes the table: an `onward` table under `[terminology.endpoint.credentials]` or
another lane refuses the start, and so does one on a server whose `[auth]` is
off, because no caller then holds a token to exchange.

The issuer's `profile` picks the grant:

- `rfc9068` (and `keycloak`): OAuth 2.0 Token Exchange (RFC 8693 §2.1). The
  caller's token is the `subject_token`, an access token, and the CDR is named
  by `resource`, or by `audience` under `keycloak`. Keycloak requires the
  bridge's client in the `aud` of the caller's token, so the client scope of
  the facade's clients carries an audience mapper that puts it there.
- `entra`: the JWT bearer grant (RFC 7523 §2.1) with the Microsoft identity
  platform's on-behalf-of parameters. The caller's token, whose `aud` is the
  bridge's application id, is the `assertion`, `requested_token_use` is
  `on_behalf_of`, and `scope` names the CDR, such as `api://<cdr>/.default`.
  Register the bridge and the CDR to issue v2.0 tokens.

| Key | Default | Meaning |
|---|---|---|
| `issuer` | none, required | The issuer of the onward tokens; its metadata names the token endpoint unless `token_endpoint` is set |
| `kind` | none, required | `persons` for a same-organisation person, `federation` for a professional of another party, `patients` for a patient |
| `profile` | `rfc9068` | The preset the issuer's profile starts from: `rfc9068`, `entra`, `keycloak` and the others `[[auth.issuer]]` takes |
| `token_endpoint` | none | The token endpoint itself |
| `client_id` | the `oauth` table's | The bridge's client id at this issuer |
| `audiences` | none, required | The audiences an onward token may name; a token naming any other is refused |
| `indicators` | none | The `resource` or `audience` values the exchange names the CDR by |
| `scope` | none | The `scope` the request names; the on-behalf-of grant requires it |
| `correlation_claim` | none | The claim a chained `federation` route carries from the caller's token to the onward one |
| `keys` | none | The profile keys over the preset; `max_onward_lifetime`, a duration such as `"1h"`, is required |
| `cache` | `true` | `false` holds no onward token, so every CDR call exchanges one at the issuer, with a warning logged at start; a setting for a performance measurement, never for production |
| `actor_assertion` | `false` | `true` names the bridge's client as the actor of every exchange it sends without an actor token, with an assertion signed by the `oauth` table's key (RFC 8693 §2.1); a lane with no assertion key, or under the JWT bearer grant, is refused at start |

Each issued token is decoded and refused unless its issuer is the one asked,
its `aud` lies inside `audiences`, it lives no longer than
`max_onward_lifetime`, it names the bridge's client as its actor (`act`, or
the profile's client claim such as `azp`), and it names the caller's person
by the profile's `person_equality` claims (`iss` and `sub` by default, `oid`
and `tid` under `entra`, so a different pairwise `sub` is accepted). A
`patients` issuer's token must also read as a patient token, a `patient/`
grant with no `user/` or `system/` one, and carry the caller's `patient`
launch context unchanged. Unless
`cache = false`, the bridge caches the token under the caller's token and the
target until
`refresh_margin` before the earlier of its own and the caller's token's
expiry, and checks it again on every use. The caller's token appears in no
log, span or metric.

Who reaches the CDR how:

| Caller | Reaches the CDR |
|---|---|
| A machine (a client credentials token) | Under the bridge's own token of `[cdr.endpoint.credentials.oauth]`, never exchanged; its composition names the machine's client in `originating_system_audit.system_id` |
| A person, a delegated or representing person, under `kind = "persons"` | Under the exchanged token |
| A professional of another party, under `kind = "federation"` | Under the exchanged token |
| A patient, under `kind = "patients"` | Under the exchanged token |
| A caller no issuer's kind carries, another party's organisation, a guest, a Basic user | Refused with `403` |

A failed exchange fails the request, and the bridge's own token is never sent
in its place. The token endpoint's `400` or `403` (RFC 8693 §2.2.2,
`invalid_request` or `invalid_target`) is the facade's answer too, with the
OAuth error in the `OperationOutcome`. An on-behalf-of `interaction_required`
answer is a `401` whose `WWW-Authenticate` carries the error and the claims
challenge, so the client signs in again. A token endpoint that fails, refuses
the bridge's own client (`401`), or issues a token the checks above refuse,
is a `502`.

```toml
[cdr.endpoint.credentials.oauth]
token_endpoint = "https://login.microsoftonline.com/<tenant>/oauth2/v2.0/token"
client_id = "<bridge application id>"
client_auth = "private_key_jwt"
private_key_file = "/run/secrets/bridge_key.pem"

[cdr.endpoint.credentials.onward]
issuer = "https://login.microsoftonline.com/<tenant>/v2.0"
kind = "persons"
profile = "entra"
token_endpoint = "https://login.microsoftonline.com/<tenant>/oauth2/v2.0/token"
audiences = ["<cdr application id>"]
scope = "api://<cdr application id>/.default"

[cdr.endpoint.credentials.onward.keys]
max_onward_lifetime = 3600
```

As environment variables, the table reads
`FERROBRIDGE__CDR__ENDPOINT__CREDENTIALS__ONWARD__ISSUER`,
`FERROBRIDGE__CDR__ENDPOINT__CREDENTIALS__ONWARD__KIND`,
`FERROBRIDGE__CDR__ENDPOINT__CREDENTIALS__ONWARD__PROFILE` and the other keys the same
way.

### `[[cdr.onward]]`

Each `[[cdr.onward]]` table adds one more onward issuer beside
`[cdr.endpoint.credentials.onward]`, with the same keys. The bridge then carries
each caller through the issuer whose `kind` carries it: a person to the
`persons` issuer, a professional of another party to the `federation`
issuer, and a patient to the `patients` issuer. Every table authenticates
with the client of `[cdr.endpoint.credentials.oauth]`, which it requires. Two issuers
of one kind refuse the start, across `[cdr.endpoint.credentials.onward]` and the
array, and so does the array on a server whose `[auth]` is off. Either form
may stand alone: a deployment that carries only patients declares one
`[[cdr.onward]]` table and no `[cdr.endpoint.credentials.onward]`.

Patients usually sign in at an issuer of their own, a separate Keycloak
realm or Entra ID tenant, whose tokens the facade admits through an
`[[auth.issuer]]` of `kind = "patients"`. The onward table then names that
realm or tenant. Under the default `rfc9068` profile:

```toml
[cdr.endpoint.credentials.onward]
issuer = "https://id.example.org/realms/staff"
kind = "persons"
audiences = ["cdr"]
indicators = ["cdr"]

[cdr.endpoint.credentials.onward.keys]
max_onward_lifetime = 300

[[cdr.onward]]
issuer = "https://id.example.org/realms/patients"
kind = "patients"
audiences = ["cdr"]
indicators = ["cdr"]

[cdr.onward.keys]
max_onward_lifetime = 300
```

### `[terminology]`

The FHIR terminology lane. The facade, the `$tofhir` and `$toopenehr`
operations and the HL7 v2 face translate every `conceptmap` code through it.
Going into openEHR, they also resolve the display of a coding that carries
none through `CodeSystem/$lookup`, because a `DV_CODED_TEXT` needs a value.
Absent, the bridge holds no terminology client, and the engines fail closed on
anything that would need one: a `conceptmap` mapping refuses, and a coding
with no display keeps its code as the value.

| Key | Default | Meaning |
|---|---|---|
| `endpoint.url` | none, required | The FHIR service base URL |
| `wire_version` | `r4` | `stu3`, `r4` or `r4b`, the release the server answers in |
| `endpoint.timeout` | `"30s"` | How long one call may take, connection included |
| `endpoint.connect_timeout` | unset | How long opening the connection may take |
| `endpoint.tls` | the platform's roots | `trust_roots_file`, `client_identity_file` or `client_identity`, and `min_version` (`"1.3"`), for a server behind a private root or one that asks for a client certificate |
| `display_fallback` | `false` | For a code the server does not know: `true` writes the code as the value of the `DV_CODED_TEXT` or `DV_TEXT`, `false` refuses the resource. A failed call refuses either way |

As an environment variable, `display_fallback` reads
`FERROBRIDGE__TERMINOLOGY__DISPLAY_FALLBACK`.

### `[cdm]`

The OMOP CDM database lane. This section reaches identifiable data, and the
server says so once at start-up. Under `serve`, readiness carries a `cdm`
indicator that reports whether the database answers.

| Key | Default | Secret | Meaning |
|---|---|---|---|
| `url` | none, required when the section is present | yes | The PostgreSQL connection URL; it must name its `sslmode` |
| `tls_ca` | none | no | The PEM CA the database's certificate is checked against |
| `tls_ca_file` | none | no | A file holding that CA, read at boot |
| `schema` | `cdm` | no | The schema the CDM tables live in, an unquoted lower-case identifier |
| `bridge_schema` | `ferrobridge` | no | The schema the bridge keeps its natural-key side table, id sequences and watermarks in |
| `person_policy` | `create_on_first_sight` | no | For a person no mapping writes a `PERSON` row for: `create_on_first_sight` gives the EHR a `person_id` and a placeholder `PERSON` row (concepts 0, `year_of_birth` 0) the first time a row refers to it; `existing` refuses a row whose EHR has no `PERSON` row ([The PERSON row](../integrate/omop-etl.md#the-person-row)) |

Two clients open the database: the concept resolver's pool and the CDM
writer's own connection. Both read the `sslmode` of `url` and the CA from
`tls_ca` or `tls_ca_file`, and both handshake with one TLS configuration:
the writer directly, and the pool through a relay on a private Unix socket
that the bridge starts beside it. With a CA set, both trust that CA and
nothing else; without one, `verify-ca` and `verify-full` trust the webpki
root set. A URL that asks for TLS names one host and one port, the server the
relay reaches; a list of hosts is refused. The modes are the libpq ones that never fall
back to plaintext
(<https://www.postgresql.org/docs/current/libpq-ssl.html>):

| `sslmode` | What the clients do |
|---|---|
| `disable` | Connect without TLS. A CA with `disable` is refused |
| `require` | Connect over TLS. With a CA, the certificate must chain to it, as libpq checks when a root certificate is present; without one, it is not checked |
| `verify-ca` | Connect over TLS; the certificate must chain to the CA, or to the webpki root set when no CA is set |
| `verify-full` | As `verify-ca`, and the certificate must name the host in `url` |

`prefer` and `allow` are refused when the configuration is read, naming the
mode: a client that honours them connects in plaintext when the server offers
no TLS, and that fallback is silent. A URL that names no `sslmode` is refused
the same way, because libpq's default is `prefer`: write `sslmode=disable`
for a database on a trusted local network or in a test container, and one of
the three TLS modes everywhere else. Any other `ssl` parameter in the URL
(`sslrootcert`, `sslcert`, `sslkey` and the rest) is refused too: the CA
comes from `tls_ca_file`, and the bridge sends no client certificate. For the
same reason the bridge refuses to start when `PGSSLROOTCERT`, `PGSSLCERT` or
`PGSSLKEY` is set in its environment: the pool's PostgreSQL client reads
those variables whatever the configuration says, so the TLS settings stay in
one place. The CA is public material, so it is
not a secret, and `FERROBRIDGE__CDM__TLS_CA` and
`FERROBRIDGE__CDM__TLS_CA_FILE` set the two keys from the environment; setting
both is refused. A typical production URL, with the CA mounted beside it:

```toml
[cdm]
url_file = "/run/secrets/cdm_url"   # postgres://bridge:...@cdm.internal:5432/omop?sslmode=verify-full
tls_ca_file = "/run/secrets/cdm_ca.pem"
```

### What `cdm init` does

`ferrobridge cdm init` reads which CDM tables `[cdm] schema` already holds
before it applies anything:

| The schema holds | `cdm init` |
|---|---|
| none of the 39 CDM v5.4 tables, or does not exist | Creates the schema when absent, then applies OHDSI's tables, primary keys and indices in one transaction, and exits 0 |
| every CDM v5.4 table | Applies nothing, prints that the schema is already initialised with the `cdm_version` its `CDM_SOURCE` rows record, and exits 0 |
| some of the tables | Applies nothing and exits 1, naming every missing table |

It then creates the bridge schema, which is safe to repeat, and brings a
bridge schema an earlier release created to the current shape, so run it once
after an upgrade. Tables outside the
CDM's 39 are left alone and do not count. The check reads table names only,
so a schema whose tables exist without their keys or indices counts as
initialised.

OHDSI's fourth file, `OMOPCDM_postgresql_5.4_constraints.sql`, carries the
foreign keys and is left out by default. `cdm init --with-constraints`
applies it after the tables, statement by statement in one transaction of its
own. At the pinned OHDSI tag `v5.5.0` PostgreSQL refuses line 157, a foreign
key onto `vocabulary (vocabulary_id)` that the primary keys file gives no key
(SQLSTATE `42830`), so the flag exits 1, prints that statement, and leaves the
tables in place and no foreign key behind.

### `[cdm.pseudonymisation]`

With this section present, the CDM writer pseudonymises every row before it
reaches the CDM: each column the CDM field definitions describe as a link
back to the source carries a keyed pseudonym, the provider's name, `npi` and
`dea` and a location's address are written as NULL, and every date of a
person moves by that person's shift. The result is pseudonymised data, still
personal data: free text, the code `*_source_value` columns and the concepts
of rare conditions are written as mapped, and the bridge schema keeps the
source identifiers it needs to replace a composition's rows, so never hand it
out with the CDM ([Pseudonymisation](../integrate/omop-etl.md#pseudonymisation)).

| Key | Default | Secret | Meaning |
|---|---|---|---|
| `key` | none, required when the section is present | yes | The HMAC-SHA-256 key, at least 32 bytes. One source value under one key gives one pseudonym on every run, so an incremental load joins; a new key renames every person |
| `key_file` | none | yes | A file holding the key, read at boot |
| `date_shift_days` | `0` | no | The largest shift of a person's dates either way, in whole days, at most 3650; `0` leaves dates as they are |

### `[etl]`

The OMOP ETL jobs `ferrobridge etl run` and `ferrobridge etl fhir` run. Both queries are parsed and
checked when the configuration loads, so a query the runner cannot read
refuses every job.

| Key | Default | Secret | Meaning |
|---|---|---|---|
| `aql` | none | no | The composition query, which `etl run` needs and `etl fhir` does not read; `etl run` refuses to start without it. It aliases `ehr_id`, `version_uid` and the whole `composition`, carries an `ORDER BY` that ends in `v/uid/value` and no `LIMIT`. Every `COMPOSITION` it reads sits below `VERSION v[LATEST_VERSION]` ([The composition query](../integrate/omop-etl.md#the-composition-query)). It may name `$since`, which `--since` binds. It may also alias `versioned_object_uid`, which the runner then checks against the version; when it does not, the runner reads it from the `version_uid`, whose `object_id` part names the versioned object (openEHR RM Common 1.1.0, `OBJECT_VERSION_ID`) |
| `page_size` | `100` | no | The `fetch` of each page of either query, and the number of watermarks each batch of the deletion check reads |
| `deletions` | `remove` | no | What a run does with a composition the CDR reports deleted: `remove` deletes its rows, links and watermark, `report` lists it in the run report and keeps them. Any other value refuses the start ([Deleted compositions](../integrate/omop-etl.md#deleted-compositions)) |
| `quality_checks` | `false` | no | With `true`, `etl run` runs the ported Data Quality Dashboard checks over the tables it wrote, `PERSON` and the derived tables once the load is done, and the run report carries their verdicts and every failing check id. A failed check never rolls the committed load back. `ferrobridge dq check` runs the same checks over the whole CDM ([Checking a populated database](../integrate/omop-etl.md#checking-a-populated-database-with-the-data-quality-dashboard)) |
| `derived_tables` | `owned_persons` | no | Which rows of `OBSERVATION_PERIOD`, `CONDITION_ERA` and `DRUG_ERA` a run rebuilds: `owned_persons`, the rows of the persons the bridge writes, keeping every other person's rows, or `whole_table`, every row from every row in the schema. Any other value refuses the start ([One writer per CDM table](deployment-shape.md#one-writer-per-cdm-table)) |
| `type_concept_id` | none, required | no | The `*_type_concept_id` the mappings write, the provenance of the records |
| `observation_period_type_concept_id` | none, required | no | The `period_type_concept_id` of every observation period |
| `vocabulary_aliases` | `LOINC` to `LOINC`, `SNOMED-CT` to `SNOMED` | no | A table from a terminology id, such as a URI or an OID, to the OHDSI `vocabulary_id` a coded value under it is looked up in, added over the defaults. A terminology id it does not list is looked up as its own `vocabulary_id`, and one longer than the 20 characters of `VOCABULARY.vocabulary_id` resolves to concept 0. The concept is still found in the loaded vocabulary. An empty id or vocabulary, a vocabulary longer than 20 characters, or a value that is not a string refuses the start. Written as `[etl.vocabulary_aliases]` with quoted keys, `"http://terminology.hl7.org/CodeSystem/v2-0001" = "Gender"` |

### `[etl.visits]`

The visit derivation, off until the section is present. The rows are grouped
by EHR and source, each visit spanning the earliest start to the latest end.

| Key | Default | Secret | Meaning |
|---|---|---|---|
| `aql` | none, required | no | The visit query, aliasing `ehr_id`, `visit_source`, `visit_start` and `visit_end`, with an `ORDER BY` that ends in `v/uid/value`, every `COMPOSITION` it reads below `VERSION v[LATEST_VERSION]` ([The composition query](../integrate/omop-etl.md#the-composition-query)), and no `$since` ([The visits the query no longer answers](../integrate/omop-etl.md#the-visits-the-query-no-longer-answers)) |
| `visit_concept_id` | none, required | no | The `visit_concept_id` of every visit |
| `visit_type_concept_id` | none, required | no | The `visit_type_concept_id` of every visit |

### `[mappings]`

The mapping sets this deployment runs. The facade, the two FHIRconnect
operations and the mapping subcommands read the FHIRconnect set in
`directory`; `etl run` reads the OMOCL set in `omocl`. The FHIRconnect set
and its templates are read once at boot, and a mapping that does not compile
refuses the start rather than the first request that touches it. The OMOCL
set is read once when `etl run` starts and compiled against each template
the first time a composition of it arrives, because an OMOCL file names an
archetype and no template.

| Key | Default | Meaning |
|---|---|---|
| `directory` | none | The directory the mapping files are read from, recursively (`.yml`, `.yaml`) |
| `templates` | none | The directory holding the operational templates the two operations compile against, as OPT 1.4 XML (`.opt`) |
| `omocl` | none | The directory the OMOCL files `ferrobridge etl run` maps with are read from, recursively (`.yml`, `.yaml`); read and validated once at start, and every file an `Include` names must be in it |

The three directories are walked the same way. Every real subdirectory is
read, a symlink to a file is followed, and a symlink to a directory is not.
An entry whose name starts with `..` is skipped, so a Kubernetes ConfigMap
mounted as a whole directory is read once: its files sit in a hidden
`..<timestamp>` directory behind a `..data` symlink, and each key appears at
the top level as a file symlink.

The facade always takes its templates from the CDR and needs `directory`
alone. The two FHIRconnect operations take theirs from one of two sources:

- With `templates` set, every `.opt` file in that directory is read. This
  source wins when `[cdr]` is configured too, because the directory is the
  explicit choice, and the operations then run with no CDR at all.
- With `templates` unset and `[cdr]` configured, the server reads the context
  files first and fetches each template they name from the CDR
  (`GET /definition/template/adl1.4/{template_id}`, openEHR ITS-REST 1.1.0
  §Definition). A template the CDR does not hold, or any other refused fetch,
  stops the start with an error naming the template and the status the CDR
  answered, so the lane never comes up with part of its mappings. A CDR that
  does not answer yet is waited for, with readiness at `503` meanwhile
  ([the health probe](container.md#the-health-probe)).

A `directory` with neither `templates` nor `[cdr]` is refused while the
operations are enabled. The server checks this when it reads the
configuration, before it calls any upstream, and the refusal names both
sources. With `[operations] enabled = false`, `directory` alone
is enough for the facade.

```toml
# The operations compile against local templates and reach no CDR.
[mappings]
directory = "/etc/ferrobridge/mappings"
templates = "/etc/ferrobridge/templates"
```

```toml
# The operations compile against the templates the CDR serves.
[cdr]
endpoint.url = "https://cdr.example.org/openehr/v1"

[mappings]
directory = "/etc/ferrobridge/mappings"
```

### `[facade]`

The FHIR R4 facade. It is off until `enabled` is true, and a disabled facade
mounts no route, so a request to `/fhir` answers `404` rather than `403`. An
enabled facade needs `[cdr]` and `[mappings] directory`: it compiles the
mapping set at boot against the templates the CDR holds, so a mapping that does
not compile refuses the start rather than the first request that touches it.
This lane reaches identifiable data, and the server says so once at start-up.

| Key | Default | Meaning |
|---|---|---|
| `enabled` | `false` | Whether the facade routes are mounted |
| `base_url` | `http://127.0.0.1:8080/fhir` | The absolute FHIR service base a client reaches, which `Location` is written under |
| `ehr_policy` | `existing` | `existing` writes only into an EHR the CDR already holds; `create_on_first_write` creates one for an unknown subject |
| `identity_store` | `identity.redb` | The file the identity map is kept in, opened at boot and refused when it cannot be written |
| `subject_namespace` | `ferrobridge` | The namespace a literal subject reference is looked up in on the CDR, the `PARTY_REF.namespace` of `EHR_STATUS.subject` |
| `subject_id_form` | `reference` | How the id of a literal subject reference is written: `reference` keeps `Patient/<id>`, `logical_id` keeps the bare `<id>`, and `uuid` keeps the bare `<id>` and refuses one that is no UUID |
| `subject_type` | `PERSON` | The `PARTY_REF.type` of the subject of an EHR the facade creates, one of the BASE `Type_validity` values (`PERSON`, `ORGANISATION`, `GROUP`, `AGENT`, `ROLE`, `PARTY`, `ACTOR`) or `ANY` |
| `ehr_status_identifier_system` | none | The `Identifier.system` whose identifier a mapping that offers the subject of the EHR (`ehrStatusAddExternalREF`) writes as `EHR_STATUS.subject.external_ref` |
| `system_id` | `FerroBRIDGE` | The `AUDIT_DETAILS.system_id` every commit records |
| `composition_language` | none, required | The `COMPOSITION.language` every commit carries, an ISO 639-1 code |
| `composition_territory` | none, required | The `COMPOSITION.territory` every commit carries, an ISO 3166-1 code |
| `search_count` | `20` | The page size of a search that names no `_count` |
| `search_count_max` | `100` | The largest page one search answers; a larger `_count` is answered with this many |
| `binding_identifier_issuers` | none | The `DV_IDENTIFIER.issuer` of each binding key the CDR binds a login to (the `identifier_issuer` of its `[[auth.party]]` tables); the facade refuses with `403` to create, update or delete a party resource whose party carries an identifier of one of them |

The three `subject_` keys make the facade's link between a Patient and its
EHR match the one the CDR's owner writes, because the facade finds an EHR
with `GET /ehr?subject_id=&subject_namespace=`. A CDR whose owner links each
EHR to a demographic party under its own namespace takes that namespace and
`logical_id`, so a subject `Patient/<party uid>` is looked up as the party
uid. A CDR that accepts only UUID pseudonyms as subject ids takes the
namespace it lists and `uuid`, so a subject whose id is no UUID is refused
with a diagnostic naming `facade.subject_id_form` before the CDR is called.
Under `logical_id` and `uuid` a `Patient` keys its own EHR the same way, by
its logical id in `subject_namespace`, so creating or updating a Patient finds
the EHR its resources are written into; a Patient with no `id`, or under
`uuid` one whose `id` is no UUID, is refused naming `facade.subject_id_form`.
Under `reference` a Patient keys its EHR by its first `identifier` with a
value, as before.
The facade never derives a pseudonym itself: name a subject by the id the CDR
already holds. A subject named by `identifier` keeps the identifier's
`system` as its namespace and its `value` as its id. A `subject_type` outside
the BASE set, or a `subject_namespace` that breaks the `OBJECT_REF`
namespace rule, is a boot error naming the key.

A mapping can offer the resource's identifiers as the subject of the EHR,
as the published `KDS_admin_entry_person.yml` does with
`ehrStatusAddExternalREF`. The openEHR RM leaves which identifier
`EHR_STATUS.subject.external_ref` holds to the deployment, so
`ehr_status_identifier_system` names its `system`. A CDR that takes subject
pseudonyms only (FerroEHR with `[privacy] subject_namespaces`) takes the
system its pseudonyms are issued in. The reference is written the way the
facade writes the subject of an EHR it creates: namespace and scheme from the
`system`, id from the `value`, type from `subject_type`. Before the
composition is committed the facade reads the `EHR_STATUS`: one that already
holds the reference is left as it is, one with no subject reference takes it
with `If-Match` on the version read, and one that names another subject
answers `409`. A CDR refusal of the write answers through the same status
table as a composition write, a `412` with the latest version as its `ETag`,
and nothing is committed after it. With the key unset, a write whose mapping
offers identifiers answers `422` naming the key; identifiers of another
system are left out of the subject reference, and two distinct identifiers of
the configured system answer `422`.

The two composition fields have no default. FHIRconnect puts them on the
project performing the mapping, and there is no correct language for a clinical
record you did not write, so an enabled facade with either key unset is a boot
error naming the key.

The identity store holds identifiers: a subject to its `ehr_id`, a resource id
to the composition it came from, and the source identity of every resource the
facade consumed. No clinical content is written into it. Back it up with the
CDR, because a lost map means the next create writes a second composition for a
resource the CDR already holds. In a container with a read-only root
filesystem, put the file on a persistent volume and run one process per file
([the identity store](container.md#the-identity-store)).

### `[facade.references]`

How the facade resolves a reference a request names and does not carry, for a
`reference` mapping that reads the referenced resource. A relative reference
(`Observation/abc`), or an absolute one under `[facade] base_url`, names a
resource this server committed, and the facade reads it through the identity
map and the CDR as a read answers it. An absolute reference to another server
is fetched only from a base listed in `fetch_bases`, with redirects refused,
so by default nothing is fetched. A reference whose resource is absent,
because its base is not listed, the server answered `404` or `410`, or this
server holds no such resource, is a declared skip: the mapping proceeds
without it and the log line names the reference, the base and the upstream
status. A fetch that failed, because the server answered another status, the
fetch timed out or the body is not the referenced resource, refuses a request
whose mapping reads that reference with `502 Bad Gateway`, and the
`OperationOutcome` names the reference and the upstream status. FHIRconnect
says only that the engine "should attempt to GET it from the sending site. If
not possible, the engine proceeds with the mapping"; its strictness chapter
asks that "either all data should be mapped, or the mapping should fail", so
an upstream failure refuses. The allow-list, the depth and the timeout are
FerroBRIDGE's own design.

| Key | Default | Meaning |
|---|---|---|
| `fetch_bases` | `[]` | The FHIR service bases an absolute reference may be fetched from, each an `http` or `https` URL with no query or fragment |
| `max_depth` | `2` | How many levels of references are followed: the request's own, then those of each resolved resource; `0` resolves none |
| `timeout` | `"10s"` | How long one fetch from a listed base may take, connection included |

Each key is settable as `FERROBRIDGE__FACADE__REFERENCES__<KEY>`, and
`fetch_bases` takes a TOML array:
`FERROBRIDGE__FACADE__REFERENCES__FETCH_BASES='["https://sender.example/fhir"]'`.
A base that is no `http` or `https` URL, or that carries a query or a
fragment, is a boot error naming `facade.references.fetch_bases`.

### `[facade.identifier_search]`

The chained search through a patient's identifier:
`Condition?patient.identifier=MRN-1` and `subject.identifier`. A client
usually holds a patient number, not an EHR id. The facade looks the
identifier up in the CDR's demographic data with an AQL query rooted at a
demographic class, the way Cadasto's experimental AQL demographic extension
documents it. AQL 1.1.0 defines no demographic semantics, so both queries
are configuration, and the CDR must serve the extension. The reference CDR,
FerroEHR, serves the `PERSON`, `ORGANISATION` and `ROLE` roots from version
5.1.0.

A query's first column is the party's `uid/value`, and a versioned uid
counts as its party. The facade treats each party's uid as a patient's
logical id and finds its EHR the way every subject is found: through the
identity map, then `GET /ehr?subject_id=&subject_namespace=` under the
`[facade] subject_` keys. So the CDR's owner links each EHR to its party's
uid. An identifier that several parties carry matches the resources of each
of them, because R4 matches a bare value "irrespective of the value of the
system property" (FHIR R4 search §3.1.1.4.10); narrow it with
`[system]|[value]`. An identifier no party carries, or a party with no EHR,
adds nothing. Put no `LIMIT` in the query, because every row counts.

| Key | Default | Meaning |
|---|---|---|
| `enabled` | `false` | Whether `patient.identifier` and `subject.identifier` are supported; off, both are unknown parameters |
| `query` | the resolve observed on Cadasto's platform: `SELECT p/uid/value FROM PERSON p WHERE p/details[openEHR-DEMOGRAPHIC-ITEM_TREE.person_details.v2]/items[openEHR-DEMOGRAPHIC-CLUSTER.person_identifier.v2]/items[at0001]/value/id = $identifier ORDER BY p/uid/value ASC` | The AQL a value with no system runs, binding the value as `$identifier` |
| `system_query` | empty | The AQL a `[system]\|[value]` value runs, binding `$identifier` and `$system`. Empty refuses a value that names a system with `400 not-supported`, because FHIRconnect maps no FHIR identifier system to a `DV_IDENTIFIER` attribute |

A query that does not bind the parameters it is handed is a boot error naming
the key. A CDR that refuses the query does not serve the extension, or not
this query of it: the search answers `501` with `not-supported`, naming the
extension and carrying the CDR's words, as the party search does.

### `[facade.party_search]`

The search on the types a party context serves: `Patient`, `Practitioner`,
`PractitionerRole` and `Organization`, by `identifier`, `name` or `_id`
(`Patient?identifier=MRN-1`, `Patient?name=jans`). The ITS-REST Demographic
API has no search, so for `identifier` and `name` the facade runs an AQL
query rooted at a demographic class, the Cadasto experimental AQL
demographic extension, and reads each party it names over the Demographic
API. `_id` needs no query: the facade reads the party the id names. The
reference CDR, FerroEHR, serves the `PERSON`, `ORGANISATION` and `ROLE`
roots from version 5.1.0 and answers `400` to a query rooted at `GROUP`,
`AGENT` or `PARTY_RELATIONSHIP`, which the search answers as `501`.

Each type has its own queries and none has a default, because the archetypes
a tenant stores and the dialect both move. A query's first column is the
party's `uid/value`, and a versioned uid counts as its party. Leave `LIMIT`
and `OFFSET` out: the facade pages the query with the query API's `offset`
and `fetch`. Keep to the forms the extension documents or Cadasto observed as
stable: a demographic root, `WHERE` and `LIKE` over the parameters,
`ORDER BY` and the `person_identifier` `CLUSTER`.

- **`identifier`** matches a bare value whatever its system, so two parties
  that carry it are two entries, and `[system]|[value]` runs `system_query`.
- **`name`** matches a name that starts with the value, ignoring case, as R4
  string search asks. The extension's `LIKE` is case-sensitive, so the value
  is bound three times as a prefix pattern (`jans*`): `$name` as typed,
  `$name_lower` in lower case and `$name_capitalised` with its first letter
  in upper case. Write the query to compare the name with each. A name in
  another case (`JANSEN` for `jans`) and an accent difference are not
  matched until the dialect can fold them. A value carrying `*` or `?` is a
  `400`.
- **`_id`** matches the one party with that id, and a party of another
  template matches nothing.
- **`_count`** is capped at `search_count_max`, and `_count=0` answers the
  number of matches in `total` with no entries.

A search takes one of these parameters, plus `_count` and the paging cursor.
A second one, or any other parameter, is a `400`, unless the client sends
`Prefer: handling=lenient`, which ignores it and reports it. A CDR that
answers `400` to a query does not serve the extension, or not this query of
it: the search answers `501` with `not-supported`, naming the extension and
carrying the CDR's words.

| Key | Default | Meaning |
|---|---|---|
| `enabled` | `false` | Whether the types under `types` are searched; off, no party type offers search |
| `types.<Type>.query` | empty | The AQL an `identifier` value with no system runs for `<Type>`, binding the value as `$identifier`; empty, the type takes no `identifier` |
| `types.<Type>.system_query` | empty | The AQL a `[system]\|[value]` value runs, binding `$identifier` and `$system`; empty refuses a value naming a system with `400 not-supported` |
| `types.<Type>.name_query` | empty | The AQL a `name` value runs, binding `$name` and, to compare them, `$name_lower` and `$name_capitalised`; empty, the type takes no `name` |

```toml
[facade.party_search]
enabled = true

[facade.party_search.types.Patient]
query = "SELECT p/uid/value FROM PERSON p WHERE p/details[openEHR-DEMOGRAPHIC-ITEM_TREE.person_details.v2]/items[openEHR-DEMOGRAPHIC-CLUSTER.person_identifier.v2]/items[at0001]/value/id = $identifier ORDER BY p/uid/value ASC"
name_query = "SELECT p/uid/value FROM PERSON p WHERE p/identities/details/items[at0010]/value/value LIKE $name OR p/identities/details/items[at0010]/value/value LIKE $name_lower OR p/identities/details/items[at0010]/value/value LIKE $name_capitalised ORDER BY p/uid/value ASC"
```

A type with only `_id` needs no query: write its table empty. On with no
type, a `system_query` with no `query`, a query that does not bind its
parameters, and a type no party context serves are boot errors naming the
key. `enabled` is settable as `FERROBRIDGE__FACADE__PARTY_SEARCH__ENABLED`;
write the `types` tables in the file, because an environment key is read in
lower case and a resource type name does not survive it.

### `[auth]`

Inbound authentication: the FHIR routes and the operator API as a SMART App
Launch 2.x resource server. The section is the one `ferrohealth-smart`, the
FerroHEALTH resource-server crate, defines for every product, and it is **on
by default**: `serve` refuses to start a facade, an operator API or a
FHIRconnect operations lane until `[auth]` names an issuer in an
`[[auth.issuer]]` table, or until `enabled = false` says the bridge runs on a
network only trusted callers reach. A server that serves only `/` and the
health routes, and the jobs (`cdm init`, `etl run`, `mapping check`), read no
`[auth]`. **While it is off, the facade authenticates no caller and serves
every request that reaches it**, reads and writes of patient data included,
and the server logs a warning saying so at start-up. The
[FHIR facade](../integrate/fhir-facade.md#security) page says what a caller
sends and what each refusal looks like.

| Key | Default | Meaning |
|---|---|---|
| `enabled` | `true` | Whether every guarded request needs a credential; `false` serves the facade and the operations open |
| `realm` | none | The `realm` every `WWW-Authenticate` challenge names |
| `scopeless_subjects` | `[]` | The machines whose token gets system-wide access when it names no SMART clinical scope, such as a backend service whose authorization server issues no scope, each a table `{ issuer, sub }` naming its `[[auth.issuer]]` table and its `sub`. Only a token the issuer's profile classifies as a machine is admitted this way; any other scopeless token is refused with `403`. An entry naming an issuer the section does not trust refuses the start |
| `capabilities` | `[]` | SMART capabilities the discovery document lists beside the three the facade enforces (`permission-v1`, `permission-patient`, `permission-user`) and the ones the issuer's metadata shows, such as `launch-ehr` or `context-ehr-patient`; each is one SMART App Launch 2.2.0 (Conformance, Capabilities) defines, or a full URI. `sso-openid-connect` also writes the issuer's `issuer`, which the document omits otherwise (Conformance, Metadata) |
| `scopes_supported` | `[]` | The scopes the discovery document lists as `scopes_supported`; list only scopes the bridge honours, because "the server SHALL support all scopes listed here" (SMART App Launch 2.2.0, Conformance, Metadata). Empty omits the member |
| `product_scope` | `__ferrobridge.operator` | The operator scope: it guards the operator API and admits `$validate`, `$tofhir` and `$toopenehr` on the facade, which commit nothing. SMART scopes cover FHIR resources only, so this scope is FerroBRIDGE's own, in a form SMART App Launch 2.2.0 gives an extension scope: a string starting with `__` or a full URI; any other value refuses the start |
| `tls.trust_roots_file` | none | A PEM file of trust roots added to the platform's, for an issuer behind a private certificate authority. The discovery document and the key set of every issuer are read trusting it, and for an issuer with `mode = "introspection"` so are the introspection endpoint and the token endpoint of its credential. It is read once at start; a file that cannot be read, is no PEM, or holds no certificate refuses the start. `tls.client_identity_file` and `tls.min_version` (`"1.3"` unless lowered to `"1.2"`) complete the table |

Each trusted authorization server is one `[[auth.issuer]]` table, and every
setting of an issuer lives on its table:

| Key | Default | Meaning |
|---|---|---|
| `issuer` | none, required | The authorization server's issuer identifier, an `https` URL with no query or fragment (RFC 8414 §2), or plain `http` on a loopback host; a token's `iss` must equal it |
| `kind` | none, required | Whose tokens the issuer signs: `persons` (the deployment's people, through its own clients, and the app-only tokens of its machines), `patients` (patients, each token naming only the record subject), `machines` (client credentials tokens only) or `federation` (another party's people, which needs its `[[auth.federation]]` table) |
| `profile` | `rfc9068` | The issuer profile the token shape and the caller classification start from: `rfc9068` (a standards-only server), `entra`, `keycloak`, `okta` or `nl`. Any profile key set on the table, such as `first_party_clients`, `delegates`, `token_types`, `required_claims`, `scope_claim` or `client_claims`, is set over it |
| `first_party_clients` | `[]` | The deployment's own clients: a person's token of a `persons` issuer is admitted only through one of these or a declared delegate, so list the clients your clinicians and operators sign in with, the operator console's included |
| `audience` | none, required | The audiences, a list, of which every token's `aud` must contain one, normally the facade's `base_url` (RFC 9068 §4) |
| `algorithms` | `["RS256", "RS384", "PS256", "PS384", "ES256", "ES384"]` | The signature algorithms a token may use; a symmetric `HS*` algorithm is refused and `none` is no algorithm |
| `clock_skew` | `"1min"` | The leeway `exp`, `nbf` and `iat` are checked with, at most `300` |
| `jwks_max_age` | `"10min"` | How long a key set is used before it is read again, so a key the issuer withdrew stops verifying |
| `jwks_refresh_interval` | `"1min"` | The shortest time between two reads of the key set that a token naming an unknown `kid`, or a failed read, triggers |
| `jwks_file` | none | A JWK Set file handed over out of band, read in place of the issuer's `jwks_uri`; the issuer's metadata is then not read |
| `timeout` | `"10s"` | How long one call to the authorization server may take |
| `role_claims` | `["roles", "groups", "entitlements"]` | The claims this issuer's roles are read from, each a dotted path into the token's JSON (the default is the three RFC 9068 §2.2.3.1 names). A path with an empty step or with whitespace refuses the start. The bridge grants nothing by role: no route it guards asks for one, so the roles a token carries change no answer |
| `mode` | `jwt` | `jwt` checks JWT access tokens against the issuer's key set; `introspection` asks the issuer's introspection endpoint |

Under the default `rfc9068` profile a token carries `typ: at+jwt` and the
claims `iss`, `exp`, `aud`, `sub`, `client_id`, `iat` and `jti` (RFC 9068
§2.2 and §4). The profile also decides who stands behind a token: a token
whose `sub` equals its `client_id` is a machine's client credentials token
(RFC 9068 §2.2), and any other token of a `persons` issuer is a person's,
admitted only through a client the table lists in `first_party_clients` or
`delegates`. The `keycloak` preset names no machine: its table lists the
`sub` of each service account in `app_only = [{ subjects = [...] }]`, and
without one refuses the start. A valid token that fits none of the issuer's kind is refused
with `401`. A token is checked by the issuer its `iss` names, and the
discovery document and the `CapabilityStatement` name the first issuer
checked by its key set.

```toml
[auth]
scopeless_subjects = [{ issuer = "https://auth.example.org/realms/clinic", sub = "service-account-ferrofed" }]

[[auth.issuer]]
issuer = "https://auth.example.org/realms/clinic"
kind = "persons"
profile = "keycloak"
audience = ["https://bridge.example.org/fhir"]
algorithms = ["RS256"]
first_party_clients = ["ferrobridge-console", "clinic-portal"]
app_only = [{ subjects = ["service-account-ferrofed"] }]

[[auth.issuer]]
issuer = "https://partner.example.net"
kind = "machines"
audience = ["https://bridge.example.org/fhir"]
role_claims = []
```

`[[auth.basic.users]]` tables admit HTTP Basic callers (RFC 7617) beside the
tokens. A Basic credential is always a person's, never a machine's: each
table names one person, takes `user`, the Argon2id PHC string of the password
as `password_hash` or in the file `password_hash_file`, and `scopes`, the
SMART `user/` scopes and the operator scope the person is granted, which the
facade checks as it checks a token's. A user listing a `system/` scope
refuses the start, naming the user and the scope. A user's `roles` list is
accepted and, as with `role_claims`, grants nothing on the bridge. `[auth.basic]
cache_ttl` (default `"5min"`) is how long a password that verified is
remembered, so a repeat request skips the Argon2id derivation; `"0s"`
remembers none.

```toml
[[auth.basic.users]]
user = "operator"
password_hash_file = "/run/secrets/operator.phc"
scopes = ["__ferrobridge.operator"]
```

A machine that calls the facade, such as an ETL client or another
FerroHEALTH product, holds a client credentials token (RFC 6749 §4.4) that
its authorization server issues: register it as a client of a `machines` or
`persons` issuer the section trusts, grant it the SMART `system/` scopes it
needs, and send the token as `Authorization: Bearer`. The bridge is a
resource server and issues no token itself.

At start-up the server reads each issuer's metadata, at
`/.well-known/oauth-authorization-server` inserted before the issuer's path and
on a `404` at the issuer followed by `/.well-known/openid-configuration`, and
the key set its `jwks_uri` names. Metadata naming another issuer, metadata
without a `jwks_uri`, a first issuer without a `token_endpoint`, and a key
set that cannot be read each refuse the start. A key set older than
`jwks_max_age` is read again before a token is checked against it. A
token whose `kid` the key set does not hold makes the server read the key set
once more, which is how a key rotation arrives, and never more often than
`jwks_refresh_interval`.

An authorization server whose access tokens are opaque, so the bridge cannot
check them against a key set, is checked by OAuth 2.0 Token Introspection
(RFC 7662) instead: `mode = "introspection"` on its `[[auth.issuer]]` table,
with an `[auth.issuer.introspection]` table. The bridge posts each token to
the issuer's introspection endpoint under its own credential, admits it when
the answer is `active: true` with `scope`, `client_id` and `exp` (SMART App
Launch 2.2.0, Token Introspection), checks an `iss` and an `aud` the answer
carries against `issuer` and `audience`, and reads the scopes and the
`patient` launch context as it reads a JWT's. An answer is held until its
`exp` and never longer than `cache_max_age`, so a revoked token
passes for at most that long. An inactive token is refused with `401`; an
endpoint that fails answers `503`. An introspected issuer takes no
`jwks_file`, and sits beside issuers checked by their key sets.

| Key | Default | Meaning |
|---|---|---|
| `introspection.endpoint` | the metadata's | The introspection endpoint, an `https` URL, when the issuer's metadata names none or another |
| `introspection.cache_max_age` | `"1min"` | The longest an active answer is used before the endpoint is asked again; `"0s"` asks on every request |
| `introspection.cache_capacity` | `10000` | The most answers held at once |
| `introspection.client` | none | The bridge's client authentication at the endpoint: the keys of [`[cdr.endpoint.credentials.oauth]`](#cdrendpointcredentialsoauth-and-terminologyendpointcredentialsoauth) (`client_id`, `client_auth`, `client_secret` or `client_secret_file`, `private_key_file`, `tls`); `issuer` defaults to the table's |
| `introspection.bearer` | none | Instead of `client`: a client credentials grant, in the same keys, whose token the bridge sends as `Authorization: Bearer`, the form SMART App Launch 2.2.0 has an EHR accept |

```toml
[[auth.issuer]]
issuer = "https://auth.example.org/realms/clinic"
kind = "persons"
audience = ["https://bridge.example.org/fhir"]
first_party_clients = ["clinic-portal"]
mode = "introspection"

[auth.issuer.introspection]
cache_max_age = "30s"

[auth.issuer.introspection.client]
client_id = "ferrobridge-rs"
client_secret_file = "/run/secrets/introspection-secret"
```

The `[[auth.issuer]]` tables live in the configuration file: an environment
variable sets one key, and `FERROBRIDGE__AUTH__ISSUER` set to a URL is refused,
since `issuer` is a list of tables. The section's own keys read from the environment, such as
`FERROBRIDGE__AUTH__ENABLED` and `FERROBRIDGE__AUTH__PRODUCT_SCOPE`.
Apart from the Basic users' hashes and the introspection credential the
section holds no secret: the resource server checks signatures with the
issuer's public keys.

### `[hl7v2]`

The HL7 v2 face: an MLLP listener whose messages are written into the CDR
through the facade's ingest service. It is off until `enabled` is true, and an
enabled face with the facade off is a boot error naming `hl7v2.enabled` and
`facade.enabled`. This lane reaches identifiable data. The
[HL7 v2 face](hl7v2-face.md) page states what it does with a message and how
it answers.

| Key | Default | Meaning |
|---|---|---|
| `enabled` | `false` | Whether the MLLP listener runs |
| `listen` | `127.0.0.1:2575` | The socket address the listener binds |
| `default_charset` | `ASCII` | The HL7 table 0211 code a message with an empty MSH-18 is read in |
| `unmapped_entries` | `skip_and_count` | `skip_and_count` or `refuse`, for an entry no program maps |
| `ehr_policy` | the facade's | `existing` or `create_on_first_write`, for the face alone |
| `profiles` | `[]` | `{ resource_type, profile }` pairs: the profile a resource of that R4 type claims when the guide wrote none |
| `idle_timeout` | `"5min"` | How long a connection may sit between messages; `"0s"` keeps it open |
| `frame_timeout` | `"30s"` | How long one frame may take to arrive whole; `"0s"` waits |
| `frame_limit` | `"1MiB"` | The largest message one frame may carry |
| `log_outcomes` | `false` | Whether an `AE` or `AR` also logs the counted outcomes by kind at debug level |
| `facility_identifier_issuer` | none | The issuer a facility's namespace id (HD.1) is written under, the issuer the CDR client's declared facilities name it by |
| `senders` | none, required | The MSH-4 sending facilities accepted, at least one: `{ namespace_id }` or `{ universal_id, universal_id_type }`, each with its `certificate_subject` and an optional `origin = "device"` with its `device_identifier_issuer` |
| `tls.certificate_file`, `tls.key_file`, `tls.client_ca_file` | none, required | The PEM chain and key the MLLP listener serves over TLS, and the authorities a sender's client certificate must chain to |

A resource type the R4 element table does not name, a profile that is not a
URL, a resource type listed twice, and a sender that names neither a
namespace id alone nor a universal id with its type are each a boot error
naming the key.

### `[change_feed]`

The source the change-feed consumers follow: the commits of the CDR, read
after each consumer's own watermark. It is off until `enabled` is true, and
an enabled feed is a boot error without a `[cdr]` section, and while no
consumer section is switched on. The [change feed](../integrate/change-feed.md)
page states what each source reads and what it cannot guarantee. No
specification governs the change feed: it is FerroBRIDGE's own design.

| Key | Default | Meaning |
|---|---|---|
| `enabled` | `false` | Whether the consumers follow the source |
| `source` | `aql` | `aql`, the poll of `/query/aql` that works against any CDR, or `ferroehr`, the reference CDR's outbox feed |
| `url` | none | `ferroehr` only, required: the root of the CDR's extensions, the path `/admin/outbox` lives under |
| `watermarks` | none | `aql` only, required: the `redb` file each reader's watermark is kept in |
| `start` | `now` | `aql` only: where a reader with no watermark starts, `now`, `beginning` or an RFC 3339 instant |
| `dead_letters` | none, required | The `redb` file the consumers park a poisoned event in and count its poisoned passes in; keep it beside `watermarks` |
| `max_attempts` | `5` | How many passes an event may fail with a poisoned failure before it is parked, 1 to 1000 |
| `page_size` | `100` | How many events one read asks for, 1 to 1000 |
| `poll_interval` | `"1s"` | How long a consumer waits after a read that found no full page |
| `backoff` | `"1s"` | How long a consumer waits after its first failed read, doubled per failure |
| `max_backoff` | `"1min"` | The longest wait after a failed read, at least `backoff` |

A missing `dead_letters`, an unknown `source`, a `start` that names no
instant, a `page_size` or `max_attempts` outside 1 to 1000, a wait of `"0s"`,
and a `url` that is no http or https base without a query or a fragment are
each a boot error naming the key. Both sources call
the CDR with the `[cdr]` credentials.

### `[events]`

The AMQP publisher of the commit events: a consumer of the change feed that
republishes each event to a RabbitMQ topic exchange. It is off until
`enabled` is true, and an enabled lane is a boot error without
`[change_feed] enabled`. The [events lane](../integrate/events.md) page
states the message, the routing keys and the delivery guarantees. No
specification governs the lane: it is FerroBRIDGE's own design.

| Key | Default | Meaning |
|---|---|---|
| `enabled` | `false` | Whether the lane publishes |
| `reader` | none, required | The reader name the lane follows the change feed under: `A-Z`, `a-z`, `0-9`, `_`, `-` and `.`, at most 128 characters |
| `url` | none, required | Secret. The AMQP 0.9.1 broker URL, `amqp://` or `amqps://` |
| `tls` | `false` | Whether an `amqp://` URL is upgraded to `amqps://` |
| `exchange` | none, required | The durable topic exchange the events are published to |
| `confirm_timeout` | `"30s"` | How long one publish waits for the broker's confirm |
| `queues` | `[]` | `{ name, binding }` pairs: the durable queues declared and bound to the exchange with each topic binding key |

A missing `reader`, `url` or `exchange`, a reader name outside its alphabet,
a URL that is not `amqp://` or `amqps://` (the refusal never repeats the URL,
which may carry a password), a `confirm_timeout` of `"0s"`, and a queue named
twice or with no binding are each a boot error naming the key. Over
`[change_feed] source = "ferroehr"`, a CDR that answers `404` for the reader
refuses the start.

### `[fhir_outbound]`

The outbound FHIR lane: a consumer of the change feed that maps each changed
composition to FHIR R4 through the facade's programs and delivers it to a FHIR
endpoint as one `transaction` Bundle per event. It is off until `enabled` is
true, and an enabled lane needs `[change_feed] enabled` and `[facade]
enabled`. It carries identifiable clinical data: the start logs a warning
naming the endpoint host. The [outbound FHIR lane](../integrate/fhir-outbound.md)
page states what an event delivers and the delivery guarantees. No
specification governs the lane: it is FerroBRIDGE's own design.

| Key | Default | Meaning |
|---|---|---|
| `enabled` | `false` | Whether the lane delivers |
| `reader` | none, required | The reader name the lane follows the change feed under, another than `[events] reader` and `[omop_incremental] reader`: `A-Z`, `a-z`, `0-9`, `_`, `-` and `.`, at most 128 characters |
| `cleartext` | `false` | Whether an `http` base is accepted, for a TLS terminator beside the bridge |
| `emitted` | none, required | The `redb` file the record of what was delivered for each composition is kept in, identifiers only |
| `endpoint.url` | none, required | The FHIR service base each Bundle is posted to, `https` without a query or a fragment, never the facade's own `base_url` |
| `endpoint.timeout` | `"30s"` | How long one delivery may take |
| `endpoint.connect_timeout` | unset | How long opening the connection may take; unset, only `timeout` bounds it |
| `endpoint.retry` | 3 attempts, `"200ms"` doubling to `"5s"` | `max_attempts`, `initial_backoff` and `max_backoff`: a delivery that got no answer or a `5xx` is tried again |
| `endpoint.tls` | the platform's roots | `trust_roots_file`, `client_identity_file` or `client_identity`, and `min_version` (`"1.3"`), for an endpoint behind a private root or one that asks for a client certificate |
| `endpoint.credentials` | none | `user` with `password` or `password_file`, or `[fhir_outbound.endpoint.credentials.oauth]`, the same table `[cdr.endpoint.credentials]` takes |

The `endpoint` table is the family's outbound endpoint table, sent through
`ferrohealth-smart`'s outbound client, which follows no redirect and renews a
token the endpoint refuses with `401` once.

A missing `reader`, `endpoint.url` or `emitted`, a reader name outside its
alphabet or another consumer's, an `http` base without `cleartext`, the
facade's own base, an `endpoint.timeout` of `"0s"`, and the lane without the change
feed or the facade are each a boot error naming the key. A record file that
cannot be opened refuses the start.

### `[omop_incremental]`

The incremental OMOP load: a consumer of the change feed that commits each
changed composition to the CDM database through the same step `etl run`
commits it with. It is off until `enabled` is true, and an enabled load is a
boot error without `[change_feed] enabled`, `[etl]`, `[cdm]` and an OMOCL
directory (`[mappings] omocl`, or `[operator] mapping_dir`). It reads the
`[etl]` concepts, visits, derived-table scope and deletion policy, and leaves
`[etl] aql` unused. The [OMOP ETL](../integrate/omop-etl.md#the-incremental-load)
page states what it reads and writes. No specification governs the
incremental load: it is FerroBRIDGE's own design.

| Key | Default | Meaning |
|---|---|---|
| `enabled` | `false` | Whether the load follows the change feed |
| `reader` | none, required | The reader name the load follows the change feed under, another than `[events] reader`: `A-Z`, `a-z`, `0-9`, `_`, `-` and `.`, at most 128 characters |

A missing `reader`, a reader name outside its alphabet or equal to the
events lane's, and a missing section the load runs over are each a boot error
naming the key. The OMOCL set that does not load and a CDM database the
concept resolver cannot reach refuse the start. Over `[change_feed] source =
"ferroehr"`, a CDR that answers `404` for the reader refuses the start.

### `[audit]`

The bridge's own access records: one Regulation (EU) 2025/327 Annex II 3.2
record per facade access an authenticated caller makes, and one system import
record per message the HL7 v2 face commits, each sent as a FHIR R4
`AuditEvent` over the IHE ITI-20 ATX: FHIR Feed to an Audit Record
Repository. It is off until `enabled` is true, and an enabled lane needs
`[facade] enabled` and `[auth]`. The [access record](access-record.md) page
states what each surface records and where. The section's shape is
FerroBRIDGE's own design.

| Key | Default | Meaning |
|---|---|---|
| `enabled` | `false` | Whether the facade and the HL7 v2 face record their accesses |
| `cleartext` | `false` | Whether an `http` base is accepted, for a TLS terminator beside the bridge |
| `endpoint.url` | none, required | The repository's FHIR base; each record is created as `[base]/AuditEvent` |
| `endpoint.timeout` | `"10s"` | How long one delivery to the repository may take |
| `endpoint.connect_timeout` | unset | How long opening the connection may take |
| `endpoint.retry` | 3 tries, `"200ms"` doubling to `"1min"` | The tries of one delivery, and `max_backoff` the wait between two rounds of tries |
| `endpoint.tls` | the platform's roots | `trust_roots_file`, `client_identity_file` or `client_identity`, and `min_version` (`"1.3"`) |
| `endpoint.credentials` | none | `user` with `password` or `password_file`, or `[audit.endpoint.credentials.oauth]`, the same table `[cdr.endpoint.credentials]` takes |
| `spool_dir` | none, required | The directory records wait in until the repository takes them; created when absent, and refused when its group or other users can reach it |
| `max_records` | `10000` | The most records the spool holds at once |
| `max_size` | `"64MiB"` | The most bytes the spool holds at once |
| `write_timeout` | `"2s"` | The longest storing one record may take |
| `source_id` | `[facade] system_id` | How the bridge names itself as the recording system (`AuditEvent.source.observer`) |
| `site` | none | The site the bridge runs at (`AuditEvent.source.site`) |
| `categories.national` | `[]` | The national categories the map may name, each `<system>\|<code>` |
| `categories.templates` | `{}` | Each template id to its Art 14(1) categories, such as `["Patient-Summaries"]`, or to `"none"` |
| `categories.archetypes` | `{}` | Each archetype id to its categories or to `"none"`; a template key wins for the same object |

Template and archetype keys compare without case, as openEHR BASE compares
every composite identifier. A missing `endpoint.url` or `spool_dir`, an `http`
base without `cleartext`, a zero bound, a category code that names no
category, and the lane without the facade or `[auth]` are each a boot error
naming the key. A spool directory that cannot be opened refuses the start.

### `[operator]`

The operator API under `/operator`, which an operator console reads the
engine state and the mapping files through. It is off until `enabled` is
true, and it needs `[auth]`: an `[operator]` section turned on while
`[auth]` is off, or names no issuer, refuses the start. The [operator API](operator-api.md) page
has the routes and the scope.

| Key | Default | Meaning |
|---|---|---|
| `enabled` | `false` | Whether the operator routes are mounted, each requiring a token that carries the operator scope, `[auth] product_scope` |
| `mapping_dir` | none | The writable directory console saves go to (`<mapping_dir>/fhirconnect/`); unset, the API checks and saves nothing. Keep it on its own volume when the root file system is read-only |

### `[operations]`

The `$tofhir` and `$toopenehr` lane. Both operations are pure transformations
and reach no CDR on a request, so they are served whenever `[mappings]
directory` is set and its templates load, from `[mappings] templates` or from
the CDR at boot. Without a mapping directory the two routes answer `503`.

| Key | Default | Meaning |
|---|---|---|
| `enabled` | `true` | Whether the two operations and their direct forms are served |
| `device_reference` | `Device/ferrobridge-<version>` | The `Provenance` `agent.who` a call that supplies no `context.who` gets |
| `composer` | `FHIRconnect` | The composition composer an inbound run fills in when no mapping does |
| `composition_language` | none | The composition language, an ISO 639-1 code |
| `composition_territory` | none | The composition territory, an ISO 3166-1 alpha-2 code |

The FHIRconnect engine chapter puts the composer and the context start time on
the engine and the composition language and territory on the project performing
the mapping, so set the last two: a template whose reference model requires
them refuses to build a composition without them, and `$toopenehr` then answers
an `OperationOutcome` naming the missing field.

### `[usage_report]`

The installation report `serve` sends to FerroPULSE. It is on by default and
sends nothing until `state_dir` names a writable directory, and a CI
environment (`GITHUB_ACTIONS` or `CI` set to `true` or `1`) keeps it off
whatever `enabled` says. [The usage report](usage-report.md) lists every field
it carries.

| Key | Default | Meaning |
|---|---|---|
| `enabled` | `true` | Whether `serve` sends the report; `FERROBRIDGE__USAGE_REPORT__ENABLED=false` switches it off |
| `endpoint` | `https://report.ferropulse.eu/v1/report` | The collector; an `https` URL, or `http` to a loopback host, with no credentials. Any other value refuses the start |
| `state_dir` | none | The writable directory the instance id is kept in, created when absent; without one the report is off |
| `deployment` | `unknown` | How the server was deployed, as the report states it: `helm`, `compose`, `binary` or `unknown`. The compose file sets `compose`; any other value refuses the start |

## The secrets, in one place

| Variable | File sibling |
|---|---|
| `FERROBRIDGE__CDR__ENDPOINT__CREDENTIALS__PASSWORD` | `FERROBRIDGE__CDR__ENDPOINT__CREDENTIALS__PASSWORD_FILE` |
| `FERROBRIDGE__CDR__ENDPOINT__CREDENTIALS__OAUTH__CLIENT_SECRET` | `FERROBRIDGE__CDR__ENDPOINT__CREDENTIALS__OAUTH__CLIENT_SECRET_FILE` |
| `FERROBRIDGE__TERMINOLOGY__ENDPOINT__CREDENTIALS__PASSWORD` | `FERROBRIDGE__TERMINOLOGY__ENDPOINT__CREDENTIALS__PASSWORD_FILE` |
| `FERROBRIDGE__TERMINOLOGY__ENDPOINT__CREDENTIALS__OAUTH__CLIENT_SECRET` | `FERROBRIDGE__TERMINOLOGY__ENDPOINT__CREDENTIALS__OAUTH__CLIENT_SECRET_FILE` |
| `FERROBRIDGE__CDM__URL` | `FERROBRIDGE__CDM__URL_FILE` |
| `FERROBRIDGE__EVENTS__URL` | `FERROBRIDGE__EVENTS__URL_FILE` |
| `FERROBRIDGE__FHIR_OUTBOUND__ENDPOINT__CREDENTIALS__PASSWORD` | `FERROBRIDGE__FHIR_OUTBOUND__ENDPOINT__CREDENTIALS__PASSWORD_FILE` |
| `FERROBRIDGE__FHIR_OUTBOUND__ENDPOINT__CREDENTIALS__OAUTH__CLIENT_SECRET` | `FERROBRIDGE__FHIR_OUTBOUND__ENDPOINT__CREDENTIALS__OAUTH__CLIENT_SECRET_FILE` |
| `FERROBRIDGE__AUDIT__ENDPOINT__CREDENTIALS__PASSWORD` | `FERROBRIDGE__AUDIT__ENDPOINT__CREDENTIALS__PASSWORD_FILE` |
| `FERROBRIDGE__AUDIT__ENDPOINT__CREDENTIALS__OAUTH__CLIENT_SECRET` | `FERROBRIDGE__AUDIT__ENDPOINT__CREDENTIALS__OAUTH__CLIENT_SECRET_FILE` |

Prefer the `_file` route: a Docker or Kubernetes secret arrives as a file, and
a file never shows up in `docker inspect` or a process listing.

## What a bad configuration does

The server refuses to start and prints the refusal on stderr naming the key,
the variable that set it and, for an unknown key, the closest known one, then
exits 78, `EX_CONFIG`; up to several refused keys are reported at once. These
are refused:

- an unknown key or an unknown section;
- a value of the wrong type, or one that is not a socket address, a URL, a
  release name or a `tracing` filter;
- `[server.caller_rate]`;
- a user beside the OAuth section of the same credentials section, an OAuth
  section with both an `issuer` and a `token_endpoint` or neither, an
  `issuer` that is not an `https` URL free of a query and a fragment, a
  `scope` that is not scope tokens, a `resource` with a fragment, and a
  `private_key_jwt` client without a readable `private_key_file`;
- an `[auth]` section with no issuer or audience while a guarded surface is
  served, an algorithm that is symmetric or unknown, a clock skew over 300
  seconds, a `token_profile` other than `rfc9068` and `jwt`, a Basic user
  whose hash is no Argon2id PHC string, a capability SMART App Launch 2.2.0
  does not define that is no full URI, a `scopes_supported` value that is
  not one scope token, and a `product_scope` outside the two SMART
  extension forms;
- a `[cdm] url` that names no `sslmode`, or one that falls back to plaintext
  or is not a libpq mode, a `[cdm]` CA that holds no PEM certificate, and a
  `PGSSLROOTCERT`, `PGSSLCERT` or `PGSSLKEY` in the environment;
- a secret set both inline and through its `_file` sibling;
- a section that is present and names no value for a key it needs.
- `[operator] enabled` while `[auth]` is off.

## An example

```toml
[server]
listen = "0.0.0.0:8080"

[telemetry]
format = "json"

[cdr]
endpoint.url = "https://cdr.example.org/ehrbase/rest/openehr/v1"

[cdr.endpoint.credentials]
user = "ferrobridge"
password_file = "/run/secrets/cdr-password"

[terminology]
endpoint.url = "https://tx.example.org/r4"
wire_version = "r4"

[mappings]
directory = "/etc/ferrobridge/mappings"

[facade]
enabled = true
base_url = "https://bridge.example.org/fhir"
ehr_policy = "create_on_first_write"
identity_store = "/var/lib/ferrobridge/identity.redb"
subject_namespace = "https://example.org/fhir/sid/patient"
composition_language = "en"
composition_territory = "GB"

[auth]
issuer = "https://auth.example.org/realms/clinic"
audience = ["https://bridge.example.org/fhir"]
```
