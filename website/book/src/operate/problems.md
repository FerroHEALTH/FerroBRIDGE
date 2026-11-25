<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# Problem types

FerroBRIDGE answers every error outside its FHIR routes as problem details,
the JSON document of [RFC 9457](https://www.rfc-editor.org/rfc/rfc9457),
with the media type `application/problem+json`. That covers the
[operator API](operator-api.md), the health routes, and the refusals the
server's HTTP stack writes itself on those routes: a request past its
timeout (`408`), a body over the limit (`413`), a server at its request
limit (`429` or `503`) and a handler that failed (`500`). The
[FHIR facade](../integrate/fhir-facade.md) and the
[FHIRconnect operations](../integrate/operations.md) under `/fhir` keep the
FHIR `OperationOutcome` that FHIR R4 prescribes.

A problem looks like this:

```json
{
  "type": "https://ferrobridge.eu/docs/operate/problems.html#provisioned",
  "title": "A provisioned file holds this path",
  "status": 409,
  "detail": "a provisioned file holds this path, and the console never writes one"
}
```

- `type` names the problem type. Each FerroBRIDGE type is a link to its
  section on this page. `about:blank` means the status code says all there
  is to say, and `title` is then the status phrase, such as `Payload Too
  Large`.
- `title` is the type's summary, the same for every occurrence.
- `status` is the HTTP status of the answer.
- `detail` says what to correct, in a fixed sentence.
- `instance`, on a refusal of the stack and a failed handler, is the
  server's request id as a `urn:uuid:` URI, the id the request log names.

A problem never quotes the request: no path, mapping file, request body,
reason, token or upstream answer appears in it, so a problem is safe to show
and to log. The server log names the cause of a `500` or a `503`, and the
`X-Request-Id` header of the answer names the request when you report it.
Every problem answer carries `Cache-Control: no-store`.

Two operator answers are documents, not problems: the `422` of a check, a
save or a reload names every diagnostic of the set, and
`GET /operator/readiness` answers `503` with the readiness document while
the process is not ready.

## The types

| Type | Status | When |
|---|---|---|
| [`invalid-request`](#invalid-request) | `400` | The `Authorization` header carries no single credential. |
| [`reason-required`](#reason-required) | `400` | A save names no reason. |
| [`bad-path`](#bad-path) | `400` | The path names no mapping file the console can write. |
| [`bad-archive`](#bad-archive) | `400` | The body is no mapping-set archive the bridge reads. |
| [`unauthenticated`](#unauthenticated) | `401` | The request carries no valid access token. |
| [`not-permitted`](#not-permitted) | `403` | The token does not carry the operator scope. |
| [`not-found`](#not-found) | `404` | The mapping file, the revision or the configuration does not exist. |
| [`provisioned`](#provisioned) | `409` | A provisioned file holds the path. |
| [`not-configured`](#not-configured) | `409` | No `[operator] mapping_dir` is configured. |
| [`precondition-failed`](#precondition-failed) | `412` | What the request replaces changed since the `ETag` it names. |
| [`precondition-required`](#precondition-required) | `428` | The write sends no precondition. |
| [`no-example`](#no-example) | `422` | The template has no example composition. |
| [`invalid-source`](#invalid-source) | `422` | The source's document is refused. |
| [`source-conflict`](#source-conflict) | `409` | Another source holds the name or the MLLP route. |
| [`unreadable`](#unreadable) | `500` | A mapping directory could not be read. |
| [`unwritable`](#unwritable) | `500` | The mapping directory could not be written. |
| [`dead-letters-unavailable`](#dead-letters-unavailable) | `500` | The dead-letter store could not be read or written. |
| [`reload-unavailable`](#reload-unavailable) | `503` | The mapping set could not be reloaded. |
| [`run-history-unavailable`](#run-history-unavailable) | `503` | The run history could not be read. |
| [`sources-unavailable`](#sources-unavailable) | `503` | The inbound sources could not be read or written. |
| [`unverified`](#unverified) | `401` | An inbound request does not prove it comes from its source. |
| [`unreadable-event`](#unreadable-event) | `400` | An inbound body is no event the source's map reads. |
| [`unknown-source`](#unknown-source) | `404` | No enabled webhook source has the name. |
| [`not-parked`](#not-parked) | `404` | No parked item of the source has the entity. |
| [`inbound-busy`](#inbound-busy) | `503` | The source has as much pending work as its `max_pending` allows. |
| [`inbound-unavailable`](#inbound-unavailable) | `503` | The inbox could not be read or written. |
| [`recovery-leased`](#recovery-leased) | `409` | Another replica runs the source's recovery. |
| [`recovery-failed`](#recovery-failed) | `502` | The source refused or failed a recovery call. |
| [`authorization-unavailable`](#authorization-unavailable) | `503` | The token could not be checked. |

## invalid-request

`400`. The `Authorization` header carries two credentials, or a value that
is no credential. The answer carries `WWW-Authenticate: Bearer
error="invalid_request"`. Send one `Bearer` token.

## reason-required

`400`. A save names its reason in the `Ferrobridge-Reason` header: one line
of at most 200 characters. Add the header and send the save again.

## bad-path

`400`. The path of a save or a check names no FHIRconnect or OMOCL mapping
file the console can write: it is under no mapping root, or the file is not
`.yml` or `.yaml`.

## bad-archive

`400`. The body of an import is no ZIP, is not sent as `application/zip`,
holds more than 4096 entries or more than 64 MiB of content, names a file
outside `fhirconnect/` and `omocl/` or one that is not `.yml` or `.yaml`, or
names one file twice. Export a revision to see the shape an import reads.

## unauthenticated

`401`. The request carries no access token, or one the server does not
accept. The answer carries the `WWW-Authenticate` challenge of the
configured schemes. Request a new token from the authorization server.

## not-permitted

`403`. The access token is valid and does not carry the operator scope,
`auth.product_scope`. The answer carries `WWW-Authenticate: Bearer
error="insufficient_scope"`.

## not-found

`404`. No mapping file is at the path, the path leaves its mapping
directory, or the operator API serves no running configuration.

## provisioned

`409`. A provisioned file holds the path, and the console never writes one.
Change the provisioned tree where it is provisioned, or save the console
file under another path.

## not-configured

`409`. No `[operator] mapping_dir` is configured, so the operator API checks,
saves and reloads nothing. Set `mapping_dir` to turn on editing.

## precondition-failed

`412`. The file changed since the `ETag` the save names, or
`If-None-Match: *` names a file that exists, or the running revision is no
longer the one an apply names in `If-Match`. Read the file or the revisions
again and send the new `ETag`.

## precondition-required

`428`. A save sends `If-Match` with the file's `ETag`, or
`If-None-Match: *` for a new file, and an apply sends `If-Match` with the
running revision's id. Add the header.

## no-example

`422`. The template is no composition template, such as a party's, or no
synthetic example of it builds. The server log names the cause.

## invalid-source

`422`. A source's document is not the kind's object, breaks one of its rules
(a webhook without a key, an MLLP route bound to neither a certificate
subject nor a network, a tenant named twice), or names a secret the
`[secrets]` directory does not hold. The detail names the rule, never a
secret's value. Fix the document, or add the secret to the directory first.

## source-conflict

`409`. Another source carries the name, or already routes one of the MLLP
senders the document names. Pick another name, or move the route off the
other source first.

## unreadable

`500`. A mapping directory could not be read. The server log names the
directory and the cause.

## unwritable

`500`. The mapping directory could not be written. The server log names the
cause; check that the server's user owns `mapping_dir`.

## dead-letters-unavailable

`500`. The state store that keeps the dead letters of the change feed
(`[state]`) refused a read or a write, so the parked events cannot be listed
or a settlement cannot be recorded. The server log names the cause; check
that the database is reachable.

## reload-unavailable

`503`. The mapping set could not be compiled, because the CDR did not serve
a template or another upstream did not answer. The running set is
unchanged, and the server log names the cause. Send the request again once
the upstream answers.


## run-history-unavailable

`503`. The run history in the bridge schema (`[cdm] bridge_schema`) could
not be read, because the CDM database did not answer or holds no run table.
The server log names the cause. A bridge schema an earlier release created
gets its run table from `ferrobridge cdm init`, which leaves every other
table as it is.

## sources-unavailable

`503`. The state store could not read or write the inbound sources, or a
secret a source names cannot be read at runtime, so the source fails closed.
The server log names the cause, and the secret by its name alone.

## unverified

`401`. The inbound request failed its source's verifier: a signature that
does not match any key, a header that is missing or does not read, a
timestamp outside the tolerance, or a client certificate the source does
not list. Nothing of the request is stored. The server log names the reason
and never a value of the request.

## unreadable-event

`400`. The verified body is no JSON, or lacks a part the source's event map
selects (the tenant, the entity, the stamp or a de-duplication field), or
holds one that is no string or number. Nothing is stored.

## unknown-source

`404`. No source has the name in `/inbound/{source}`, or the source is
disabled or of another kind.

## not-parked

`404`. The settlement names an entity that has no parked work item in the
source.

## inbound-busy

`503`. The source has `max_pending` work items waiting or running, so the
intake stores no more until the workers catch up. The answer carries
`Retry-After`. Nothing is stored.

## inbound-unavailable

`503`. The state store could not store the event or read the inbox. Nothing
is stored, so the source sends the event again. The server log names the
cause.

## recovery-leased

`409`. Another replica holds the recovery lease of the source, so this one
runs neither its redelivery nor its reconciliation. Send the request again
later, or wait for the leader's next round.

## recovery-failed

`502`. The source refused or failed the redelivery call or a reconciliation
poll, or a secret the step names cannot be read. The server log names the
step and the status, and never a value of a record.

## authorization-unavailable

`503`. The server could not check the access token, because the
authorization server's keys could not be read. Send the request again once
the authorization server answers.
