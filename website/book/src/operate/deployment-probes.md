<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# Checking a deployment

After you start or upgrade a FerroBRIDGE deployment, run
`scripts/deploy-probe.sh` from a checkout of the repository against its base
URL. The script asks each HTTP lane what state it is in and prints one finding
per lane. It needs `bash` and `curl`, and it writes nothing to the CDR.

No specification governs this suite, its lanes or its findings: this page is
FerroBRIDGE's own design.

<!-- toc -->

## Run it

```sh
scripts/deploy-probe.sh https://bridge.example.org
```

State what you configured, so a lane in the wrong state fails the run:

```sh
scripts/deploy-probe.sh \
  --expect facade=on --expect operations=off \
  https://bridge.example.org
```

| Option | Meaning |
|---|---|
| `--expect <lane>=<on\|off>` | Fails the run when the lane is not in that state. Repeat it per lane. |
| `--json <file>` | Also writes the findings, with the evidence of each probe, to `<file>` |
| `--token-file <file>` | Sends the bearer token in `<file>` on the `/fhir` probes, for a facade behind `[auth]` |
| `--timeout <seconds>` | How long each request may take, 10 by default |

## The lanes and what each probe asks

| Lane | Probe | `on` | `off` |
|---|---|---|---|
| health | `GET /health/liveness`, `GET /health/readiness`, `GET /health/info` | each answers `200` | each answers `404` |
| facade | `GET /fhir/metadata` | `200` with a `CapabilityStatement` | `404`: a disabled facade mounts no route |
| operations | `POST /fhir/$tofhir` with an empty `Parameters` | `400` or `422` with an `OperationOutcome`: the engine refused the empty request, so the lane is serving | `503` with the issue code `not-supported`: no `[mappings]` set is configured |

The operations probe sends a request the engine must refuse, so it proves the
lane is mounted and compiled without needing a composition of yours.

## The four findings

| Finding | When |
|---|---|
| `on` | Every probe of the lane got the answer of a served lane |
| `off` | Every probe of the lane got the answer of a lane that is not configured |
| `failing` | A probe got a `5xx` other than the operations lane's `not-supported`, a `200` the lane never gives (a facade `200` that is no `CapabilityStatement`), a `401` or `403` with the token you supplied, or the lane is half mounted (one probe `on`, another `off`) |
| `uncovered` | A probe got no HTTP answer, a `401` or `403` with no token supplied, or a status no rule above classifies |

A readiness `503` is `failing`, and the script prints the start of its body,
which names the indicator that is down. During the start, before the lanes are
mounted, every route but `/` and the health routes answers `503`, so run the
script after readiness answers `200`.

An `uncovered` lane is never counted as passed. The script cannot tell you
anything about a lane it did not observe, so it fails with its own exit
status rather than letting silence read as success.

The report ends with the findings each lane was not in. One run sees only the
state the deployment is in: a run against a deployment with the facade on
says nothing about how the facade behaves when it is off or failing, and the
report lists those findings as not observed.

## Exit status

| Status | Meaning |
|---|---|
| `0` | Every lane observed, none failing, every `--expect` met |
| `1` | A lane is `failing`, or a lane is not in the state `--expect` names |
| `2` | The arguments are not valid |
| `3` | A lane is `uncovered`, and none is failing |

`scripts/deploy-probe.sh --self-test` runs the script against a stub `curl`
through every finding of every lane, without a server.
