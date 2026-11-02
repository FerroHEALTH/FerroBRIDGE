<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# The access record

Regulation (EU) 2025/327 Annex II 3.2 asks an EHR system to record, on every
access to personal electronic health data, who accessed it, which natural
person, the categories of the data, when, and where the data came from. The
CDR records each call the bridge makes, but ITS-REST carries no end-user
identity on a read, so the CDR's record names the bridge's own credential.
For an access through the FHIR facade, the bridge is the one system that
knows the person, so it writes the record itself when `[audit]` is on
([configuration](configuration.md#audit)).

## Where each surface's record lives

| Surface | Record | Where it lives |
|---|---|---|
| The FHIR facade | One Annex II 3.2 access record per access an authenticated caller makes: read, vread, search, history, create, update, delete, transaction, batch, `Composition/$document`, `$tofhir` and `$toopenehr` | The Audit Record Repository `[audit] base_url` names, as an IHE BALP `AuditEvent` |
| The HL7 v2 face | One system import record per message it commits (ITI-20 "PHI-import", the DICOM "Import" event), naming the sending facility and no person | The same repository |
| The OMOP ETL (`etl run`, `etl fhir`, `dq check`) and the incremental load | None of the bridge's own: these read in batch under the bridge's CDR credential, with no person behind the read | The CDR's own access log, which names the bridge's ETL client |
| The operator API | None of the bridge's own: it serves mapping files and no health data | The request log line of `ferrohealth-smart` |

Every guarded surface also keeps its request log line, which names the caller
the token verified, the route and the status, and never a body.

## What a facade record holds

- **Who accessed (3.2(a), (b)):** the token's issuer, subject, client and
  `jti`, or the Basic user, and the roles the token grants.
- **The categories (3.2(c)):** classified from the template and archetype ids
  of every composition and party the request read or wrote, through the
  `[audit.categories]` map. An id the map does not name leaves the record
  unclassified, and the record says so.
- **When (3.2(d)):** the moment the access was answered.
- **The origin (3.2(e)):** the CDR at `[cdr] base_url`, with each `ehr_id`
  the access reached and the patient the identity map binds to it.
- **The request:** the interaction, the resource it named, the search query,
  and the `X-Request-Id` the request log carries.

Each record states how long the repository keeps it, at least the three years
of Art 9(2), and the security labels of its categories.

## When a record cannot be taken

Recording fails closed. Before an access runs, the bridge takes the record's
place in the spool under `[audit] spool_dir`; when the spool is full, the
access is refused with `503` and an `OperationOutcome` of code `transient`
before the CDR is called. When the record of a finished access cannot be
stored, the answer is withheld with the same `503`. The HL7 v2 face answers
`AR` in both cases, and the sender's resend is safe, since the identity map
recognises a message it already committed.

A record waits in the spool until the repository answers `2xx`; the bridge
retries an unreachable repository with a backoff up to `retry_max_ms`, and a
record the repository refuses with a `4xx` is moved to the spool's
`quarantine` directory for an operator. Records still in the spool at a
shutdown are delivered by the next start. Size `max_records` and `max_bytes`
for the longest repository outage you plan for: a full spool refuses every
access until the repository takes records again.
