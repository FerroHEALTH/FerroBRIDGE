<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# The operator API

The operator API lets an operator console read what the bridge runs and the
mapping files it runs from, without a shell on the host. It is off by
default. No specification governs it: it is FerroBRIDGE's own design.

## Turning it on

Set `[operator] enabled = true` (or `FERROBRIDGE__OPERATOR__ENABLED=true`).
The API needs inbound authentication, so the server refuses to start when
`[operator] enabled` is true and `[auth] enabled` is not. The routes are
mounted under `/operator` on the server's one listener; the bridge has no
second admin listener.

```toml
[auth]
enabled = true
issuer = "https://auth.example.org/realms/clinic"
audience = "https://bridge.example.org/fhir"

[operator]
enabled = true
```

## The operator scope

Every route needs a bearer access token from the configured issuer, checked
as the FHIR facade checks one. The token's `scope` claim must carry the
operator scope, `auth.operator_scope`, which is `ferrobridge.operator` unless
you set another single scope token. SMART scopes cover FHIR resources only,
so the operator scope has nothing in SMART to overlap with.

| Request | Answer |
|---|---|
| no token, or a token that does not hold | `401` with `WWW-Authenticate: Bearer` (`error="invalid_token"` for a bad token) |
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
| `owner` | `provisioned`: the deployment owns the file |
| `read_only` | `true` for a provisioned file |
| `diagnostics` | The diagnostics the running load raised about the file |

## `GET /operator/mappings/{path}`

The file's bytes, verbatim, as `application/yaml`, with the SHA-256 as a
strong `ETag`. A path is plain names under a root: an empty, `.`, `..` or
`..`-prefixed segment answers `404`, and so does a file that resolves outside
its root once every symlink is followed, a path under no root, and a file
that is not `.yml` or `.yaml`.

## What the log records

Every operator request leaves one line on the `ferrobridge::audit` target
with the token's `sub`, the method, the matched route (such as
`/operator/mappings/{*path}`), the status, the duration and the request id,
beside the ordinary request line. Neither line carries a path value, a token
or any mapping content.
