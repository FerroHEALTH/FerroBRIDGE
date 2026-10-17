<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# The usage report

`ferrobridge serve` sends a small JSON report to FerroPULSE, a collector that
Cadasto B.V. runs for every FerroHEALTH product. It goes out at each process
start and once a day. It says that the server is alive, which version it
runs, which lanes it serves, whether its upstreams answer, and how fast it
answers, in coarse aggregates. The report is on by default. Every boot logs
one INFO line that says whether it is on and how to switch it off.

This page lists every field the report carries, says when it is sent, gives
the switch, and carries the privacy notice for the data the collector stores.
The wire format is FerroPULSE's report envelope v2; no FHIR, openEHR or OMOP
specification governs any of it, and the report is FerroBRIDGE's own design.

<!-- toc -->

## When a report is sent

There are two kinds of report, and both are one HTTPS `POST` to
`https://report.ferropulse.eu/v1/report` with a JSON body of at most 64 KiB.

- **`start`:** at each `serve` start, once the lanes have started (after the
  CDR answered, when a lane needs it). A server sends at most one start report
  per ten minutes: the time of the last one is kept in the state directory, so
  a container in a restart loop sends one start report per ten minutes, not
  one per restart.
- **`daily`:** the first attempt 10 minutes after the start report, then
  about every 24 hours (23.5 to 24.5 hours after the previous one, spread at
  random). It covers the requests since the previous daily report of the same
  process, or since the lanes started.

The jobs `cdm init`, `etl run`, `mapping check` and `vocab load` send nothing.

The report needs a state directory, `[usage_report] state_dir`
(`FERROBRIDGE__USAGE_REPORT__STATE_DIR`), where the server keeps the instance
id. Without one, or with one it cannot write, the server sends nothing:
without a stable id each restart would count as a new installation. The
compose file is the one deployment that turns it on: it mounts the named
volume `ferrobridge-state` at `/home/nonroot`, the image's home directory,
which the runtime user (uid 65532) owns, and sets
`FERROBRIDGE__USAGE_REPORT__STATE_DIR=/home/nonroot/state`. The container
image sets no state directory, so a plain `docker run` or a Kubernetes pod
sends nothing until you mount a writable volume and point the key at it:

```shell
docker run -v ferrobridge-state:/home/nonroot \
  -e FERROBRIDGE__USAGE_REPORT__STATE_DIR=/home/nonroot/state \
  ghcr.io/ferrohealth/ferrobridge:<version>
```

A bind mount needs a host directory that uid 65532 can write.

The boot line, with the report on, reads:

```text
usage report ON: instance id, version, licence type, deployment, uptime, host size, enabled lanes, mapping counts as ranges, upstream health and coarse request latency per route group, at each start and once a day; no clinical content, identifiers, mapping names, URLs or request paths. Off: FERROBRIDGE__USAGE_REPORT__ENABLED=false. Details: https://ferrobridge.eu/docs/operate/usage-report.html
```

It carries the configured endpoint as the structured field `endpoint`. With
the report off, the line says why, for example:

```text
usage report OFF: switched off (FERROBRIDGE__USAGE_REPORT__ENABLED=false); nothing is sent. Details: https://ferrobridge.eu/docs/operate/usage-report.html
usage report OFF: CI detected (GITHUB_ACTIONS); nothing is sent. Details: https://ferrobridge.eu/docs/operate/usage-report.html
usage report OFF: no state directory (FERROBRIDGE__USAGE_REPORT__STATE_DIR is unset), and without a stable instance id each restart would count as a new installation; nothing is sent. On: mount a writable volume and set FERROBRIDGE__USAGE_REPORT__STATE_DIR to it. Details: https://ferrobridge.eu/docs/operate/usage-report.html
```

## What a report contains

Every report carries these fields:

| Field | Sent in | What it is |
|---|---|---|
| `schema` | both | Always `2`, the version of the report format. |
| `event` | both | `start` or `daily`. |
| `first_start` | `start` | `true` when this start created the instance id, which marks a new installation; `false` on every later start. |
| `product` | both | Always `ferrobridge`. |
| `instance_id` | both | A random UUID (version 4), created at the first start and stored in `<state_dir>/instance-id`. It is not derived from the host or anything else about the deployment. |
| `version` | both | The server version, for example `0.0.7`. |
| `git_sha` | both | The commit the binary was built from, 7 to 40 hex digits. A build outside a git checkout sends `0000000`. |
| `licence` | both | The licence grant type, `non-commercial` (what the Business Source License grants everyone). |
| `deployment` | both | How the server was deployed: `helm`, `compose`, `binary` or `unknown`, from `[usage_report] deployment`, which the compose file sets to `compose`. |
| `uptime_s` | both | Seconds since this process started. |
| `cpu_bucket` | both | The CPUs available to the process, as a range: `1-2`, `3-4`, `5-8`, `9-16` or `17+`. |
| `memory_bucket` | both | The memory available to the process in GiB, the smaller of the host's memory and the container limit, as a range: `<4`, `4-8`, `8-16`, `16-32` or `32+`. Off Linux the server cannot read it and sends `<4`. |
| `status` | both | The state facts below. |
| `metrics` | `daily` | The performance aggregates below. |

The `status` object. A member for a lane or an upstream the deployment does
not configure is left out.

| Member | What it is |
|---|---|
| `lanes` | The lanes the configuration switches on, sorted: `etl`, `facade`, `hl7v2`, `operations`, `terminology`. `etl` means an `[etl]` section is present; `serve` never runs it. |
| `auth` | Whether the FHIR routes check a bearer token (`[auth] enabled`). |
| `facade_mappings` | The programs the FHIR facade compiled, as a range: `0`, `1-10`, `11-50`, `51-200` or `201+`. Never a mapping name, a template id or a resource type. |
| `operations_mappings` | The programs the FHIRconnect operations compiled, as the same ranges. |
| `cdr_ok` | Whether the openEHR CDR answered its readiness check when the report was built. |
| `terminology_ok` | Whether the terminology server answered its readiness check. |
| `cdm_ok` | Whether the OMOP CDM database answered its readiness check. |
| `identity_store_ok` | Whether the facade's identity store answered its readiness check. |
| `hl7v2_listener_ok` | Whether the HL7 v2 listener accepts connections. |

The health members run the same checks as `GET /health/readiness`, so a
report reads what a readiness probe would have read at that moment.

A start report, formatted here for reading (the server sends it on one line):

```json
{
  "schema": 2,
  "event": "start",
  "first_start": true,
  "product": "ferrobridge",
  "instance_id": "5f0c6b2e-3d4a-4c8b-9e1f-2a3b4c5d6e7f",
  "version": "0.0.7",
  "git_sha": "1a2b3c4d",
  "licence": "non-commercial",
  "deployment": "compose",
  "uptime_s": 4,
  "cpu_bucket": "3-4",
  "memory_bucket": "8-16",
  "status": {
    "lanes": ["facade", "operations", "terminology"],
    "auth": true,
    "facade_mappings": "11-50",
    "operations_mappings": "1-10",
    "cdr_ok": true,
    "terminology_ok": true,
    "identity_store_ok": true
  }
}
```

### The daily metrics

The daily report adds one object that covers the window since the previous
daily report:

```json
"metrics": {
  "window_24h": {
    "requests_bucket": "1k-10k",
    "groups": {
      "facade_read": {
        "p50_ms": 18.2, "p95_ms": 61.0, "p99_ms": 140.5,
        "errors_5xx": 0,
        "histogram": [0, 40, 900, 1500, 300, 60, 5, 1, 0, 0, 0]
      }
    },
    "counters": {
      "responses_4xx": 12,
      "hl7v2_aa": 0, "hl7v2_ae": 0, "hl7v2_ar": 0,
      "mapping_failures": 3
    }
  }
}
```

- **`requests_bucket`:** the number of counted HTTP requests and HL7 v2
  messages in the window, as a range: `0`, `1-100`, `100-1k`, `1k-10k`,
  `10k-100k` or `100k+`. Each range includes its lower bound and excludes its
  upper one, so `1-100` means 1 to 99. The health probes
  (`/health/liveness`, `/health/readiness`, `/health/info`) and the root
  document `/` are not counted.
- **`groups`:** one entry per group of requests that saw at least one in the
  window. A request is placed in a group by the route template it matched,
  never by the path or the parameters the client sent:

  | Group | Routes | Direction |
  |---|---|---|
  | `facade_read` | `GET /fhir/{type}/{id}`, `GET /fhir/{type}/{id}/_history/{vid}` | openEHR to FHIR |
  | `facade_search` | `GET /fhir/{type}`, `POST /fhir/{type}/_search` | openEHR to FHIR |
  | `facade_write` | `POST /fhir/{type}`, `PUT /fhir/{type}/{id}` | FHIR to openEHR |
  | `facade_transaction` | `POST /fhir` | FHIR to openEHR |
  | `facade_validate` | `POST /fhir/{type}/$validate` | |
  | `facade_metadata` | `GET /fhir/metadata` | |
  | `operations_to_fhir` | `POST /fhir/$tofhir`, `POST /fhir/tofhir` | openEHR to FHIR |
  | `operations_to_openehr` | `POST /fhir/$toopenehr`, `POST /fhir/toopenehr` | FHIR to openEHR |
  | `other` | every other route, and a request no route matched | |
  | `hl7v2_message` | a message on the HL7 v2 listener, timed from its frame to its acknowledgment | HL7 v2 to openEHR |

- **`p50_ms`, `p95_ms`, `p99_ms`:** the median, 95th and 99th percentile
  latency of the group in milliseconds, estimated from its histogram.
- **`errors_5xx`:** the responses with a 5xx status. An HL7 v2 message has
  no HTTP status, so `hl7v2_message` always carries 0; its outcomes are the
  acknowledgment counters.
- **`histogram`:** 11 request counts, one per latency bucket. The bucket upper
  bounds in milliseconds are 5, 10, 25, 50, 100, 250, 500, 1000, 2500, 5000,
  and no bound for the last.
- **`counters`:**
  - `responses_4xx`: the responses with a 4xx status over every counted
    group.
  - `hl7v2_aa`, `hl7v2_ae`, `hl7v2_ar`: the acknowledgments the HL7 v2 face
    sent with each code (HL7 table 0008). `hl7v2_ar` also counts the `AR`
    answered to a frame the listener could not read, which is no message
    and is not timed in `hl7v2_message`.
  - `mapping_failures`: each resource or composition a FHIRconnect program
    refused to map, on a facade write or transaction, on the HL7 v2 face's
    ingest and on a `$tofhir` or `$toopenehr` call. A refusal the engine
    returns because a terminology server did not translate a `conceptmap`
    code is counted too. A request refused before any program ran (a body
    in the wrong media type, a resource no program maps, a CDR that refused
    the commit) is not.

## What a report never contains

A report carries no patient data, no resource, composition or message
content, no code or display text, no identifier other than the instance id
(no `ehr_id`, subject id or FHIR resource id), no mapping file name, mapping
text or template id, no request path or parameter, no CDR, terminology or
database URL or host, no error message, no user name, no client IP address,
no host name and no customer or staff name. The report types in the server
have no field that could hold any of these, and the collector refuses a
report with a value that is not a boolean, a number or a short lower-case
token.

The collector does record the IP address the report arrives from. See
[What the collector stores](#what-the-collector-stores).

## Switch it off

```shell
FERROBRIDGE__USAGE_REPORT__ENABLED=false
```

or, in the configuration file:

```toml
[usage_report]
enabled = false
```

The compose file forwards this variable from your shell, so
`FERROBRIDGE__USAGE_REPORT__ENABLED=false docker compose up` switches the
report off there too. In a CI job the report is off by itself: when
`GITHUB_ACTIONS` or `CI` is `true` or `1`, the server sends nothing, and
`enabled = true` does not change that. A container does not inherit the
runner's environment, so a test lane that starts the image passes
`--env FERROBRIDGE__USAGE_REPORT__ENABLED=false`; the guard
`scripts/checks/usage-report-off.sh` fails a launcher in this repository that
does not.

With the report off, the server sends nothing to FerroPULSE and writes nothing
to the state directory, and the boot line says `usage report OFF`. Nothing
else in the server changes. Switching it off stops new reports only. Reports
already stored at the collector stay until the retention period ends, unless
you ask for them to be erased.

## Network

- **Destination:** outbound HTTPS to `report.ferropulse.eu`, port 443.
  `[usage_report] endpoint` moves it; the server refuses to start on an
  endpoint that is plain HTTP (except to a loopback host) or that carries
  credentials.
- **One direction only:** the server reads the status code of the answer and
  nothing else. It does not follow redirects, and nothing the collector
  answers changes the server's configuration or behaviour.
- **Failures stay local:** a send gives up after 5 seconds without a
  connection or 10 seconds in total. A failure is logged at DEBUG, and a
  refusal by the collector at WARN. Boot, readiness and request handling
  never wait on the report, so a slow or unreachable collector affects none
  of them. A daily report that does not reach the collector is tried again an
  hour later with the same window; one the collector answered, even with a
  refusal, is not resent.

## Privacy notice

This section gives the information that Articles 13 and 14 of the
[GDPR](https://eur-lex.europa.eu/eli/reg/2016/679/oj) list, for the personal
data a report and its stored record can contain. A report names no person.
It can still relate to one: the collector stores the address each report
comes from, and a server run by one person on their own internet connection
can be linked to that person through the address or through the instance id.
Where that is so, the report is personal data, and this notice applies to it.
The collector is not anonymous.

### Controller

Cadasto B.V., the Netherlands. Write to
[info@cadasto.com](mailto:info@cadasto.com) or use
<https://www.cadasto.com/contact/>. Cadasto has not appointed a data
protection officer.

### Purposes and lawful basis

Cadasto processes the reports for three purposes:

- to know which FerroBRIDGE versions run in the field, so it can tell how
  many installations a security release concerns and whether they upgrade;
- to know which lanes the installations run;
- to see how the routes perform in real use, so it can find and fix slow
  paths.

The lawful basis is Art. 6(1)(f) GDPR, legitimate interest. The interest is
Cadasto's, as the maker of FerroBRIDGE: keeping the product it publishes
secure and maintained, which needs the versions in use and the reach of each
security release, and improving it, which needs the lane mix and field
performance. The data come from the FerroBRIDGE server itself. You are not
obliged to send them: the report can be switched off, and switching it off
changes nothing else in FerroBRIDGE.

### What the collector stores

- Every report as it arrives, with the fields listed above.
- With each report: the time of receipt, the UTC day, the source IP address
  (IPv4 or IPv6), and the country derived from that address.
- Per instance id: when it was last seen, its current version, and whether it
  is alive.
- Per day: latency totals summed across the whole fleet, without instance ids.

The country comes from an IP-to-country database bundled with the collector,
so no address leaves the collector host for the lookup. IP geolocation by
[DB-IP](https://db-ip.com), licensed
[CC BY 4.0](https://creativecommons.org/licenses/by/4.0/). Reports from the
address ranges of GitHub-hosted CI runners are marked as such and left out of
every dashboard.

### Recipients and location

Hetzner Online GmbH (Gunzenhausen, Germany) hosts the collector in its cloud
and processes the reports as Cadasto's processor, under a data processing
agreement (Art. 28 GDPR). The collector runs in Hetzner's data centre in
Falkenstein, Germany. The FerroPULSE dashboard that shows the reports sits
behind a GitHub login with no public or anonymous access; only members of the
FerroHEALTH organisation can sign in.

### Retention

A daily job deletes reports and start reports 13 months after they were
received, an instance record 13 months after the instance was last seen, and
the fleet-wide daily totals 13 months after their day. Backups of the
collector's database are kept for 14 days, so a deleted report can remain in a
backup for at most 14 days more.

### Your right to object

> **You have the right to object** at any time, on grounds relating to your
> particular situation, to the processing of reports that are personal data
> concerning you (Art. 21(1) GDPR).
>
> **To stop future reports**, switch the report off as described in
> [Switch it off](#switch-it-off). No request to anyone is needed.
>
> **To have the reports already stored erased**, write to
> [info@cadasto.com](mailto:info@cadasto.com) with the instance id, which the
> server keeps in `<state_dir>/instance-id`. Under the compose file it sits on
> the `ferrobridge-state` volume:
> `docker run --rm -v <project>_ferrobridge-state:/s busybox cat /s/state/instance-id`
> prints it. Cadasto then deletes every report, start report and instance
> record stored for that instance id, source addresses included
> (Art. 17(1)(c) GDPR). Switch the report off first, or the next report starts
> a new record.
>
> The fleet-wide daily totals hold no instance ids, so an erased instance's
> share in days already summed stays in those totals.

### Your other rights

At the same address you can ask for access to the reports stored for your
instance id (Art. 15), for their rectification (Art. 16) or erasure (Art. 17),
or for restriction of their processing (Art. 18). You have the right to lodge
a complaint with a supervisory authority, in particular in the Member State of
your habitual residence, place of work or place of the alleged infringement
(Art. 77).
