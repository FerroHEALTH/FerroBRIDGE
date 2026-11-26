<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# Upgrading to the state store

The release that moves the bridge's state into the
[PostgreSQL state store](high-availability.md) deletes every key that named a
local file and carries no alias for any of them: a configuration that still
names a removed key refuses to start, so `ferrobridge config check` finds
each one before you restart.

## Before you upgrade

1. Create a PostgreSQL 19 database and a role that owns it, and add
   `[state]` ([`[state]`](configuration.md#state)).
2. Generate `[state.seal]`: one sealing key and the `lookup_key`, each
   `openssl rand -base64 32`. Keep the `lookup_key` for good; the store pins
   its fingerprint and refuses to start under another.
3. Let a running bridge drain its audit spool: records left in the old
   spool directory are not read.
4. Note each HL7 v2 sender and the `[audit.categories]` map, which move into
   the store through the operator API after the upgrade.
5. Stand up the identity source (a PIXm Manager such as FerroPIX) and add
   `[identity_source]`.

## Every removed and moved key

| Removed | Where it went | How to carry the state across |
|---|---|---|
| `[facade] identity_store` | The identity map, in `[state]`, with identifiers under `[state.seal] lookup_key` | It is not carried: the map starts empty and fills as requests resolve |
| `[facade] subject_namespace`, `subject_id_form`, `subject_type` | `[identity_source]`: every EHR subject is the ITI-83 pseudonym in `pseudonym_domain` | Each EHR is found again through its pseudonym |
| `[shl] store` | `[state]` and `[state.seal]` | Links issued before the upgrade are not read; issue them again |
| `[facade.app_state] store`, `[facade.launch_task] store` | `[state]` | Not carried; the old `redb` files are not read |
| `change_feed.watermarks`, `change_feed.dead_letters` | `[state]`, under one lease per reader (`[change_feed] lease`) | A reader on the `aql` source starts again where `start` says |
| `POST /operator/events/{consumer}/parked/{id}/replay` | `…/retry` and `…/discard`, each with a reason in `ferrobridge-reason` | Parked events are not carried |
| `audit.spool_dir`, `audit.max_records`, `audit.max_size`, `audit.write_timeout` | The audit outbox in `[state]` | Drain the spool before the upgrade |
| `[audit.categories]` | The `audit-categories` settings register | `PUT /operator/registers/audit-categories` with the map you kept |
| `[operator] mapping_dir` and its `..revisions` directory | Mapping revisions in `[state]` | Save each console file again through the operator API; revision 1 is the provisioned tree |
| `[hl7v2] senders`, `profiles`, `default_charset`, `unmapped_entries`, `facility_identifier_issuer`, `ehr_policy`, `log_outcomes` | One `mllp` source per sender in `[state]` | `POST /operator/sources` for each sender with its route and settings |

An MLLP route that names both a `certificate_subject` and `source_cidrs`
needs a connection that proves both, so one that let a sender reach either
listener splits into two senders, or drops the proof it does not need.

## After you upgrade

`[facade]`, `[shl]`, `[change_feed]`, `[audit]`, `[hl7v2]` and `[inbound]`
each refuse to start without `[state]`. Check the `state` indicator on the
readiness route, then write the HL7 v2 senders, the audit categories and any
console mapping files through the operator API.
