<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# The FHIR facade

The FHIR side of FerroBRIDGE is a REST facade in front of an openEHR CDR. A
FHIR client sees one FHIR server; behind it, every request becomes CDR
operations driven by FHIRconnect mappings. The facade stores no clinical data
of its own.

**Audience:** integrators who connect a FHIR client to openEHR data through
the facade.

The facade is off until `[facade] enabled` turns it on, and a disabled facade
mounts no route, so a request answers `404`. This page covers the surface,
reads and writes, identity, party resources, `$validate`, the status table
and terminology. The other facade tasks have pages of their own:

- [Securing the facade](facade-security.md): tokens, SMART permissions and
  browser apps;
- [Searching the facade](facade-search.md): search declarations and what a
  search answers;
- [Consent and access directives](facade-consent.md);
- [SMART App State and launch Tasks](facade-smart.md);
- [Documents and the patient summary](facade-documents.md): `$document`,
  the MHD Document Responder, `Patient/$summary` and the EEHRxF Annex X
  data groups;
- [The Yellow Button API](yellow-button.md).

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
| `GET /fhir/Composition/{id}/$document` | The document Bundle of one Composition; also `GET /fhir/Composition/$document?id={id}` |
| `GET /fhir/Patient/{id}/$summary` | The patient summary of one patient, an IPS 2.0.1 document, with `[facade.summary]` on ([Patient summary](facade-documents.md#patientsummary-writes-the-patient-summary)) |
| `GET /fhir/{type}/{id}` | Read one resource back out of its composition |
| `GET /fhir/{type}/{id}/_history/{vid}` | Vread: read the version a write's `Location` named |
| `PUT /fhir/{type}/{id}` | Update, with `If-Match` |
| `POST /fhir` | A `transaction` Bundle, all or nothing, or a `batch` Bundle, each entry on its own |
| `GET /fhir/{type}/{id}/_history` | The history of a party resource |
| `DELETE /fhir/{type}/{id}` | Delete a party resource |
| `GET /fhir/.well-known/smart-configuration` | The SMART discovery document, with `[auth]` on |
| `POST`, `PUT`, `DELETE` and `GET /fhir/Basic` | SMART App State, with `[facade.app_state]` on ([SMART App State](facade-smart.md#smart-app-state)) |
| `POST` and `GET /fhir/Task`, `GET /fhir/Task/{id}` | SMART launch `Task`s, with `[facade.launch_task]` on ([SMART launch Tasks](facade-smart.md#smart-launch-tasks)) |

A type no loaded program maps is absent from the `CapabilityStatement` and
answers `404` with a `not-supported` `OperationOutcome` on the wire. A search on a type no search
declaration answers is a `404` with `not-supported`, and a `_count` that is not
a non-negative integer is a `400` with `invalid`, so a client never reads a
page it did not ask for.

The `CapabilityStatement` names exactly the resource types the loaded programs
map, with `create`, `read`, `vread` and `update` per type, `transaction` and
`batch` at system level, the `$validate` operation, `$document` on
`Composition`, `$summary` on `Patient` with `[facade.summary]` on,
`updateCreate: false`,
`conditionalCreate` true, `conditionalUpdate` false (no `PUT [type]?[search]`
is served; `If-Match` on an update is the version-aware update),
`versioning: versioned-update` and `readHistory: true` per type, and
`fhirVersion: 4.0.1`. A type a search
declaration answers also names `search-type` and, in `searchParam`, exactly
`_id` and the parameters the declaration states; every other type names no
search parameter. A type no program maps and a `reference` mapping of a
loaded program creates is listed with `read` and `vread` alone (see
[Resources the mapping creates](#resources-the-mapping-creates)).
A type a party context serves is listed with `read`, `vread`,
`history-instance`, `create`, `update` and `delete` and no search (see
[Party resources](#party-resources)).
When the FHIRconnect operations lane is served under the same base,
`rest.operation` also names `$tofhir` and `$toopenehr`; their direct forms are
never declared ([The FHIRconnect operations](operations.md) says why).
With `[auth]` on, `rest.security` names the `SMART-on-FHIR` service and
carries the `oauth-uris` extension with the authorization server's
endpoints, for clients that discover SMART through the statement (see
[Security](facade-security.md#security)).

## Media types and bodies

The facade reads `application/fhir+json` and `application/json`, which R4 names
as the JSON media type and its alias, and answers `415` for anything else. Every
response carries `application/fhir+json`, except a `$document` and the MHD
resources, which answer `application/fhir+xml` when asked. An `Accept` header
that admits neither is a `406`.

A body is parsed through the strict codec of the generated `fhir-types` crate,
so an unknown property is a `400` with `structure` rather than a value that is
silently dropped. `Prefer: return=minimal`, `return=representation` and
`return=OperationOutcome` are honoured on a write, and the default is the
resource itself. With `return=OperationOutcome` the outcome names the committed
version and then each loss the mapping declared: a field the engine filled is
an `information` issue, an element it could not carry an `incomplete`
`warning`, each with the FHIRPath of the element in `expression` and the
openEHR path in `details`.

Everything the facade authors is an `OperationOutcome` with an `issue.code`
from the R4 value set. An openEHR error body never reaches the wire as its own
document: the CDR's `message` and every `validationErrors` entry travel
verbatim inside `issue.diagnostics`, which keeps one error vocabulary on the
wire while losing nothing the CDR said. A CDR that answers RFC 9457 problem
details (`application/problem+json`) is read the same way: its `title`, its
`detail`, each `errors` violation as `path: detail`, and its problem `type`.
The bridge's composition and contribution writes send
`Prefer: return=representation`, so a CDR that lists violations only for such
a write lists them.

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
fixes that "once assigned, this value never changes". The facade gives each
resource the deterministic id the FHIRconnect engine chapter recommends, a hash
of the composition uid and the entry path, the same id `$tofhir` answers for
the same occurrence. The composition uid is the version container, the part of
the `OBJECT_VERSION_ID` that stays the same across versions. The entry path
names one occurrence of the context's start archetype wherever the template
places it: a `content` item, an item of a `SECTION` at any depth, or an instance
repeating below another entry, with a later occurrence named by its place in
its container, as in `/content[openEHR-EHR-SECTION.adhoc.v1]/items[2]`. Each
occurrence is a resource of its own. The hash is SHA-256, written as 52
lowercase base32 characters. `meta.versionId` carries the openEHR
`version_tree_id` and never enters the derivation, so a new version of a
composition reads back under the same id with a new `meta.versionId`.
`meta.source` names the openEHR version uid the answer was read from.

The identity map lives in the state store ([`[state]`](../operate/configuration.md#state)),
which every replica shares, in eight tables: a patient identifier to its
`ehr_id` and each `ehr_id` back to the first person recorded for it, an
external resource id to the internal one, an internal id to the
composition version container with the entry path and split occurrence, and the
source resource `id` with its `meta.versionId` to the mapping that consumed
them, beside the contribution each source was committed in, each
`Resource.identifier` of a committed resource, and one row that pins the
identity derivation and the lookup key. The map wins once written: every row
is recorded once, so two replicas agree on the one that stands. A map another
derivation wrote is refused at start. The store keeps identifiers and nothing
else, and no identifier value in clear: a patient identifier and every
`Resource.identifier` value are stored as an HMAC under the `[state.seal]`
lookup key, and the person of an EHR is sealed, which a test proves by
scanning the tables for the value.

That last table is what makes create idempotent. A resource re-sent with the
same `id` and `meta.versionId` commits nothing and answers `200 OK` with the
composition it already produced. A resource with a known `id` and a new
`meta.versionId` commits a later version of that composition. A resource with a
known `id` and no `meta.versionId` is a replay when it maps to the content the
composition holds now, and a later version otherwise. A transaction entry with
a known `id` and a new `meta.versionId` commits that later version inside the
Bundle's contribution. The Operate page on failure and identity states the
full rule.

### Resources the mapping creates

Going out of openEHR, a `hierarchy.split` with `create: resource` creates one
resource per further occurrence of its node, and a `reference` mapping
creates the resource it names. R4 says a contained resource "does not have an
independent existence apart from the resource that contains it", so the
facade contains none of them. Each one gets a binding of its own the first
time the facade answers the resource whose mapping creates it (its parent), on
a read, a vread, a search or a write under `Prefer: return=representation`.
Its id is a SHA-256 over the parent's entry key, the creating mapping, the
openEHR occurrence positions and the split's `unique` values, rendered as 52
lowercase base32 characters, so a re-run and a later composition version
derive the same id, and the map wins once written. The parent references a
created resource as `<Type>/<id>`. A split's further occurrence is a sibling
of the parent, and the parent names no reference to it.

`GET /fhir/{type}/{id}` and the vread answer a created resource: the facade
maps the parent's entry again and returns the created resource of that id,
with the parent's `meta.versionId` and `meta.source` and no `meta.profile`.
A version that no longer creates it answers `404`. A `PUT` of a created
resource answers `405 Method Not Allowed` with `not-supported`, because its
content is the parent's entry: update the parent. A search of the parent's
type lists every split occurrence beside the entry's own resource, each
once: a page holds at most `_count` resources and may end between two
occurrences of one entry, `Bundle.total` counts every resource, and an
`_id` that names a created resource answers that resource alone. A type
only a `reference` creates declares no search. No specification governs the
identity of a created resource or how a search reaches it: it is
FerroBRIDGE's own design.

## A read is a valid instance

Every resource the facade answers, on a read, a vread or a write under
`Prefer: return=representation`, is a valid R4 instance: each element the R4
element table marks `min 1` is present, at the top of the resource and inside
every element and contained resource it carries (a resource the mapping
creates is answered on its own, never contained). A mapping often has no
outbound row for the subject, because the ingest reads the subject to find the
EHR and the composition does not hold it. When the rendered resource has no
`subject` (or `patient`, on a type that names its subject so), the facade
writes it from the identity map: the person the map recorded for the
composition's EHR, as the literal `Patient/<id>` reference the deployment
reads it from (the party uid the state store recorded under a `Patient` party
context, the pseudonym otherwise), and otherwise as a logical reference with
`type: Patient` and the person's `identifier`. That is the form a create reads back into the
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
`system_id` naming the bridge. When a machine (a client credentials token)
made the request, its client is the originating system
(`originating_system_audit.system_id`) and the bridge the feeder system
(`feeder_system_audit.system_id`), which RM `FEEDER_AUDIT` records for a
feeder "different from the originating system". A source resource that carries no `id` is
recorded as unknown, never given one. So a reader of the CDR can tell which
FHIR resource a composition came from without asking the bridge.

The CDR writes who committed each version, from the credential it
authenticated. A single write states the change type and `system_id`, which
names the bridge, in `openehr-audit-details`, and no committer: ITS-REST
1.1.0 leaves every member of the header optional, and a CDR that writes the
committer itself refuses one that says more than its own. A transaction's
CONTRIBUTION must carry a committer in its `audit` and in every version's
`commit_audit`, so there the facade names the party the CDR authenticated,
and nothing more:

- under the bridge's own client credentials grant, a `PARTY_IDENTIFIED`
  named by its client id;
- as a Basic user with [`[cdr] committer_issuer`](../operate/configuration.md#cdr)
  set, the placeholder `{"_type": "PARTY_IDENTIFIED", "identifiers":
  [{"_type": "DV_IDENTIFIER", "issuer": "<committer_issuer>", "id":
  "<user>"}]}`;
- under [`[cdr.endpoint.credentials.onward]`](../operate/configuration.md#cdrendpointcredentialsonward),
  where the request reaches the CDR under the token the caller's was
  exchanged for, the same placeholder with the exchanged token's `iss` and
  `sub`;
- otherwise, a `PARTY_IDENTIFIED` named by `system_id`.

The caller the facade authenticated is recorded in the bridge's own
[access record](../operate/access-record.md), and a machine caller as the
originating system above. The facade sends no attestation.

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
the write committed. The facade finds that version in the CDR's revision
history of the composition, reads it, and answers it with its `ETag`. A
`{vid}` the history does not list answers `404`, and a version the CDR reports
deleted answers `410`. An earlier version stays readable after the latest one
was deleted.

A read and a vread also answer `Last-Modified`: the commit time the revision
history records for the version, as an HTTP-date. That costs one more CDR
call per read. A commit time with no UTC offset names no instant, so that
answer carries no `Last-Modified`.

An update needs `If-Match` when the map knows the id. Without one, the facade
reads the CDR's current `ETag` and answers `412` with it on a mismatch rather
than overwriting a version the client never saw. Send back the `ETag` a read
or write answered, `If-Match: W/"1"`: the facade completes that `versionId` to
the current composition version and answers `412` with the current `ETag`
when it is stale. A value that names no version is `400`. The body must
name the id the URL names: a body with no `id`, or another one, is `400`. A `PUT` to an id the map does
not know is a `404`: the facade does not upsert, and the
`CapabilityStatement` says `updateCreate: false`.

A `transaction` Bundle maps every entry first and commits the lot through one
ITS-REST contribution so the CDR makes it atomic. The facade uses `fullUrl`
in two places. It names each entry by its `fullUrl` in every refusal, or by
its position when it has none. And a `subject` or
`patient` reference whose value is another entry's `fullUrl` is read through
that entry: the person is the pseudonym the identity source holds for that
entry's identifier in the first accepted domain, asked once per identifier
for the whole Bundle. The
facade resolves no other reference. A reference is mapped as the resource
carries it, and the facade never fetches a referenced resource from anywhere.
Any failure answers one `OperationOutcome` naming every failing entry, and
nothing is committed.

A committed transaction answers `200` with a `transaction-response` Bundle,
one entry per request entry in the same order. Each entry's `response` carries
`201 Created`, the `location` `[base]/[type]/[id]/_history/[vid]` and the
`etag` `W/"[vid]"`, the values a single create answers, and an
`outcome` naming each loss the entry's mapping declared, as a single write
answers it under `return=OperationOutcome`. Send the same Bundle
again and nothing is committed: each entry answers `200 OK` with the location
of the resource the first delivery created. A Bundle only some of whose
entries an earlier delivery consumed is refused with `409`, naming those
entries, and a Bundle that carries one resource twice is refused with `422`.
A delivery that arrives while another delivery of the same Bundle or resource
is still in flight is refused with `409` and commits nothing; retry it after
the first one answers. The redelivery rule is on the
[failure and identity](../operate/failure-and-identity.md) page.

An entry whose mapping links a resource as a composition of its own (a
FHIRconnect linked mapping) is refused with `422` and `not-supported`: the
linked composition's id exists only once the CDR commits it, so it cannot
share the transaction's one contribution. Send that resource on its own, or as
an entry of a `batch`, whose entries commit one at a time.

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
and commits nothing. Each composition's `FEEDER_AUDIT` names the sending
application, the order numbers and the message (see the HL7 v2 face page)
where a resource would be named here. A subject reference to another entry of the
Bundle is read through that entry: the person is the referenced resource's
first `identifier`, the same pair a `subject.identifier` names.

### References the request does not carry

A `reference` mapping reads the resource a reference points at. FHIRconnect
says that when the resource is not in the request, "the engine should attempt
to GET it from the sending site. If not possible, the engine proceeds with the
mapping", and that it should stop at a level or track what it resolved. The
facade's policy beyond that is FerroBRIDGE's own design:

- In a transaction Bundle, an entry answers a reference to it first: by its
  `fullUrl`, or by `<Type>/<id>` when it carries an id. Such a reference is
  never read from this server or fetched.
- A relative reference (`Observation/abc`), or an absolute one under
  `[facade] base_url`, names a resource this server committed. The facade
  reads it through the identity map and the CDR, as a read of it answers.
- An absolute reference to another server is fetched with
  `Accept: application/fhir+json` only when its base is listed in
  `[facade.references] fetch_bases`, and a redirect is not followed. The list
  is empty by default, so by default the facade fetches nothing, and a client
  cannot make the bridge call a URL it names.
- References are followed to `max_depth` levels (default 2: the request's own,
  then those of each resolved resource), and each literal is resolved once,
  so a cycle ends.
- A reference the facade does not resolve, because its base is not listed,
  the server answered a status other than `200`, the fetch timed out, the body
  is not the referenced resource, or this server holds no such resource, is a
  declared skip: the write proceeds without it. The log line names the
  reference, the base and the upstream status, never a body, and `$validate`
  answers it as a `warning` issue naming the reference and the reason.

Only a reference whose type some loaded `reference` mapping names is
followed, because no other is read. The keys are on the Operate page under
`[facade.references]`.

### Demographic references

A reference a mapping writes as a party (a recorder as the composer or as
`ENTRY.provider`, a participant) is never dropped. FHIRconnect sends it to a
demographics service, and with none "a FHIRconnect engine should use the IDs
maintained by the FHIRconnect engine's FHIR-server" (`engine/demographics.adoc`).
The facade is that server, so a relative reference, or one under
`[facade] base_url`, becomes an identity issued by that base:

- in `PARTY_IDENTIFIED.identifiers` as a `DV_IDENTIFIER` whose `issuer` is
  the base, whose `id` is `<Type>/<id>` and whose `type` is `fhir-reference`,
  where the FLAT format carries identifiers (`ENTRY.provider`);
- in `PARTY_PROXY.external_ref` as a `PARTY_REF` in the base's namespace
  with a `GENERIC_ID` of scheme `fhir-reference`, on the composer and on a
  participation's performer, whose FLAT keys carry only the reference.

A read turns either back into the same `Reference.reference`. A reference to
another server keeps that server as its issuer, and a `urn:` reference to an
entry of the request is refused. The identity shapes are FerroBRIDGE's own
design. A CDR that refuses identified parties in clinical content answers
`422`, and the facade passes its words through in `issue.diagnostics`.

## Party resources

A party context (a FHIRconnect context whose template is rooted at a `PERSON`,
`ORGANISATION` or `ROLE`) serves its resource type from the CDR's demographic
store over the ITS-REST 1.1.0 Demographic API, DEVELOPMENT, which the release
marks `x-status: DEVELOPMENT`. It loads only with `[cdr] demographic` on: a
party context with the gate off is a boot refusal, and one resource type is
served by one party context. The correspondences live in the mapping files,
for example `Patient` and `Practitioner` onto a `PERSON`, `PractitionerRole`
onto a `ROLE`, and `Organization` onto an `ORGANISATION`.

- **Identity.** The id is the party's `versioned_object_uid` and
  `meta.versionId` its version tree id; the identity map holds only a
  `Patient` party's pseudonym. One `PERSON` reads as a `Patient` and as a `Practitioner` under the same
  id, because FHIR scopes an id to its type.
- **Interactions.** `read`, `vread`, `history-instance`, `create`, `update`
  (over the version `If-Match` names, or the latest) and `delete` (the latest
  version). The Demographic API has no search or list operation, so a party
  type offers no search, and the `CapabilityStatement` documents that on the
  type. With [`[facade.party_search]`](../operate/configuration.md#facadeparty_search)
  a configured type is searched by `identifier` or `name` through the Cadasto
  experimental AQL demographic extension, or by `_id`, and the statement
  declares `search-type` and exactly those parameters on that type. A CDR that does not
  serve the extension answers the search with `501 not-supported`.
- **The patient's EHR.** A `Patient` create or update first asks
  [`[identity_source]`](../operate/configuration.md#identity_source) for the
  pseudonym of its identifier in the first accepted domain, and refuses
  before anything is committed when it gets none. After the commit the state
  store records the party's id against the pseudonym, so the clinical
  resources that name `Patient/<id>` find the EHR keyed by it. With
  `ehr_policy = "create_on_first_write"` the facade also looks that EHR up
  and creates it when the CDR holds none. Under `existing` the write sends no
  EHR call, and a `Practitioner` never gets one. When the EHR call fails after the party committed, the answer is the
  EHR refusal with a warning that names the stored `Patient`.
- **Template.** A party built against another template than the context's
  answers `404`. The template may be an ADL 1.4 OPT or an ADL 2 operational
  template, the one a CDR serves only on its `adl2` route included. ADL 2.4
  admits two node identifier coding systems (§4.2.5): an at-coded template
  names its nodes `[at0001]`, an id-coded one `[id3]`, and the paths of the
  model mapping name the codes the template and its data carry, because a
  path predicate is a shortcut for `[@archetype_node_id = …]` (BASE 1.2.0,
  Architecture Overview, Paths and Locators). FerroEHR's professional
  `PERSON`, `openEHR-DEMOGRAPHIC-PERSON.healthcare_professional.v1.0.0`, is
  id-coded: its identifier is `details[id2]/items[id3]` and its full name
  `identities[id4]/details[id5]/items[id6]`. The header names the
  archetype's interface id and pins its release as the revision
  (`archetype: openEHR-DEMOGRAPHIC-PERSON.healthcare_professional.v1`,
  `revision: 1.0.0`), as the FHIRconnect header does for any archetype; the
  context names the full template id. An at-coded path names no node of an
  id-coded template, and the load refuses it as a path the template does not
  have.
- **Status.** A Demographic API `400` is a `500`; `404`, `409`, `412` and
  `422` reach the caller as themselves, with the CDR's words in
  `issue.diagnostics`.
- **Authorization.** A `patient/` token reaches the one `Patient` its launch
  context names and no other party resource.
- **Bound professionals.** A `PERSON` that carries a binding key, an
  identifier of an issuer
  [`binding_identifier_issuers`](../operate/configuration.md#facade) names,
  is bound to a login at the CDR and provisioned through the CDR's SCIM
  service. The facade refuses a create, update or delete of such a party
  (a `Practitioner`, typically) with `403 forbidden` before it writes to the
  CDR, and the CDR refuses the same write from any client but an
  administrator or that SCIM client.

A `PractitionerRole` maps `practitioner` onto the `ROLE`'s mandatory
`performer`. The reference must name a `Practitioner` this facade serves,
relative (`Practitioner/<id>`) or under the facade's base, and carry nothing
beside `reference`; the facade answers any other reference, and a role with
no practitioner, with an error rather than dropping it. These rules are FerroBRIDGE's own design: no
specification defines a FHIRconnect mapping rooted at a party.

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
`body_limit` answers `413` with the `too-long` issue, and a request that
outlasts `request_timeout` answers `504` with the `timeout` issue. The
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

A mapping that names a `conceptmap` is translated through `$translate` on
every read, write, search and `$validate`, and on the HL7 v2 face. The map is
read as authored from openEHR to FHIR: a composition going out as FHIR is
translated forward with the map's `url`, and a resource coming in is
translated with `reverse=true`. Every code is resolved before the composition
or resource is built, so a terminology refusal costs no CDR write. Only an
`equivalent` or `equal` match counts. An untranslated optional element is
reported as a loss, and an untranslated required one refuses the resource.
With no `[terminology]` section, a `conceptmap` mapping refuses with "no
terminology server is configured to translate through".

A coding with no `display` that goes into a `DV_CODED_TEXT`, or into a
`DV_TEXT` whose value would otherwise fall back to the coding's code (the
concept carries no `text` either), is looked up through `CodeSystem/$lookup`
on every run into openEHR (a write, `$validate`, `$toopenehr` and the HL7 v2
face): the display the server names becomes the value. A code the server does
not know refuses the resource unless
`[terminology] display_fallback` is `true`, which writes the code instead; a
failed call refuses either way. With no `[terminology]` section the code
stands as the value, and the run reports the fallback as a loss. An
`DV_CODED_TEXT` code of an archetype-local terminology (`local` or `openehr`)
is never looked up.

## What the facade does not do

It does not materialise a FHIR store, it does not implement Subscriptions, and
it does not send demographics to an external endpoint. Each of those is
recorded as outside the current design, with its reasoning, on the tracker.
