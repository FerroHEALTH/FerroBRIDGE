<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# Failure and identity behaviour

A bridge carries clinical data between systems, so a silently dropped element
or a swallowed upstream error becomes a wrong record in the receiving system.
FerroBRIDGE fails loudly instead. The FHIRconnect engine chapter states its
guidance as recommendations, so each rule below is pinned as FerroBRIDGE's own
decision, and this page is the operator-facing statement of it.

<!-- toc -->

## All or nothing per unit

A transaction Bundle that cannot be mapped in full is refused with an
`OperationOutcome` naming every failing entry, and nothing is committed. A
batch Bundle answers per entry and counts its failures, because FHIR R4 defines
batch that way. There is no best-effort mode. On the OMOP side the unit is one
composition's whole record graph: a composition becomes many linked rows, and
they commit together or not at all, so no `FACT_RELATIONSHIP` row ever points
at a record that was never written.

## Type coercion is strict

A value that does not parse as the RM leaf type the Web Template resolved is a
refusal. FerroBRIDGE does not round a number, truncate a string, or guess a
date format to make a value fit.

## An upstream failure stays a failure

A refused CDR call, a failed terminology lookup, or a timeout is reported with
the upstream status. It never becomes an empty value, a default, or a silently
missing element. When a display cannot be resolved through `$lookup`, you get
an error that says so, not a resource with a coding missing its display.

## Unmapped OMOP codes are recorded, never dropped

A source code with no standard concept lands as `concept_id = 0` with the
source value kept in its source column. That is the CDM's own answer for "no
matching concept", and it keeps the row auditable. A row is never discarded
because its code did not resolve, and every run reports how many codes landed
there, per mapping, because the reference implementation's own authors measured
8.65% of primary concepts doing so and called that an underestimate.

## Identity

On the FHIR side an exported resource id is derived deterministically, as the
specification recommends, from the composition's stable `versioned_object_uid`,
the entry path and the split occurrence, so it stays the same across
composition versions, which FHIR requires of a logical id; `meta.versionId`
carries the openEHR version. FerroBRIDGE also keeps a persistent id-map: the
patient identifier to `ehr_id` relation, the external to internal resource id
relation, and the resource id to composition relation, so a `PUT` resolves and
a re-sent Bundle is recognised. It also records each `Resource.identifier` a
committed resource carries against its resource id, which is what a
conditional create's `identifier` search reads.

A resource the mapping creates beside the one it maps, one per further
occurrence of a `hierarchy.split` with `create: resource` or one per
`reference` mapping going out of openEHR, gets a binding of its own the first
time the facade answers its parent. Its id is a digest over the parent's
entry, the mapping that creates it and the openEHR occurrence it is created
for, so it stays the same across re-runs and composition versions. The parent
references it as `<Type>/<id>`, and `GET [base]/<Type>/<id>` answers it on
its own: the facade maps the parent's entry again and returns the created
resource of that id. The binding holds identifiers only, never content.

The id-map file records the schema it was written in. This build writes
schema 3 and reads schemas 1 to 3; a file with no schema row was written by
0.0.6 or earlier and reads as schema 1. Schema 3 adds the binding of a
created resource, so a file of schema 1 or 2 reads unchanged. Opening the
store checks the schema and stamps the file with schema 3. A file in any
other schema, for example one a later build wrote, stops the start with an
error that names the file, the schema it found and the schemas this build
reads, and the file is left unchanged.

Every openEHR identifier the id map holds is checked again when a request
reads it: an `ehr_id` must not carry a character that would change the
request path, and a version container must match the openEHR BASE
`HIER_OBJECT_ID` grammar. A stored value that fails its check answers
`500 Internal Server Error` with an `exception` issue. The issue names the
resource the record belongs to (`Condition/<id>`), or the resource id a
consumed source was recorded as, together with the field and the value it
holds. The CDR is not called, and nothing is written. Opening the store does
not scan the records, so a malformed value surfaces on the first request that
reads it.

## A re-sent transaction commits once

Every entry the facade commits records the source it consumed. A resource is
keyed by its `resourceType`, `id` and `meta.versionId`, the same key a single
create uses. A message is keyed by its type, its control id and the entry's
position in the Bundle, so a redelivered message is recognised even when it is
rebuilt with fresh resource ids. When every entry of a Bundle was consumed
before, the Bundle commits nothing and answers the resources the first delivery
created. When only some were, the Bundle is refused with `409` and nothing is
committed. An entry without an `id` has no key, so its Bundle commits again
each time it is sent. These rules are FerroBRIDGE's own design.

The CDR does not say in which order it lists the versions of a contribution.
After the commit, FerroBRIDGE reads each version back and matches it to the
entry it came from by the `FEEDER_AUDIT` the engine wrote: the item's type,
its id and its version. Where several entries name the same item, as the
entries of one message do, the composition content decides. That comparison
masks what two mappings of one entry can differ in without the source
changing: the composition `uid` the CDR assigns, and every time the engine
filled from its clock because no mapping wrote it. The engine reads the clock
once per resource and hands that instant to the builder as the `ctx/time`
default, which fills `EVENT_CONTEXT.start_time`, `HISTORY.origin`,
`EVENT.time` and `ACTION.time` where the mapping left them empty (ITS-REST
Simplified Formats, master06 §time). Each `DV_DATE_TIME` whose value is that
instant is left out of the comparison, so a Bundle mapped again at a later
instant still matches the compositions its first delivery stored. A version that
matches no entry or more than one refuses the answer with a `500` that names
the contribution, and no binding is recorded.

The commit is two steps. First the facade commits the contribution with
`Prefer: return=representation` and records the contribution uid the CDR
answers against every keyed entry of the Bundle, before it binds any of them.
Then it binds each entry from the versions the answer lists. A CDR that
ignores the header answers an empty `201` that still names the contribution,
and the facade then reads the contribution back with
`GET /ehr/{ehr_id}/contribution/{contribution_uid}` and binds from its
versions, with the same `FEEDER_AUDIT` check. A contribution that cannot be
read back or bound is a `500` naming the contribution, and the recorded uid
stays. When you send the same Bundle again, the facade finds that uid,
commits nothing, reads the contribution back and binds from it, and answers
`200 OK` for each entry as for any re-sent Bundle. Each entry's identity is
read from the composition the CDR stored at its version, on the first
delivery and on a retry alike, so a retry binds the id the first delivery
would have bound.

A single create shares that rule. When you `POST` a resource whose `id` and
`meta.versionId` a transaction committed and did not finish binding, the
facade reads the recorded contribution back, finds the version whose
`FEEDER_AUDIT` names the resource, binds it, commits nothing, and answers
`200 OK` with the composition the first delivery produced. The other versions
of that contribution belong to the transaction's other entries and are passed
over.

## A re-sent single create commits once

A single create (`POST [base]/{type}`) of a resource whose `id` the identity
map already consumed follows the same rule as a re-sent transaction, keyed by
the same `resourceType`, `id` and `meta.versionId`. FHIR R4 says nothing about
a repeated create with a client-assigned id, so these rules are FerroBRIDGE's
own design.

- **The same `id` and the same `meta.versionId`:** the create is a replay. The
  facade commits nothing and answers `200 OK` with the `Location`, the `ETag`
  and the body of the composition the first delivery produced, at the version
  it stands at now. A re-sent Bundle gets the same answer per entry.
- **The same `id` and another `meta.versionId`:** a changed `meta.versionId`
  means the sender changed the resource, so the create commits a later version
  of the composition the first delivery produced and answers `200 OK`. That is
  the update a `PUT` performs, following the version the CDR holds now.
- **The same `id` and no `meta.versionId`:** the key has no version to compare,
  so the facade maps the resource and compares the result with the composition
  as the CDR holds it now. The comparison masks the same fields as the
  transaction path: the composition `uid` and every time the engine filled from
  its clock. The same content is a replay, answered as above. Other content is
  the later version.
- **An `id` the map binds to more than one composition:** the facade cannot
  tell which one the create revises, so it answers `409 Conflict` with a
  `conflict` issue and commits nothing. Only a store written before 0.0.4
  can hold such an `id`. Each of those compositions is a resource of its
  own, and the issue names every resource id (`Condition/<id>`, one per
  composition). Send the change as an update, `PUT [base]/Condition/<id>`
  with that `id` in the body, to the resource it revises. The store keeps
  the split, so every later change of that source goes the same way.

A transaction entry follows the same lookup. An entry whose exact key the map
consumed, or a transaction committed, is recognised as above. An entry whose
`id` the map consumed at another `meta.versionId` commits a later version of
that composition inside the Bundle's contribution: its `UpdateVersion` names
the composition's latest version as `preceding_version_uid` and its audit
states a modification, beside the creations of the other entries. The entry
answers `200 OK` in the `transaction-response`, keeps its resource id, and its
`Location` names the new version. The commit is recorded before the binding
and matched by `FEEDER_AUDIT` as for any entry, so a retry after a failed
binding reads the contribution back and commits nothing. An entry the map
binds to more than one composition, or to a composition in another EHR,
refuses the Bundle with `409`, and two entries that revise one composition
refuse it with `422`. `PUT` is unchanged.

Two deliveries of one source that overlap commit once. Before a delivery reads
the identity map, it claims the key of every entry it carries, and it holds
those claims until it answers, whether it commits, is refused, or fails. A
second delivery that finds any of its keys claimed commits nothing and answers
`409 Conflict` with an `OperationOutcome` whose `duplicate` issues name each
entry another delivery holds. The rule is the same for a transaction and for a
single create of a resource with an `id`, and a single create also claims the
`resourceType` and `id` alone, so two creates of one `id` at different
versions do not run at once. Retry after the first delivery has
answered: the retry is then recognised as a re-sent Bundle or resource, as
described above. The claims live in the server process, one set per identity
store, so they order deliveries that reach the same FerroBRIDGE instance.

On the OMOP side every CDM v5.4 primary key is a 32-bit integer, so ids come
from database sequences and a bridge-owned side table maps each source record
to its row. That table is what makes a re-run replace rather than duplicate,
and it keeps the openEHR identity in the `*_source_value` columns.

The FHIR scheme has a failure mode worth knowing before you run it: a re-sent
Bundle that omits one resource reads as a different mapping. It is documented
rather than hidden.

## An HL7 v2 sender and receiver become MessageHeader endpoints

A FHIR R4 `message` Bundle opens with a `MessageHeader`
(<https://hl7.org/fhir/R4/bundle.html#invs>, `bdl-12`), whose
`source.endpoint` and `destination.endpoint` are required `url` values
(<https://hl7.org/fhir/R4/messageheader.html>). The v2-to-FHIR guide writes
MSH-3 (Sending Application) and MSH-5 (Receiving Application) there, and both
are `HD` values. FerroBRIDGE builds the endpoint from the `HD` in this order:

1. When HD.3 (the universal ID type) is `ISO`, `UUID`, `DNS` or `URI` and
   HD.2 (the universal ID) is valued, the endpoint is the form the guide's
   `HD` endpoint map assigns: `urn:oid:`, `urn:uuid:`, `urn:dns:` or
   `urn:uri:` followed by HD.2.
2. Otherwise, or when that form is no valid `url`, the endpoint is derived:
   `urn:ferrobridge:hl7v2-hd:` followed by HD.1 (the namespace ID), and, when
   HD.2 or HD.3 is valued, `:` HD.2 `:` HD.3. Every byte outside the RFC 3986
   `pchar` set is percent-encoded, and so is `:` inside a component, so
   `North Lab App` becomes `urn:ferrobridge:hl7v2-hd:North%20Lab%20App`.

HD.1, when valued, is also written to `source.name` or `destination.name`, so
the application name stays readable. No specification governs the derived
form: it is FerroBRIDGE's own design. The guide leaves an `HD` without a
typed universal ID to the implementer, a v2 application name often holds
spaces a `url` cannot carry, and the `ferrobridge` prefix marks the value as
derived so nobody reads it as an identifier the sender assigned. The same
`HD` always yields the same endpoint.

When MSH-3 and MSH-24 (Sending Network Address) are both empty and MSH-4
(Sending Facility) is valued, FerroBRIDGE builds `source.endpoint` and
`source.name` from the MSH-4 `HD` by the same two rules. The guide still
writes MSH-4 into `MessageHeader.sender` as an Organization, and that stays.
The destination side works the same way: with MSH-5 and MSH-25 (Receiving
Network Address) empty, MSH-6 (Receiving Facility) gives
`destination.endpoint` and `destination.name`. Each fallback is counted as a
`facility-endpoint` outcome naming the facility field and the element it
filled. The guide's MSH-3 and MSH-24 rows leave a message valuing neither to
the implementer, so the fallback is FerroBRIDGE's own design.

A message that names its sender in none of MSH-3, MSH-24 and MSH-4 is
mapped. The guide's MSH-24 row writes the `data-absent-reason` extension with
the code `unknown` into `source.endpoint` for it, and FHIR R4 JSON carries a
primitive with an extension and no value as the `_endpoint` property alone
(<https://hl7.org/fhir/R4/json.html#primitive>), which satisfies the required
endpoint. When MSH-4 is valued, its endpoint replaces the extension as above.
A run that still completes no `MessageHeader` is refused with an error naming
the element it lacks, so no header-less message Bundle is ever produced. A
message valuing MSH-24 is one: the guide's row into `source` has two
qualified `HD` data type maps and no single one to run.

## Version mismatch is refused at load time

The mapping version, the grammar version, the archetype revision, the template
`sem_ver`, and the profile version all have to agree. A mismatch is refused when
the mapping loads, not when a request arrives, so a bad configuration cannot sit
waiting for the first patient record to expose it.

## Programmed mappings are compiled in

`mappingCode` in FHIRconnect and `CustomMapping` in OMOCL name a function.
FerroBRIDGE registers those as named Rust functions at build time. There is no
runtime plugin loading, so what a deployment can execute is fixed by the binary
you audited.
