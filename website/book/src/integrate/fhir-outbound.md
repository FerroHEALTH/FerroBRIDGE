<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# The outbound FHIR lane

The outbound FHIR lane pushes each change in the CDR to a FHIR R4 server as
it happens: every composition a contribution changed is mapped to FHIR
through the loaded FHIRconnect programs and delivered to the endpoint you
configure. No specification governs the lane: it is FerroBRIDGE's own design.

The lane is a consumer of the [change feed](change-feed.md), so it runs
against any CDR, and it maps through the [FHIR facade](fhir-facade.md)'s
programs and identity map, so a resource the lane delivers has the id, the
content and the subject a facade read of it answers. It is off by default and
is switched on with `[change_feed]`, `[facade]` and `[fhir_outbound]`
([configuration](../operate/configuration.md#fhir_outbound)).

**The lane carries identifiable clinical data.** Every resource names its
patient. Point it only at an endpoint that is allowed to hold that data, over
TLS, and keep it apart from any stream that carries identifiers alone, such as
the [events lane](events.md). The server logs a warning naming the endpoint
host each time it starts the lane.

## What one event delivers

Each event becomes one FHIR R4 `transaction` Bundle posted to the endpoint's
base ([R4 http.html, transaction](https://hl7.org/fhir/R4/http.html#transaction)):

- **A created or updated composition:** the lane reads its latest version
  from the CDR and maps it. Every resource it maps to is a `PUT [type]/[id]`
  entry, carrying its `id`, `meta.versionId` (the composition's
  `version_tree_id`) and `meta.source` (the CDR URL of the composition
  version). A resource the composition's earlier delivery carried and this
  version no longer maps to is a `DELETE` entry.
- **A deleted composition** (the CDR answers `204`): a `DELETE` entry for
  every resource its last delivery carried.
- **An erasure** of a composition or of a whole EHR: a `DELETE` entry for
  every resource the lane delivered for it.

The lane keeps a record of what it delivered for each composition in the
`redb` file `[fhir_outbound] emitted` names, identifiers only, so a deletion
and an erasure can retract what the content no longer shows.

## Delivery guarantees

- **At least once:** the event is acknowledged only after the endpoint
  answered `2xx`. A delivery sent again is the same set of `PUT`s and
  `DELETE`s, which changes nothing at the endpoint.
- **A failure that may pass** (no answer, `401`, `408`, `429`, a `5xx`, the
  CDR or the terminology server not answering) leaves the event to be read
  again on the next pass, and never counts toward parking it.
- **A refusal of the content** (any other `4xx`, a composition no mapping
  admits) is retried `[change_feed] max_attempts` passes and then parked in
  the dead-letter store, where the operator API and the console can inspect
  and replay it.
- **No resource without a subject:** a composition of an EHR the identity map
  binds to no patient, or a resource of a type that names its subject and
  carries none, is refused and parked; nothing of the event reaches the
  endpoint.
