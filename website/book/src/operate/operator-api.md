<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# The operator API

The operator API lets an operator console read what the bridge runs and the
mapping files it runs from, without a shell on the host. It is off by
default. No specification governs it: it is FerroBRIDGE's own design.

## Turning it on

Set `[operator] enabled = true` (or `FERROBRIDGE__OPERATOR__ENABLED=true`).
The API needs inbound authentication, so the server refuses to start when
`[operator] enabled` is true and `[auth] enabled` is false or `[auth]` names
no issuer. The routes are
mounted under `/operator` on the server's one listener; the bridge has no
second admin listener.

```toml
[auth]
issuer = "https://auth.example.org/realms/clinic"
audience = ["https://bridge.example.org/fhir"]

[operator]
enabled = true
mapping_dir = "/var/lib/ferrobridge/mappings"
```

## The operator scope

Every route needs a bearer access token from the configured issuer, checked
as the FHIR facade checks one, or the Basic credential of an
`[[auth.basic.users]]` user. The token's `scope` claim, or the user's
`scopes`, must carry the operator scope, `[auth] product_scope`, which is
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

Each refusal has a JSON body with `error` and `message`.

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

## `GET /operator/configuration`

The configuration the bridge started on, as one JSON document. The bridge
reads `ferrobridge.toml` and the environment once, at start, so the answer
holds what it runs on until the next start:

- `file`: the configuration file it read, as `--config` or
  `FERROBRIDGE_CONFIG` named it, or `null` when it read none;
- `read_at`: when it read the configuration, RFC 3339 UTC;
- `entries`: every key the configuration holds a value for, in key order,
  each with its dotted `key` (`cdr.credentials.oauth.client_id`), its
  `value` and its `source`: `file`, `environment` (a `FERROBRIDGE__`
  variable, `FERROBRIDGE_LOG_FORMAT` or `RUST_LOG`, which win over the
  file) or `default`.

A `value` carries its `kind` (`string`, `integer`, `float`, `boolean`,
`list`, `table` or `secret`) beside it. A secret is never answered by value.
A secret and its `_file` sibling are one entry, keyed by the secret
(`cdm.url`, `cdr.credentials.password`,
`cdr.credentials.oauth.client_secret`, the same two of `[terminology]`, and
`password_hash` of each `[[auth.basic.users]]` user), whose value is its
`state`: `set` (set as a value, in the file or the environment), `file` with
the `path` it is read from, or `unset`. The file of a private key
(`private_key_file`, `client_key_file`) is answered as a secret read from
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
| `owner` | `provisioned` for a file under a configured mapping directory, `console` for one the console saved under `[operator] mapping_dir` |
| `read_only` | `true` for a provisioned file, `false` for a console file |
| `diagnostics` | The diagnostics the running load raised about the file |

## `GET /operator/mappings/{path}`

The file's bytes, verbatim, as `application/yaml`, with the SHA-256 as a
strong `ETag`. A path is plain names under a root: an empty, `.`, `..` or
`..`-prefixed segment answers `404`, and so does a file that resolves outside
its root once every symlink is followed, a path under no root, and a file
that is not `.yml` or `.yaml`.

## Saving mapping files

With `[operator] mapping_dir` set, the console checks drafts, saves them and
reloads the running set. Without it, the three routes below answer `409`.

- **One owner per file.** The files under `[mappings] directory` and
  `[mappings] omocl` are provisioned and never written. A console save goes
  to `mapping_dir/fhirconnect/<path>` or `mapping_dir/omocl/<path>`, and a
  save whose path names a provisioned file answers `409`. Console files join
  the set beside the provisioned ones, at every reload and at start-up, where
  a set that does not compile refuses the start. A file with the path of a
  provisioned one is ignored in favour of the provisioned file.
- **OMOCL files reach `etl run`.** `serve` checks OMOCL files and does not
  run them. `etl run` reads `mapping_dir/omocl/` beside `[mappings] omocl`
  when its configuration turns the operator API on, with the same ownership
  rule, so a file saved here maps the next run. Two files that define the
  same mapping refuse the save, the reload and the run.
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
the draft's diagnostics named by `path`. Nothing is written.

### `PUT /operator/mappings/{path}`

The body is the file's new bytes, saved verbatim. A save carries:

| Header | Meaning |
|---|---|
| `If-Match` | The `ETag` the file has now, from the file route or a previous save; `*` matches any existing file |
| `If-None-Match: *` | Instead of `If-Match`, for a file that does not exist yet |
| `Ferrobridge-Reason` | Why the file changes: one line of at most 200 characters |

| Answer | When |
|---|---|
| `200` | The set compiled with the draft; the file is written and the running set replaced. The body is the new set's report and the `ETag` the new SHA-256 |
| `400` | No reason, or a path the console cannot write |
| `409` | A provisioned file holds the path, or no `mapping_dir` is configured |
| `412` | The file changed since the `ETag` given, or `If-None-Match: *` names an existing file |
| `422` | The set does not compile with the draft; the body names every diagnostic. Neither the file nor the running set changes |
| `428` | Neither `If-Match` nor `If-None-Match: *` |
| `503` | The CDR did not serve a template, so the set could not be compiled |

The draft is written to a hidden file beside its target and synced; only a
draft that compiles is renamed over the target and the directory synced, so a
reader sees the old file or the new one. The running set is then compiled
from disk and swapped in one step: the facade's programs and search
declarations, the FHIRconnect operations and the operator state, so the
`CapabilityStatement` names the new set at once. A request in flight finishes
on the set it started with. Checks, saves and reloads run one at a time.

### `POST /operator/reload`

Compiles the provisioned and console files on disk and swaps them in, with
the answers of a save: `200` with the report, `422` with the diagnostics and
the running set unchanged, `503` when an upstream did not answer.

The quickstart `compose.yaml` runs the server with a read-only root file
system and mounts `./operator-mappings` (or `FERROBRIDGE_OPERATOR_MAPPINGS`)
at `/var/lib/ferrobridge/mappings`, the `mapping_dir` it sets. The container
runs as `65532:65532`, so hand that directory to that user before turning
the API on.

Each replica reloads on its own. With `mapping_dir` on a shared volume, a
file one replica saved reaches another at that replica's next reload or
restart; the console reloads the replica behind its configured URL.

## What the log records

Every operator request leaves one audit line on the
`ferrohealth_smart::audit` target, the record every FerroHEALTH product
writes: the token's `sub` as `principal`, its `client_id` and issuer, the
scheme (`via`), the method, the matched route (such as
`/operator/mappings/{*path}`), the interaction `operator`, the status, the
duration and the request id, beside the ordinary request line. A save adds
one more line on the `ferrobridge::audit` target with the caller's `subject`,
`action` `save`, the `file` it wrote, `old_sha256` (`-` for a new file),
`new_sha256`, the `reason` and the status; a reload adds `action` `reload`.
No line carries a token or any mapping content.
