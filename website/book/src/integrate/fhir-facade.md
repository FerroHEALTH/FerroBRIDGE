<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# The FHIR facade

The FHIR side of FerroBRIDGE is a REST facade in front of an openEHR CDR. A
FHIR client sees one FHIR server; behind it, every request becomes CDR
operations driven by FHIRconnect mappings. The facade stores no clinical data
of its own.

The facade runs. It is off until `[facade] enabled` turns it on, and a disabled
facade mounts no route, so a request answers `404` rather than `403`. What this
page describes is what the server answers today: conformance, create, read,
vread, update, search, transaction, batch and `$validate`.

<!-- toc -->

## Why a facade

FHIRconnect defines the mapping language and leaves the server surface to the
implementer. Two shapes exist in the prior art: publish a facade, or leave both
the facade and the CDR to the integrator. No standard prefers either.
FerroBRIDGE publishes the facade so that a FHIR client needs to know about one
server and one base URL, and so that the mapping direction, the terminology
calls, and the CDR commit happen in one place that can refuse a request as a
whole.

## The surface

FHIR R4 (4.0.1), because that is the only version the FHIRconnect schemas admit
for `spec.version`. The service base is `/fhir`.

| Request | What it does |
|---|---|
| `GET /fhir/metadata` | The `CapabilityStatement` of the loaded mapping set |
| `POST /fhir/{type}` | Create, conditional through `If-None-Exist` |
| `GET /fhir/{type}?…` | Search the type over the AQL its search declaration states |
| `POST /fhir/{type}/_search` | The same search, its parameters in a form body |
| `POST /fhir/{type}/$validate` | The dry run that commits nothing |
| `GET /fhir/{type}/{id}` | Read one resource back out of its composition |
| `GET /fhir/{type}/{id}/_history/{vid}` | Vread: read the version a write's `Location` named |
| `PUT /fhir/{type}/{id}` | Update, with `If-Match` |
| `POST /fhir` | A `transaction` Bundle, all or nothing, or a `batch` Bundle, each entry on its own |
| `GET /fhir/.well-known/smart-configuration` | The SMART discovery document, with `[auth]` on |

A type no loaded program maps is absent from the `CapabilityStatement` and
answers `404` with a `not-supported` `OperationOutcome` on the wire. A search on a type no search
declaration answers is a `404` with `not-supported`, and a `_count` that is not
a non-negative integer is a `400` with `invalid`, so a client never reads a
page it did not ask for.

The `CapabilityStatement` names exactly the resource types the loaded programs
map, with `create`, `read`, `vread` and `update` per type, `transaction` and
`batch` at system level, the `$validate` operation, `updateCreate: false`,
`conditionalCreate` true, `conditionalUpdate` false (no `PUT [type]?[search]`
is served; `If-Match` on an update is the version-aware update), and
`fhirVersion: 4.0.1`. A type a search
declaration answers also names `search-type` and, in `searchParam`, exactly
`_id` and the parameters the declaration states; every other type names no
search parameter.
When the FHIRconnect operations lane is served under the same base,
`rest.operation` also names `$tofhir` and `$toopenehr`; their direct forms are
never declared ([The FHIRconnect operations](operations.md) says why).
With `[auth]` on, `rest.security` names the `SMART-on-FHIR` service and
carries the `oauth-uris` extension with the authorization server's
endpoints, for clients that discover SMART through the statement (see
[Security](#security)).

## Security

With [`[auth]`](../operate/configuration.md#auth) on, the facade is a SMART
App Launch 2.x resource server for one authorization server, fixed by its
issuer and the audience the deployment configures. **With `[auth]` off, the
facade authenticates nobody and serves every request it receives.**

Every request under `/fhir` carries `Authorization: Bearer <access token>`,
except `GET /fhir/metadata` and the SMART discovery document at
`GET /fhir/.well-known/smart-configuration` (also served at
`/.well-known/smart-configuration`), which a client reads before it holds a
token. The health routes stay open too. The token is a JWT signed by a key
in the issuer's key set, with an `iss` equal to the issuer, an `aud` naming
the audience, and an `exp`, `nbf` and `iat` that hold within the configured
clock skew. Under the default `rfc9068` profile it also carries `typ: at+jwt`,
`sub`, `client_id`, `iat` and `jti` (RFC 9068).

| Answer | When | `WWW-Authenticate` |
|---|---|---|
| `401` | No bearer token, or a token in another scheme | `Bearer` |
| `401` | A token that does not hold, whatever the reason, including a token of another issuer or for another audience | `Bearer error="invalid_token"` |
| `403` | A valid token whose scopes do not permit the request | `Bearer error="insufficient_scope"` |
| `403` | Any request to a route under `/fhir` the guard does not classify, before the token is read | `Bearer error="insufficient_scope"` |
| `503` | A token naming a key the server cannot fetch because the key set does not answer | none |

Each refusal carries an `OperationOutcome`, `login` for a `401` and
`forbidden` for a `403`. Neither names the claim that failed or the scope
that was missing.

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
| read, vread | `r` |
| search (`GET [base]/{type}`, `POST [base]/{type}/_search`) | `s` |
| update | `u` |
| `$validate` | `c` or `u` |
| transaction | the permission of each entry's `request.method`: `c` for `POST`, `u` for `PUT`, `r` for `GET`, `d` for `DELETE` |
| batch | per entry, the permission of the interaction its `request` names: `c` for a create, `u` for an update, `r` for a read or vread, `s` for a search |
| `$tofhir`, `$toopenehr` | a valid token, no clinical scope: they transform the request body and reach no CDR |

A scope reads `patient/`, `user/` or `system/`, then a resource type or `*`,
then the letters in the order `cruds`, such as `system/*.crs` or
`patient/Condition.rs`. The SMART v1 suffixes are read as the spec maps them:
`.read` as `rs`, `.write` as `cud` and `.*` as `cruds`. A scope with letters
out of order, an unknown resource type, or a `?` search constraint grants
nothing; the facade does not evaluate the constraint, so it does not grant
more than the scope says. A `user/` scope reaches every resource of its type,
because the facade has no per-user access model of its own, and so does a
`system/` scope. A token with no clinical scope at all gets system-wide
access only when its `sub` is in `scopeless_subjects`.

A `patient/` scope reaches only the patient in the token's `patient` claim.
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

Every request under `/fhir` leaves one audit line on the `ferrobridge::audit`
log target: the time, the token's `sub` and `client_id`, the method, the
interaction, the resource type and id, the status and the duration. A
refused token is recorded as `unauthenticated`, and with `[auth]` off the
subject reads `-`. A batch leaves one line per entry, carrying the entry's
interaction, resource type, id and status and its position in the Bundle as
`entry`, then the request's own line with the interaction `batch`. The line
never carries the token, another claim, a query value or a body.

The bridge reaches the CDR with its own credential
([`[cdr.credentials.oauth]`](../operate/configuration.md#cdrcredentialsoauth)),
never with the caller's token, because an access token is bound to the
audience it was issued for. The CDR therefore records the bridge as the actor,
and the caller's identity lives in the bridge's audit line.

## Media types and bodies

The facade reads `application/fhir+json` and `application/json`, which R4 names
as the JSON media type and its alias, and answers `415` for anything else. Every
response carries `application/fhir+json`. An `Accept` header that admits neither
is a `406`.

A body is parsed through the strict codec of the generated `fhir-types` crate,
so an unknown property is a `400` with `structure` rather than a value that is
silently dropped. `Prefer: return=minimal`, `return=representation` and
`return=OperationOutcome` are honoured on a write, and the default is the
resource itself.

Everything the facade authors is an `OperationOutcome` with an `issue.code`
from the R4 value set. An openEHR error body never reaches the wire as its own
document: the CDR's `message` and every `validationErrors` entry travel
verbatim inside `issue.diagnostics`, which keeps one error vocabulary on the
wire while losing nothing the CDR said.

## Program selection

A request is mapped by the program whose context claims the profiles the
resource declares in `meta.profile`. Every declared profile is considered, as
set membership, and the `templateId` pin of the FHIRconnect operations surface
selects among candidates when a deployment sends one.

Two refusals are distinct on the wire, and both are `422`. When no program
claims the resource, the outcome names the profiles it saw and answers
`not-supported`. When several programs claim it and nothing pins the template,
the outcome names the candidates, so the fix is a pin rather than a guess.

## Identity

A FHIR resource id has to be stable across composition versions, because R4
fixes that "once assigned, this value never changes". The facade derives it
from the entry's `LOCATABLE.uid` when the entry carries one that the R4 `id`
grammar admits, and otherwise from a SHA-256 over the version container, the
entry path and the split occurrence, rendered as 52 lowercase base32
characters. `meta.versionId` carries the openEHR `version_tree_id` and never
enters the derivation, so a new version of a composition reads back under the
same id with a new `meta.versionId`. `meta.source` names the openEHR version
uid the answer was read from.

The identity map is a `redb` file with seven tables: a patient identifier to its
`ehr_id` and each `ehr_id` back to the first person recorded for it, an
external resource id to the internal one, an internal id to the
composition version container with the entry path and split occurrence, and the
source resource `id` with its `meta.versionId` to the mapping that consumed
them, beside the contribution each source was committed in and each
`Resource.identifier` of a committed resource. The map wins once written, so a derivation change never renames a
resource a client already holds. The store keeps identifiers and nothing else:
no clinical content is written into it, and a test greps the file for a mapped
value to prove it.

That last table is what makes create idempotent. A resource re-sent with the
same `id` and `meta.versionId` commits nothing and answers `200 OK` with the
composition it already produced. A resource with a known `id` and a new
`meta.versionId` commits a later version of that composition. A resource with a
known `id` and no `meta.versionId` is a replay when it maps to the content the
composition holds now, and a later version otherwise. A transaction entry with
a known `id` and a new `meta.versionId` commits that later version inside the
Bundle's contribution. The Operate page on failure and identity states the
full rule.

## A read is a valid instance

Every resource the facade answers, on a read, a vread or a write under
`Prefer: return=representation`, is a valid R4 instance: each element the R4
element table marks `min 1` is present, at the top of the resource and inside
every element and contained resource it carries. A mapping often has no
outbound row for the subject, because the ingest reads the subject to find the
EHR and the composition does not hold it. When the rendered resource has no
`subject` (or `patient`, on a type that names its subject so), the facade
writes it from the identity map: the person the map recorded for the
composition's EHR, as a literal reference when the person was keyed by one in
`subject_namespace`, and otherwise as a logical reference with `type: Patient`
and the person's `identifier`. That is the form a create reads back into the
same EHR, so you can send a read body back as an update unchanged. A subject
the mapping wrote is kept. The log line of the request counts the elements the
facade filled and names their paths, never a value. When a required element
stays absent, the facade answers `500 exception` with the element path in
`issue.location` rather than an invalid resource. No specification governs the
fill: it is FerroBRIDGE's own design.

## Provenance

Every composition the facade commits carries a `FEEDER_AUDIT`, which is the
reference model's own element for data transformed into openEHR form: the
source resource's `id` and type as an `originating_system_item_ids` entry, its
`meta.versionId` as `originating_system_audit.version_id`, and the configured
`system_id` naming the bridge. A source resource that carries no `id` is
recorded as unknown, never given one. So a reader of the CDR can tell which
FHIR resource a composition came from without asking the bridge.

## Writes

A create resolves the subject to an EHR: the identity map first, then the CDR
by subject id and namespace, and only then the configured policy. With
`ehr_policy = "existing"` an unknown subject is refused; with
`create_on_first_write` the facade creates the EHR and records it.

`If-None-Exist` behaves as R4 defines it: no match creates, one match answers
`200` with the resource that already exists, and several matches answer `412`.
The search it runs is over the identity map, so the parameters it answers are
`_id` and `identifier`, each in the comma-separated OR form. `identifier`
matches the `Resource.identifier` of every resource the facade committed, in
the token forms `[code]`, `|[code]` and `[system]|[code]`; the `[system]|`
form is refused. Any other parameter is refused, because a silently narrowed
search would turn a duplicate into a second composition.

A create answers `201` with `Location` naming the FHIR resource under the
configured base URL and `ETag` carrying the version.

That `Location` is `[base]/{type}/{id}/_history/{vid}`, and a `GET` of it is
the R4 vread. The `{vid}` is the version tree id of the composition version
the write committed, so the facade reads that exact version from the CDR and
answers it with its `ETag`. A `{vid}` the CDR does not hold answers `404`, and
a version the CDR reports deleted answers `410`.

An update needs `If-Match` when the map knows the id. Without one, the facade
reads the CDR's current `ETag` and answers `412` with it on a mismatch rather
than overwriting a version the client never saw. Send back the `ETag` a read
or write answered, `If-Match: W/"1"`: the facade completes that `versionId` to
the current composition version and answers `412` with the current `ETag`
when it is stale. A value that names no version is `400`. The body must
name the id the URL names: a body with no `id`, or another one, is `400`. A `PUT` to an id the map does
not know is a `404`: this milestone does not upsert, and the
`CapabilityStatement` says `updateCreate: false`.

A `transaction` Bundle maps every entry first and commits the lot through one
ITS-REST contribution so the CDR makes it atomic. The facade uses `fullUrl`
in two places. It names each entry by its `fullUrl` in every refusal, or by
its position when it has none. And a `subject` or
`patient` reference whose value is another entry's `fullUrl` is read through
that entry: the person is that entry's first `identifier` with a value. The
facade resolves no other reference. A reference is mapped as the resource
carries it, and the facade never fetches a referenced resource from anywhere.
Any failure answers one `OperationOutcome` naming every failing entry, and
nothing is committed.

A committed transaction answers `200` with a `transaction-response` Bundle,
one entry per request entry in the same order. Each entry's `response` carries
`201 Created`, the `location` `[base]/[type]/[id]/_history/[vid]` and the
`etag` `W/"[vid]"`, the values a single create answers. Send the same Bundle
again and nothing is committed: each entry answers `200 OK` with the location
of the resource the first delivery created. A Bundle only some of whose
entries an earlier delivery consumed is refused with `409`, naming those
entries, and a Bundle that carries one resource twice is refused with `422`.
A delivery that arrives while another delivery of the same Bundle or resource
is still in flight is refused with `409` and commits nothing; retry it after
the first one answers. The redelivery rule is on the
[failure and identity](../operate/failure-and-identity.md) page.

A `batch` Bundle runs each entry as the single interaction its `request`
names, and answers `200` with a `batch-response` Bundle, one entry per request
entry in the same order (R4 http.html §3.1.0.11). The entries run in the order
R4 gives for a transaction, every `POST` before every `PUT` and every `GET`
last, and the answer keeps the Bundle's order.

| Entry `request` | Runs as |
|---|---|
| `GET [type]/[id]` | read |
| `GET [type]/[id]/_history/[vid]` | vread |
| `GET [type]?…` | search |
| `POST [type]` | create, with `ifNoneExist` as its `If-None-Exist` |
| `PUT [type]/[id]` | update, with `ifMatch` as its `If-Match` |

Each entry's `response` carries the status the single interaction answers,
with its `location` and `etag`. A successful entry carries its resource, or
none under `Prefer: return=minimal`, which reaches every entry, and a failed
entry carries its `OperationOutcome` in `response.outcome`. A CDR refusal of
one entry is that entry's status through the status table, and the other
entries still run. A `DELETE`, `PATCH` or `HEAD` entry answers `405`, a
`request.url` the facade routes no interaction for answers `404`, and an
absolute `request.url` under another server's base answers `400`. An entry
whose resource references another entry's `fullUrl` answers `400 invalid`,
because R4 calls such a batch non-conformant; the other entries run. A Bundle
with an entry that carries no `request`, or a `POST` or `PUT` entry with no
resource, is refused whole with `400`, naming each such entry, and nothing
runs.

An HL7 v2 message that the [HL7 v2 face](../operate/hl7v2-face.md) receives
goes through this same path as a transaction of its Bundle's entries, so the
same identity and replay rules apply to it. Its entries are keyed by the
message's MSH-10 and message type and their position in the Bundle, so a
message sent again commits nothing and is answered `AA`, as its first delivery
was. A message whose delivery overlaps another of the same message is refused
and commits nothing. Each composition's `FEEDER_AUDIT` names the message where
a resource would be named here. A subject reference to another entry of the
Bundle is read through that entry: the person is the referenced resource's
first `identifier`, the same pair a `subject.identifier` names.

## `$validate` is a dry run of this server

`POST /fhir/{type}/$validate` runs the whole inbound path up to the commit and
commits nothing, which a test proves by counting EHRs and versions before and
after. It answers `200` for both verdicts: `information` issues when the
resource would commit, and an `error` issue carrying the validator's message
verbatim when it would not. The operation-level statuses mirror the create
path, so a `415`, a `422` for an unmapped profile or a `404` for an unknown
type reads the same on both routes.

The validator is this server, not the CDR. openEHR ITS-REST 1.1.0 documents no
composition-validation route, and a commit the bridge never finalises would
still write a version, so what runs is the mapping engine, the Web Template
validation the composition build performs, and the strict RM reader. Every
outcome says so in `issue.details`, so you can read what a verdict covers. A
CDR that refuses a composition this validator accepts still answers `422` on
the create.

## Status mapping

No specification governs what a CDR answer becomes on the FHIR wire, so the
mapping is FerroBRIDGE's own design. It lives in one table, each row citing
both sides, and one wire test asserts each row.

| The CDR answers | The facade answers |
|---|---|
| `422`, template validation | `422` with `validationErrors` verbatim in `issue.diagnostics` |
| `412` | `412` carrying the current `ETag` |
| `204` on a read, the composition is deleted | `410` |
| `404` | `404` |
| `401` | `401`, propagating `WWW-Authenticate`, never `403` |
| `400`, `405` or `415` | `500`: the bridge chose a call the CDR does not offer |
| any `5xx` | `502` carrying the upstream status |
| `400` on a search query | `500`: the bridge wrote the AQL the CDR refused |
| `408` on a search query | `502` carrying the upstream status |
| no answer: a connect failure or a timeout | `502` |
| no call: the token endpoint for the CDR credential fails or refuses | `502` naming the token endpoint's refusal, never the secret |
| `401`, and the token endpoint refuses the one renewal | `502` naming the token endpoint's refusal, never the secret |

Two answers come from the server itself, before or around the handler, and
under `/fhir` both are an `OperationOutcome`: a body over
`body_limit_bytes` answers `413` with the `too-long` issue, and a request that
outlasts `request_timeout_ms` answers `504` with the `timeout` issue. The
`504` replaces the plain `408` other routes answer, because a `408` tells a
client it may resend a write the CDR may already have committed. A handler
that fails unexpectedly answers `500` with the `exception` issue and the
request id in `diagnostics`; the cause stays in the server log.

## Paths are written as well as read

The engine is a bidirectional path model over FHIR JSON, because a mapping has
to construct a resource as well as read one. The model is guided by the element
table of the generated `fhir-types` crate, so it knows which elements repeat,
which are choice types, and which alternatives a choice admits. The three
read-side FHIRPath forms the published mappings use (`ofType()` and `as()`,
`extension(url)`, `resolve()`) are path-model operations; no FHIRPath evaluator
is embedded. FHIRconnect's `^` parent operator and `$fhirRoot` are not FHIRPath,
and the path model resolves them when the mapping is compiled.

## Terminology

The facade calls a configured FHIR terminology server for three operations:
`$lookup` to resolve a code's display, `$translate` for a `conceptmap`
reference, and `$validate-code`. The `$lookup` case is the common one, because
openEHR requires `DV_CODED_TEXT.value` while FHIR leaves `display` optional. A
terminology failure is an error, never a coding with a missing display.

The facade does not call the terminology server yet. The call site is the
inbound engine seam, and it runs the calls before anything is built, so a
terminology refusal will cost no CDR write. It arrives with the registry of
mapping functions in
[issue #95](https://github.com/FerroHEALTH/FerroBRIDGE/issues/95); until then
a mapping that needs a display resolved refuses the unit rather than writing a
coding without one.

## Search

FHIRconnect states that it "does not focus specifically on AQL and FHIRsearch",
so no specification governs FHIR search over an openEHR CDR. The surface below
is FerroBRIDGE's own design, built on the R4 search page for everything the
client sees.

### Declaring search beside a context

A context answers search only when a search declaration names it. The
declaration is a YAML file in the mapping directory whose name ends in
`.search.yml` or `.search.yaml`; the FHIRconnect loader skips those files, and
the facade reads them at boot after the mapping set compiles.

```yaml
context: condition.problem-list        # metadata.name of the context mapping
patient:
  parameters: [patient, subject]       # the names that address the EHR's subject
  required: true                       # a search without one is a 400
order:                                 # the total order every page is read in
  - v/commit_audit/time_committed/value
  - v/uid/value
parameters:
  code:
    type: token
    node: { class: EVALUATION, archetype: openEHR-EHR-EVALUATION.problem_diagnosis.v1 }
    path: data[at0001]/items[at0002]/value      # the coded text, relative to the node
    systems:
      "http://snomed.info/sct": SNOMED-CT
  onset-date:
    type: date
    node: { class: EVALUATION, archetype: openEHR-EHR-EVALUATION.problem_diagnosis.v1 }
    path: data[at0001]/items[at0077]/value/value
  recorded:
    type: date
    path: context/start_time/value             # no node: read under the composition
```

A parameter names the node it is read under with `node`, the RM class and
the archetype id of an archetype root, and gives its `path` relative to that
node. A parameter with no `node` is read under the composition. A `token`
names the coded value itself, a `DV_CODED_TEXT`, a `DV_TEXT` or a
`CODE_PHRASE`, and the facade reads its code and its system from that one
value, so `[system]|[code]` never pairs the system of one diagnosis with the
code of another. A `string` or a `date` names a data value, with any
attribute below it.

The facade writes the query itself:
`SELECT e/ehr_id/value, v/uid/value, c FROM EHR e CONTAINS VERSION v[LATEST_VERSION] CONTAINS COMPOSITION c`,
restricted to the context's template through
`c/archetype_details/template_id/value`. Each occurrence of a parameter read
under a node binds that node in the `FROM`, as
`CONTAINS EVALUATION n0[openEHR-EHR-EVALUATION.problem_diagnosis.v1]`, so
the conditions of one occurrence read one node, while a repeated parameter
(an AND) may match a node of its own. Every search value travels as a bound
AQL parameter and never as query text.

The declaration is checked when the server starts, against the template the
context maps, and any of these refuses the start with the file named: an
unknown key, a context no loaded mapping compiles, a parameter type other
than `token`, `string` or `date`, a name under the `_` prefix R4 reserves, a
node the template does not hold, a path the template does not hold, a path
that crosses a node which may occur more than once below the bound one (bind
that node instead), a token over a value that is not coded, a path that is not
one AQL path, a query that does not parse as AQL, two declarations of one
resource type over one template, and two declarations of one type that state
different parameters. `order` defaults to the commit time and then the version
uid; set it when your CDR orders better by something else, and end it with a
key that is unique per composition.

### What a search answers

| Parameter | What it does |
|---|---|
| a declared `token` | `[code]`, `[system]\|[code]`, `\|[code]` and `[system]\|`; `systems` translates a FHIR system URI into the openEHR terminology id, and an unlisted system compares as written |
| a declared `string` | starts-with by default, `:contains` and `:exact` |
| a declared `date` | the range of the value's precision, with the prefixes `eq`, `ne`, `gt`, `lt`, `ge`, `le`, `sa` and `eb` |
| `patient`, `subject` (as declared) | `[id]`, `Patient/[id]`, an absolute URL on this server, or the `:Patient` modifier, resolved through the identity map to the EHR |
| `_id` | a logical id, resolved through the identity map to its composition |
| `_count` | the page size, capped at `[facade] search_count_max`; `0` answers the total alone |
| `_sort` | declared parameters read under the composition, `-` for descending, ahead of the declared order; a parameter read under a node is refused, because it has a value per node |
| `_summary` | `count` answers the total alone; `false` is the default |
| `_total` | `none`, `estimate` or `accurate` |

A comma inside a value is an OR and a repeated parameter an AND. An empty
parameter is ignored. A subject or an id the identity map does not know
matches nothing, so the answer is an empty `searchset`, never a `404`, and no
query reaches the CDR. A subject is read from the map and never echoed from
the request.

A parameter the type does not declare is a `400` with `not-supported` naming
it. Send `Prefer: handling=lenient` and the facade ignores it instead, leaves it
out of the `self` link and reports it in an `OperationOutcome` entry with
`search.mode = outcome`. A modifier the facade does not support is a `400` in
both modes, because R4 makes that a SHALL. So are a malformed value, the `ap`
date prefix, and a string pattern carrying `*` or `?`, which AQL `LIKE` reads
as wildcards with no escape (search it with `:exact`). Chained parameters and
the `:identifier` modifier are not supported yet
([issue #492](https://github.com/FerroHEALTH/FerroBRIDGE/issues/492)).

### The searchset

The answer is a `searchset` Bundle with an `id` and `meta.lastUpdated`. Each
entry is one composition, with a unique `fullUrl` under the configured base and
`search.mode = match`, mapped back through the context's program as a read
maps it. `link[self]` names the parameters the search used with the page size,
and `link[next]` appears when another page follows. Its `_cursor` is opaque:
follow the link as answered, because a cursor pasted onto another search is a
`400`.

Each entry of the context's archetype in a composition is a resource of its
own. A query that binds a node answers a row per matching node, and the
search answers the entries that hold the matched nodes; a search that binds
no node, such as `patient` alone, answers every entry of each matching
composition. The rows of one composition sit together under the order, and
an entry the page already holds is passed over. Each page reads one row past
its room through the CDR's `offset` and `fetch`, and a `next` link is written
only when another entry is left, its cursor at the first row of that entry's
composition with the number of its entries already answered. `Bundle.total`
counts matching entries, never the page and never the nodes: a first page
that holds every match carries it at no cost, and `_total=accurate` on a
paged search costs one AQL count of the entries, which reads no composition.
The count binds the entry as `CONTAINS <class> x[<archetype>]`, and AQL
`CONTAINS` also finds an entry nested in a `SECTION`, so a search that binds
a node or an `_id`, or a template that nests the entry, reads the rows and
counts each entry once instead. `_total=estimate` always answers from one
count query, and `_total=none` leaves the total out. `_count=0` and `_summary=count` answer the total
alone, with no entries and no `next` link.

A search never answers an empty Bundle for a failed call. A CDR refusal reaches
the wire through the status table above: a `5xx` or a `408` on the query is a
`502` carrying the upstream status, and a `400` on it is a `500`, because the
bridge wrote the AQL. A row the facade cannot read is a `502` naming the column,
never the value.

## What the facade does not do

It does not materialise a FHIR store, it does not implement Subscriptions, and
it does not send demographics to an external endpoint. Each of those is
recorded as outside the current design, with its reasoning, on the tracker.
