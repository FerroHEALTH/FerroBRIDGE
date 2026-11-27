<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# Securing the facade

**Audience:** integrators building a FHIR client or a SMART app, and operators
who configure `[auth]`.

How the facade authenticates a caller, which SMART permission each interaction
needs, and how a browser app reaches it across origins. The keys are on
[Configuring the server](../operate/configuration.md#auth).

<!-- toc -->

## Security

With [`[auth]`](../operate/configuration.md#auth) on, the default, the facade
is a SMART App Launch 2.x resource server for the authorization servers the
deployment trusts, each a named `[auth.issuer.<name>]` table fixed by its
issuer. A token is for the facade when its `aud` names `[facade] base_url`, or
one of the audiences the issuer table lists in its place.
The check is `ferrohealth-smart`'s, the resource server every FerroHEALTH
product runs. **With `[auth]` off, the
facade authenticates nobody and serves every request it receives.**

Every request under `/fhir` carries `Authorization: Bearer <access token>`,
or the Basic credential of a user `[[auth.basic.users]]` lists, except `GET /fhir/metadata` and the SMART discovery document at
`GET /fhir/.well-known/smart-configuration` (also served at
`/.well-known/smart-configuration`), which a client reads before it holds a
token. The health routes stay open too. The token is a JWT signed by a key
in the issuer's key set, with an `iss` equal to the issuer, an `aud` naming
the facade, and an `exp`, `nbf` and `iat` that hold within the configured
clock skew. It also carries `typ: at+jwt` or `application/at+jwt`, `sub`,
`client_id`, `iat`, `jti` and `scope` (RFC 9068). The authorization server's
client registry decides which clients reach the facade, and the token's
standard claims decide who the caller is: a machine's client credentials
token, whose `sub` is its `client_id`, a user named by the type of their
`fhirUser` with an actor named by `act`, or another party's person on a
`federation` issuer. A Basic credential is always a user's; a machine never
authenticates with one.

| Answer | When | `WWW-Authenticate` |
|---|---|---|
| `401` | No credential, or one in a scheme the server does not admit | `Bearer`, and a `Basic realm="", charset="UTF-8"` challenge beside it when Basic users are configured |
| `400` | Two credentials, or an `Authorization` value that is no credential | `Bearer error="invalid_request"` |
| `401` | A token that does not hold, whatever the reason, including a token of another issuer or for another audience, and a valid token whose claims name no caller | `Bearer error="invalid_token", error_description="<reason>"` |
| `401` | A Basic credential that does not hold, whatever the reason | `Basic realm="", charset="UTF-8"` |
| `403` | A valid token whose scopes do not permit the request | `Bearer error="insufficient_scope", error_description="scope-not-granted"` |
| `403` | Any request to a route under `/fhir` the guard does not classify, before the token is read | none |
| `503` | A token naming a key the server cannot fetch because the key set does not answer | none |

Each refusal carries an `OperationOutcome`, `login` for a `401`, `invalid` for
a `400` and `forbidden` for a `403`. A configured `realm` is named on every
challenge. A `Bearer` challenge with an error code carries the stable reason
code of the check that refused the token as its `error_description` (RFC 6750
§3), such as `expired`, `audience-mismatch`, `issuer-unknown`,
`signature-invalid` or `scope-not-granted`; the `ferrohealth-smart` book lists
every code. The bridge logs the code with the token's issuer, client and key
id, never the token. Neither the challenge nor the body names the claim that
failed or the scope that was missing.

The guard classifies every route it admits by name: the interactions in the
table below, the two FHIRconnect operations and their direct forms, the
`CapabilityStatement` and the discovery document. A route under `/fhir` that
it does not name fails closed with `403` and reaches no handler and no CDR,
so a route added without a classification is refused rather than open to any
token. A test reads every route the server mounts under `/fhir` and fails
when one has no classification. With `[auth]` off the facade stays open.

The permission each interaction needs follows the SMART v2 `cruds` letters,
on the resource type in the path or in the entry:

| Interaction | Needs |
|---|---|
| create | `c`, and `u` too when the create revises a resource the facade already holds (a new `meta.versionId` of a known `id`); a replay that changes nothing needs `c` only |
| read, vread, history (a party resource) | `r` |
| `Composition/$document` | `r` on `Composition` and on every type the document carries |
| delete (a party resource) | `d` |
| search (`GET [base]/{type}`, `POST [base]/{type}/_search`) | `s` |
| update | `u` |
| `$validate` | `c` or `u`, or the operator scope (`[auth] product_scope`): the dry run commits nothing, so an operator console needs no clinical write scope |
| transaction | the permission of each entry's `request.method`: `c` for `POST`, `u` for `PUT`, `r` for `GET`, `d` for `DELETE` |
| batch | per entry, the permission of the interaction its `request` names: `c` for a create, `u` for an update, `r` for a read or vread, `s` for a search |
| `$tofhir`, `$toopenehr` | a valid token, no clinical scope, the operator scope included: they transform the request body and reach no CDR |

A scope reads `patient/`, `user/` or `system/`, then a resource type or `*`,
then the letters in the order `cruds`, such as `system/*.crs` or
`patient/Condition.rs`. The SMART v1 suffixes are read as the spec maps them:
`.read` as `rs`, `.write` as `cud` and `.*` as `cruds`. A scope with letters
out of order, an unknown resource type, or a `?` search constraint grants
nothing; the facade does not evaluate the constraint, so it does not grant
more than the scope says. A `user/` scope reaches every resource of its type,
because the facade has no per-user access model of its own, and so does a
`system/` scope. A token with no clinical scope reaches nothing.

A `patient/` scope reaches only the patient in the token's `patient` claim,
the launch context the authorization server sets (SMART App Launch 2.2.0,
Scopes and Launch Context, `launch/patient`).
The facade reads a patient's data as the EHR it resolves for the subject
`Patient/<id>`: a create or update must name `subject` (or `patient`) as
the literal reference `Patient/<id>`, and a read reaches a resource only
when the EHR holding its composition belongs to that subject. A search under
a `patient/` token is held to that patient's EHR, whatever subject parameters
it names, and answers an empty searchset when the identity map binds the
patient no EHR. A resource
whose subject is named by identifier cannot be shown to be the patient's, so
a patient-scoped token is refused for it. A `patient/` scope without a
`patient` claim grants nothing.

A transaction is authorised entry by entry before anything is mapped, and one
entry the token does not cover refuses the whole Bundle with `403`, because a
transaction succeeds or fails as one unit. A batch is authorised entry by
entry too, and an entry the token does not cover, or one outside a
`patient/` token's compartment, answers `403 Forbidden` inside the
`batch-response` while the other entries run.

Every request under `/fhir` but the discovery document leaves one audit line
on the `ferrohealth_smart::audit` log target, the record every FerroHEALTH
product writes: the time, the token's `sub` or the Basic user as
`principal`, the token's `client_id` and issuer, the caller's `kind`
(`person`, `patient`, `system`, `delegated` and the other kinds
`ferrohealth-smart` names; a Basic user is always `person`), the scheme the
credential arrived with as `via` (`bearer` or `basic`), what the login
resolved to as `binding` and `party` (`-` while the bridge binds no party),
the method, the matched route,
what was addressed (`Condition/c-1`, or the type alone), the interaction,
the status and the duration. Beside the log envelope its fields are exactly
`at`, `request_id` (the `X-Request-Id` that ties the line to the request
log), `principal`, `client`, `kind`, `via`, `issuer`, `binding`, `party`,
`method`, `route`, `addressed`, `interaction`, `status` and `duration_ms`. A refused
credential is recorded as `unauthenticated` with `via` `none`, and with
`[auth]` off the principal reads `-` with `via` `open`. A batch leaves one
line per entry, carrying the entry's interaction, what it addressed and its
status and its position in the Bundle as `entry`, then the request's own
line with the interaction `batch`. The line
never carries the token, another claim, a query value or a body.

The bridge never forwards the caller's token to the CDR, because an access
token is bound to the audience it was issued for. By default it reaches the
CDR with its own credential
([`[cdr.endpoint.credentials.oauth]`](../operate/configuration.md#cdrendpointcredentialsoauth-and-terminologyendpointcredentialsoauth)),
so the CDR records the bridge as the actor and the caller's identity lives in
the bridge's audit line. With
[`[cdr.endpoint.credentials.onward]`](../operate/configuration.md#cdrendpointcredentialsonward)
it exchanges the caller's token per request for a token of the CDR's issuer
that names the same person (RFC 8693 token exchange, or the Entra ID
on-behalf-of grant), so the CDR records the clinician too; a machine keeps the
bridge's own token, and a caller the exchange cannot carry is refused. The
audit line still names the caller.

### Browser apps (CORS)

A purely browser-based SMART app calls the facade from another origin, so the
browser asks the facade, through cross-origin resource sharing (CORS), whether
the page may read each answer. SMART App Launch 2.2.0 (App Launch,
Considerations for Cross-Origin Resource Sharing support) says a server that
supports such apps "SHALL enable" CORS for any origin on the public discovery
endpoints and for a client's registered origins on the FHIR REST API, and FHIR
R4 (Security, Communications) asks the same of a server with browser clients.
The facade answers both:

| Route | Origins | `Access-Control-Allow-Origin` |
|---|---|---|
| `GET /fhir/metadata`, `GET /fhir/.well-known/smart-configuration`, `GET /.well-known/smart-configuration` | Any | `*` |
| Every other route under `/fhir` | Those [`[facade.cors] allowed_origins`](../operate/configuration.md#facadecors) lists | The request's `Origin`, with `Vary: Origin` |

The facade holds no client registry, so the registered origins are the list
you configure. With the list empty, the default, the REST routes answer no
origin and a browser withholds every answer from a page on another origin.
No answer ever carries `Access-Control-Allow-Credentials`: an app sends its
token in the `Authorization` header, never in a cookie.

The facade answers the preflight (`OPTIONS` with
`Access-Control-Request-Method`) of an admitted origin itself, before the
token check, because a browser sends a preflight without the token. The
answer is `204` with the methods the facade mounts (`GET` on the discovery
endpoints; `GET`, `POST`, `PUT` and `DELETE` on the REST routes), the request
headers it reads (`Authorization`, `Content-Type`, `Accept`, `Prefer`, the
conditional headers and `X-Request-Id`) and a ten-minute `Access-Control-Max-Age`.
A preflight from an origin the list does not name runs as any `OPTIONS` does:
the guard refuses it with `403`, and the browser blocks the request it
announced. An admitted origin also reads a refusal, such as the `401` of a
missing token, with `Location`, `ETag`, `Last-Modified`, `Content-Location`,
`WWW-Authenticate` and `X-Request-Id` exposed to the page.
