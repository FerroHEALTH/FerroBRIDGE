<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# The operator API

The operator API lets an operator console read what the bridge runs and the
mapping files it runs from, without a shell on the host. It is off by
default. No specification governs it: it is FerroBRIDGE's own design.

## Turning it on

Set `[operator] enabled = true` (or `FERROBRIDGE__OPERATOR__ENABLED=true`).
The API needs inbound authentication, so the server refuses to start when
`[operator] enabled` is true and `[auth] enabled` is false or `[auth]` holds
no `[auth.issuer.<name>]` table. The routes are
mounted under `/operator` on the server's one listener; the bridge has no
second admin listener.

```toml
[auth.issuer.clinic]
issuer = "https://auth.example.org/realms/clinic"

[operator]
enabled = true

[state]
url_file = "/run/secrets/ferrobridge_state_url"
```

## The operator scope

Every route needs a bearer access token from the configured issuer, checked
as the FHIR facade checks one, or the Basic credential of an
`[[auth.basic.users]]` user, a person. The token's `scope` claim, or the scopes
`[basic_grants]` grants the user, must carry the operator scope, `[auth] product_scope`, which is
`__ferrobridge.operator` unless you set another. SMART scopes cover FHIR
resources only, so the operator scope is FerroBRIDGE's own, in a form SMART
App Launch 2.2.0 (Scopes and Launch Context, Extensibility) gives an
extension scope: a string starting with `__` or a full URI. Any other value
refuses the start.

| Request | Answer |
|---|---|
| no credential, or one that does not hold | `401` with `WWW-Authenticate: Bearer` (`error="invalid_token"` for a bad token) |
| two credentials, or an `Authorization` value that is no credential | `400` with `WWW-Authenticate: Bearer error="invalid_request"` |
| a valid token without the operator scope | `403` with `WWW-Authenticate: Bearer error="insufficient_scope"` |
| the issuer's keys cannot be read | `503` |

Each refusal is an RFC 9457 problem details document
([Problem types](problems.md)), served as `application/problem+json`.

The operator scope also admits the operations that commit nothing to the
CDR: `POST /fhir/{type}/$validate` beside its `c` or `u` permission, and
`$tofhir` and `$toopenehr`, which need no clinical scope. A token carrying
only the operator scope can therefore dry-run a mapping and transform with
it, and can read and write no clinical record.

## `GET /operator/state`

What the running load compiled, as one JSON document:

- `contexts`: one entry per compiled FHIRconnect context with `context`,
  `file`, `template`, `profile`, `profile_version`, `resource_type`,
  `models` (each with `name`, `file` and the `extensions` applied, in the
  order they applied) and `search`, the search declaration the facade
  answers the context's searches with;
- `templates`: each template the contexts compiled against, with `source`
  (`directory` and its `file`, or `cdr`);
- `omocl`: the OMOCL files and whether they `valid`ate; `serve` loads and
  validates them and does not run them;
- `counts` and `diagnostics`: what `ferrobridge mapping check` reports for
  the same configuration, read once at start-up.

## `GET /operator/readiness`

The readiness document `GET /health/readiness` answers, with why each
indicator that is down is down: `state` (`up`, `degraded` or `down`), `phase`
(`booting`, `serving` or `draining`) and `indicators`, each with its `name`,
its `state` and, when it is not up, its `detail`, which names the upstream
status or the reason the call never reached it and never a body, a
credential, or clinical content. It answers `200` while the bridge is ready
and `503` otherwise. The operator console reads it, and falls back to the
open route, which names no detail, while the operator API is off.

## `GET /operator/events`

The change-feed consumers and the AMQP topology of the
[events lane](../integrate/events.md), as one JSON document. A bridge whose
lane is off answers `200` with no consumer and a `null` topology.

- `consumers`: each consumer with its `name` (the readiness indicator it
  reports as), its `source` (`aql` or `ferroehr`), its `reader`, its
  `positions` (one per stream: `clinical` or `party` with the last
  acknowledged `seq` in `at`, or `committed` with the watermark instant in
  `at` and `versions_at_instant`), its `state` (`starting`, `following` or
  `stalled`), the messages it `published` since the bridge started, the
  instant it `last_published`, and its `last_error`, the `at` and `cause` of
  the last failed pass, kept after it recovers;
- `topology`: the `exchange`, the `routing_key` grammar
  (`<kind>.<change_type>.<template_id>`) and the bound `queues`, each a
  `name` and a `binding`.

The document carries identifiers, instants and counts, never an event body,
and never the broker URL.

## `GET /operator/etl/runs`

The newest 50 [OMOP ETL](../integrate/omop-etl.md#the-run-history) runs the
bridge schema keeps, newest first, as `{"runs": [...]}`. Each run names its
`id`, its `kind` (`compositions` for `etl run`, `fhir` for `etl fhir`), when
it `started` and `finished` (RFC 3339 UTC), its `compositions` (`read`,
`committed`, `skipped` and `refused`), the `rows` it wrote over every table,
its `refusals`, and the compositions it `removed` because the CDR reports
them deleted. A bridge with no `[cdm]` answers `404`, and one whose CDM
database does not answer, or holds no run table yet, answers `503`
([`run-history-unavailable`](problems.md#run-history-unavailable)).

### `GET /operator/etl/runs/{run_id}`

One run whole: the facts of the listing, the `rows` it wrote by CDM table,
its `concept_zero` assignments, every refused record in `refusals` (its
`composition`, `mapping`, `table`, `column`, openEHR `element` and
`reason`), and its `deletions` (`checked`, `extant`, the `removed`, `kept`
and `unknown` compositions by versioned object id, and the `rows` removed by
table). A run the history does not hold answers `404`. A run names
compositions by their versioned object id and never carries a clinical
value.

## `GET /operator/hl7v2/concept-maps`

The [HL7 v2 face](hl7v2-face.md)'s ConceptMap directory, as one JSON
document. A bridge whose face is off answers `404`.

- `counted_since`: the instant the face started, RFC 3339 UTC, since when
  the `messages` counts run;
- `maps`: every map the face runs, by id, each with its `id`, canonical
  `url` and `title`, its `kind` (`message`, `segment`, `datatype`, `table`
  or `other`, from the first word of the id), its `origin` (`guide`,
  `supplement` for a FerroBRIDGE correction that adds a map, or `override`
  for one that replaces a map of the guide), its `ends`, what it maps from and to (each
  group's source and target code system for a table map, else the
  `source` and `target` the id names before and after `-to-`), the `rows`
  the face reads from it, and the `messages` whose mapping applied it.

A message counts once for each map its mapping applied, whether or not its
ingest then committed, and the counts start again at zero when the bridge
restarts. The document carries no message content.

## `GET /operator/templates/{id}/example`

A synthetic example composition of the template `id`, one the running set
compiled against, in canonical JSON. The bridge generates it from the
template's Web Template: each leaf holds a value its constraint admits,
such as the first entry of a coded list or the lower bound of a range, and
the composition is validated against the template before it is answered.
When not every leaf can be filled together, the example holds the mandatory
leaves alone. Every value is synthetic. A template the set does not hold
answers `404`, and one with no example, such as a party template,
`no-example` (`422`).

## `GET /operator/configuration`

The configuration the bridge started on, as one JSON document. The bridge
reads `ferrobridge.toml` and the environment once, at start, so the answer
holds what it runs on until the next start:

- `file`: the configuration file it read, as `--config` or
  `FERROBRIDGE_CONFIG` named it, or `null` when it read none;
- `read_at`: when it read the configuration, RFC 3339 UTC;
- `entries`: every key the configuration holds a value for, in key order,
  each with its dotted `key` (`cdr.endpoint.credentials.oauth.client_id`), its
  `value` and its `source`: `file`, `environment` (a `FERROBRIDGE__`
  variable, `FERROBRIDGE_LOG_FORMAT` or `RUST_LOG`, which win over the
  file), `set` (a `--set` argument, which wins over both) or `default`.

A `value` carries its `kind` (`string`, `integer`, `float`, `boolean`,
`list`, `table` or `secret`) beside it. A secret is never answered by value.
A secret and its `_file` sibling are one entry, keyed by the secret
(`cdm.url`, `cdr.endpoint.credentials.password`,
`cdr.endpoint.credentials.oauth.client_secret`, the same two of `[terminology]`, and
`password_hash` of each `[[auth.basic.users]]` user), whose value is its
`state`: `set` (set as a value, in the file or the environment), `file` with
the `path` it is read from, or `unset`. The file of a private key
(`private_key_file`, `tls.client_identity_file`) is answered as a secret read from
that file. A key the configuration leaves unset and that has no default is
absent from `entries`.

## `GET /operator/mappings`

Every mapping file under the configured directories, as a JSON array. A file
is named by a path that opens with its root, `fhirconnect/` for the
`[mappings] directory` tree and `omocl/` for `[mappings] omocl`, so the
answer never names a host directory.

| Field | Meaning |
|---|---|
| `path` | The file's path under its root, such as `fhirconnect/condition.context.yml` |
| `language` | `fhirconnect` or `omocl` |
| `kind` | `model`, `extension`, `context` or `search` for FHIRconnect, `mapping` for OMOCL; `null` for a file the running load did not read |
| `sha256` | The lowercase hexadecimal SHA-256 of the file's bytes |
| `owner` | `provisioned` for a file under a configured mapping directory, `console` for one of the running revision in the state store |
| `read_only` | `true` for a provisioned file, `false` for a console file |
| `diagnostics` | The diagnostics the running load raised about the file |

## `GET /operator/mappings/{path}`

The file's bytes, verbatim, as `application/yaml`, with the SHA-256 as a
strong `ETag`. A path is plain names under a root: an empty, `.`, `..` or
`..`-prefixed segment answers `404`, and so does a file that resolves outside
its root once every symlink is followed, a path under no root, and a file
that is not `.yml` or `.yaml`.

## Saving mapping files

With [`[state]`](configuration.md) configured, the console checks drafts,
saves them and reloads the running set, and every replica runs what one
saves. Without it, the three routes below answer `409`.

- **One owner per file.** The files under `[mappings] directory` and
  `[mappings] omocl` are provisioned and never written. A console file lives
  in the state store, in a revision, and a save whose path names a
  provisioned file answers `409`. Console files join the set beside the
  provisioned ones, at every save and at start-up, where a running revision
  that does not compile refuses the start. A file with the path of a
  provisioned one is ignored in favour of the provisioned file.
- **Every replica runs the running revision.** A save notifies every
  replica in the transaction that records it, and each replica compiles the
  new revision and swaps it in. A replica that cannot compile it, or cannot
  reach the store, keeps its running set and tries again every five seconds.
- **OMOCL files reach `etl run`.** `serve` checks OMOCL files and does not
  run them. `etl run` reads the running revision's OMOCL files beside
  `[mappings] omocl` when its configuration turns the operator API on and
  names `[state]`, with the same ownership rule, so a file saved here maps
  the next run. Two files that define the same mapping refuse the save and
  the run.
- **Search declarations save beside their context.** A `*.search.yml` file
  under `fhirconnect/` is checked against the context it names, with every
  other declaration of the set, and swapped in with the facade's programs, so
  the `CapabilityStatement` and search answer with it after the save. A
  declaration that names no compiled context, or that the facade's search
  checks refuse, answers `422` with an `fb-search-declaration` diagnostic and
  changes nothing.
- A file that is not `.yml` or `.yaml` answers `400`.

### `POST /operator/mappings/check?path={path}`

The body is a draft of the file at `path`, under `fhirconnect/` or
`omocl/`. The bridge compiles it with every other file of the running set,
for every lane, and answers `200` with `clean`, `counts` and `diagnostics`,
the draft's diagnostics named by `path`. Nothing is recorded.

### `PUT /operator/mappings/{path}`

The body is the file's new bytes, saved verbatim. A save carries:

| Header | Meaning |
|---|---|
| `If-Match` | The `ETag` the file has now, from the file route or a previous save; `*` matches any existing file |
| `If-None-Match: *` | Instead of `If-Match`, for a file that does not exist yet |
| `Ferrobridge-Reason` | Why the file changes: one line of at most 200 characters |

| Answer | When |
|---|---|
| `200` | The set compiled with the draft; a new running revision holds it and the running set is replaced. The body is the new set's report and the `ETag` the new SHA-256 |
| `400` | No reason, or a path the console cannot write |
| `409` | A provisioned file holds the path, or no `[state]` is configured |
| `412` | The file changed since the `ETag` given, or `If-None-Match: *` names an existing file |
| `422` | The set does not compile with the draft; the body names every diagnostic. No revision is recorded and the running set stays |
| `428` | Neither `If-Match` nor `If-None-Match: *` |
| `503` | The CDR did not serve a template, so the set could not be compiled |

A save holds the running revision for its whole transaction, so saves from
every replica run one at a time. The draft is compiled in memory with the
running revision's other files and the provisioned ones; only a set that
compiles is recorded, as one transaction. The set is then swapped in one
step: the facade's programs and search declarations, the FHIRconnect
operations and the operator state, so the `CapabilityStatement` names the new
set at once. A request in flight finishes on the set it started with.

### `POST /operator/reload`

Compiles the provisioned files and the running revision again and swaps
them in, with the answers of a save: `200` with the report, `422` with the
diagnostics and the running set unchanged, `503` when an upstream did not
answer. A reload records no revision; use it after you change a provisioned
file.

## Mapping-set revisions

With `[state]` configured, the bridge keeps every set of console files as a
revision in the state store. A revision holds the whole set of console
files, each by its wire path and SHA-256 digest, with who recorded it, when
and why. Each save and each apply records one that runs, and an import
records one that waits for an apply. The provisioned files belong to the
deployment: they are in no revision, and an apply leaves them as they are.
Without `[state]` the routes below answer `409`.

### `GET /operator/revisions`

Answers `running`, the id of the revision the bridge runs, and `revisions`,
newest first. Each names its `id` (a number that grows with each revision),
its `parent` (the revision that ran when it was recorded), the `author` (the
token's subject), the `time`, the `reason`, the `origin` (`baseline`,
`save`, `apply` or `import`), whether it was `applied`, and how many `files`
it holds. The baseline is revision 1: the provisioned tree at the bridge's
first start, with no console file, recorded once by whichever replica
starts first on an empty store.

### `GET /operator/revisions/{id}`

One revision whole, with `files`, every wire path and its digest, and
`source`, the revision an apply restored. An id no revision has answers
`404`.

### `GET /operator/revisions/{id}/diff`

How the revision differs from the running one: `from` (the running id),
`to`, and `files`, each changed file with its `change` (`added`, `removed`
or `changed`), its two digests and `unified`, a unified diff of its lines
with three lines of context. A file both revisions hold unchanged is left
out.

### `POST /operator/revisions/{id}/apply`

Runs the revision's files on every replica, recorded as a new revision. A
rollback is the apply of an earlier revision; an apply of the baseline runs
the provisioned tree alone. The request carries:

| Header | Meaning |
|---|---|
| `If-Match` | The id of the running revision, in quotes, such as `"3"` |
| `Ferrobridge-Reason` | Why: one line of at most 200 characters |

| Answer | When |
|---|---|
| `200` | The set compiled with the revision's files, which now run. The body is the new set's report and the `ETag` the id of the revision the apply recorded |
| `400` | No reason |
| `404` | No revision has the id |
| `409` | No `[state]` is configured |
| `412` | The running revision is not the one `If-Match` names |
| `422` | The set does not compile with the revision's files; the body names every diagnostic, and nothing changes |
| `428` | No `If-Match` |
| `503` | The CDR did not serve a template, so the set could not be compiled |

The revision's files are compiled in memory with the provisioned files
first, while the apply holds the running revision. Only a set that compiles
is recorded and runs.

### `GET /operator/revisions/{id}/export`

The revision's console files as one ZIP (`application/zip`), each entry
named by its wire path, such as `fhirconnect/condition.context.yml`. The
entries are in path order, deflated at one level, with the ZIP epoch as
their time and fixed permissions, so the same files always give the same
bytes: an import of an export, exported again, answers the archive it read.
Provisioned files stay out.

### `POST /operator/revisions/import`

The body is a ZIP, sent as it is with `Content-Type: application/zip` and a
`Ferrobridge-Reason`. The bridge reads at most 4096 entries and 64 MiB of
content, inside the `[server]` body ceiling, and every entry names a
`.yml` or `.yaml` file under `fhirconnect/` or `omocl/`. It compiles the
set with the archive's files standing in for the console's, as a dry run,
and records them as a revision with `origin` `import` that does not run
until an apply.

| Answer | When |
|---|---|
| `201` | The set compiles with the archive's files. The body is the check's report, `Location` the new revision and the `ETag` its id |
| `400` | No reason, or a body that is no archive the bridge reads (`bad-archive`) |
| `409` | No `[state]` is configured |
| `422` | The set does not compile with the archive's files; the body names every diagnostic, and nothing is recorded |
| `503` | The CDR did not serve a template, so the set could not be checked |

## What the log records

Every operator request leaves one audit line on the
`ferrohealth_smart::audit` target, the record every FerroHEALTH product
writes: the token's `sub` as `principal`, its `client_id` and issuer, the
caller's `kind`, the scheme (`via`), the method, the matched route (such as
`/operator/mappings/{*path}`), the interaction `operator`, the status, the
duration and the request id, beside the ordinary request line. A save adds
one more line on the `ferrobridge::audit` target with the caller's `subject`,
`action` `save`, the `file` it wrote, `old_sha256` (`-` for a new file),
`new_sha256`, the `reason` and the status; a reload adds `action` `reload`.
An apply adds `action` `apply` with the `revision` it applied, the
`running` revision it replaced, the revision it `recorded` and the
`reason`. An import adds `action` `import` with the archive's
`archive_sha256`, the number of `files`, the revision it `recorded` and the
`reason`.
No line carries a token or any mapping content.
