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
| [`unauthenticated`](#unauthenticated) | `401` | The request carries no valid access token. |
| [`not-permitted`](#not-permitted) | `403` | The token does not carry the operator scope. |
| [`not-found`](#not-found) | `404` | The mapping file or the configuration does not exist. |
| [`provisioned`](#provisioned) | `409` | A provisioned file holds the path. |
| [`not-configured`](#not-configured) | `409` | No `[operator] mapping_dir` is configured. |
| [`precondition-failed`](#precondition-failed) | `412` | The file changed since the `ETag` the save names. |
| [`precondition-required`](#precondition-required) | `428` | The save sends no precondition. |
| [`unreadable`](#unreadable) | `500` | A mapping directory could not be read. |
| [`unwritable`](#unwritable) | `500` | The mapping directory could not be written. |
| [`reload-unavailable`](#reload-unavailable) | `503` | The mapping set could not be reloaded. |
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
`If-None-Match: *` names a file that exists. Read the file again and save
over its new `ETag`.

## precondition-required

`428`. A save sends `If-Match` with the file's `ETag`, or
`If-None-Match: *` for a new file. Add one of them.

## unreadable

`500`. A mapping directory could not be read. The server log names the
directory and the cause.

## unwritable

`500`. The mapping directory could not be written. The server log names the
cause; check that the server's user owns `mapping_dir`.

## reload-unavailable

`503`. The mapping set could not be compiled, because the CDR did not serve
a template or another upstream did not answer. The running set is
unchanged, and the server log names the cause. Send the request again once
the upstream answers.

## authorization-unavailable

`503`. The server could not check the access token, because the
authorization server's keys could not be read. Send the request again once
the authorization server answers.
