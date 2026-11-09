<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# The events lane

The events lane publishes a small message for every commit of the CDR to an
AMQP 0.9.1 broker (RabbitMQ), so downstream systems learn about a change
without polling: an analytics pipeline, a care-coordination service, a cache
that needs invalidating. No specification governs eventing: the lane is
FerroBRIDGE's own design.

The lane is a consumer of the [change feed](change-feed.md), so it runs
against any CDR: on the AQL source it publishes composition versions, and on
the reference CDR's outbox feed it publishes every commit of either outbox,
deletions and erasures included. It is off by default and is switched on with
`[change_feed]` and `[events]` ([configuration](../operate/configuration.md#events)).

## The message

Each changed version is one message, published persistently to a durable
topic exchange. The body is JSON: the event as the change feed served it, plus
the `version_index` of the version the message names.

| Member | Meaning |
|---|---|
| `contribution_id` | The contribution the change belongs to; absent on an erasure |
| `ehr_id` | The EHR; `null` for a demographic change |
| `committed_at` | The commit instant; absent on an erasure |
| `versions[]` | One entry per changed object: `vo_id`, `kind`, `sys_version`, `version_tree_id`, `change_type`, `template_id` |
| `seq` | The delivery sequence number of the reference CDR's outbox; absent on the AQL source |
| `event` | `erase` on an erasure tombstone of the reference CDR; absent otherwise |
| `version_index` | Which entry of `versions` this message is for |

The message carries identifiers and provenance, never clinical content. A
consumer that needs the content reads it from the CDR's REST API with its own
credentials, for example `GET /ehr/{ehr_id}/composition/{vo_id}`.

## Routing keys and queues

Every message is published under a three-word routing key:

```text
<kind>.<change_type>.<template_id>
```

For example `COMPOSITION.249.openEHR-EHR-COMPOSITION_encounter_v1`. AMQP topic
keys use `.` between words, so every character of a value outside
`[A-Za-z0-9_-]` becomes `_`, and an absent template is `-`
(`EHR_STATUS.251.-`, and `EHR.erase.-` for the erasure of an EHR). The change
type is the openEHR audit change type code: `249` creation, `250` amendment,
`251` modification, `523` deleted.

Bind a queue with the topic wildcards to pick what you need:
`COMPOSITION.*.*` for every composition change, `*.523.*` for every deletion,
`#` for everything. The lane declares the topology `[events]` names whenever it
opens a connection: the durable topic exchange, then each queue in `queues`,
durable, bound with its `binding`. Declaring them again changes nothing, so a
broker that was replaced gets its queues back on the next connection. There is
no subscription API: the configuration is the topology, and the operator API
shows it read-only in the running configuration.

## Delivery

- **At least once.** The lane publishes the messages of an event in order and
  waits for the broker's confirm of each. The change feed's watermark moves
  only after every message of an event was confirmed, so a crash or a refusal
  publishes the event again. Deduplicate on `contribution_id` with
  `version_index`.
- **In order.** Events are published in the order of their source: `seq` order
  per outbox on the reference CDR's feed, commit-instant order on the AQL
  source, which gives no order across EHRs when instants tie
  ([the AQL source's limits](change-feed.md)).
- **No event is dropped.** A broker that is down, a broker that refuses a
  message, a confirm that does not arrive within `confirm_timeout`, and a
  CDR that refuses the feed each stop the lane where it is: the events wait at
  the source, the `events` health indicator reports `degraded`, and readiness
  stays up, since nothing else the server serves depends on the lane.

The usage report counts each confirmed message as `events_published`, and no
log line carries a message body.

## Securing the broker URL

The broker URL carries credentials, so keep it in a secret: `url_file` reads
it once at boot from a mounted file, and the running configuration and every
refusal leave it out. Use TLS for anything beyond a local broker, with an
`amqps://` URL or `tls = true`.

## Moving from the reference CDR's own publisher

FerroEHR, the reference CDR, published these events to RabbitMQ itself up to
its 5.5 line. From 5.6.0 it keeps the outbox and serves it over the cursor
feed, and FerroBRIDGE publishes. To move a deployment:

1. Name a reader for FerroBRIDGE in FerroEHR's configuration
   (`[outbox] readers = ["ferrobridge"]`) and give the client FerroBRIDGE
   calls with the scope FerroEHR names for the outbox routes
   (`__ferroehr.outbox`).
2. Configure FerroBRIDGE with the same exchange FerroEHR published to
   (`ferroehr.events` by default):

   ```toml
   [change_feed]
   enabled = true
   source = "ferroehr"
   url = "https://ehr.example/ferroehr/rest/extensions/v1"
   dead_letters = "/var/lib/ferrobridge/dead-letters.redb"

   [events]
   enabled = true
   reader = "ferrobridge"
   url_file = "/run/secrets/broker-url"
   exchange = "ferroehr.events"
   queues = [{ name = "ferroehr.events.analytics", binding = "COMPOSITION.*.*" }]
   ```

3. Replace each subscription row FerroEHR kept with a `queues` entry. FerroEHR
   named a subscription's queue `<exchange>.<name>` and built its binding key
   from the row's `kind`, `change_type` and `template_id`, with `*` for an
   unset one; write that name and that key into `queues`, and your consumers
   keep reading the queues they read before.

The routing keys and the message body are the ones FerroEHR published, so a
consumer bound to them keeps matching. Upgrade FerroEHR and switch
the lane on together: the events committed between the two wait in the outbox
until the reader acknowledges them, inside FerroEHR's retention window.
