<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# The OMOP ETL

The OMOP side of FerroBRIDGE turns openEHR content into rows in an OMOP Common
Data Model v5.4 database, driven by OMOCL mapping files. OMOP is a relational
analytics schema rather than an API, so the deliverable is populated tables.
The sink, the runner, the OMOCL engine and `ferrobridge cdm init` are built,
and `ferrobridge etl run` maps each composition with the files in
`[mappings] omocl`.

<!-- toc -->

## A batch job, not a listener

The ETL runs as a job over AQL. It queries the CDR with `POST /query/aql`,
converts each EHR's compositions through the loaded OMOCL files, and writes
typed CDM rows. It pulls; the CDR does not push.

That follows from the specification surface rather than from taste: change
notification is not part of openEHR ITS-REST 1.1.0, so there is nothing
conformant to subscribe to. If FerroBRIDGE later consumes a particular CDR's
change events, that is a FerroBRIDGE extension, labelled as one, and the batch
path stays the conformant baseline.

## The tables in play

PERSON, OBSERVATION_PERIOD, VISIT_OCCURRENCE, VISIT_DETAIL,
CONDITION_OCCURRENCE, DRUG_EXPOSURE, PROCEDURE_OCCURRENCE, DEVICE_EXPOSURE,
MEASUREMENT, OBSERVATION, DEATH, SPECIMEN, FACT_RELATIONSHIP, NOTE, and
NOTE_NLP, beside the vocabulary tables.

Some of those come from mappings and some are derived. PERSON rows come per
EHR. VISIT_OCCURRENCE comes from a configured AQL query, checked when the
configuration loads, grouped by `ehr_id` and source. OBSERVATION_PERIOD is
derived from the first and last clinical event per person, and the ERA tables
run the SQL the CDM publishes. The CDM leaves each of these to the ETL's
discretion, so each is labelled as FerroBRIDGE's own.

An OMOCL record names its target table in `type`. The CDM says the domain of
the resolved standard concept decides the table, so FerroBRIDGE validates the
two against each other: a literal concept whose domain disagrees with `type`
refuses the mapping at load, and a resolved concept whose domain disagrees
refuses the record. A row is never moved to a table the mapping did not name.
A source code the vocabulary maps to several standard concepts in the record's
domain writes one row per concept, each with the same source value and source
concept, as the CDM conventions ask.

## The row types are generated

The CDM v5.4 row types are generated from the OHDSI `CommonDataModel` field
definitions, which publish the model in machine-readable form. The PostgreSQL
DDL is not regenerated: OHDSI renders it through a dialect layer that sits
outside those definitions, so the rendered files are vendored verbatim and
applied by `ferrobridge cdm init`, and a test asserts the generated columns
equal the DDL's. Nothing hand-transcribes a column list. The foreign keys of
OHDSI's constraints file are applied only by `cdm init --with-constraints`,
which PostgreSQL refuses at the pinned OHDSI tag; see
[What `cdm init` does](../operate/configuration.md#what-cdm-init-does).

Both clients the ETL opens to the CDM database, the concept resolver and the
writer, connect over the TLS the `sslmode` of `[cdm] url` names: `disable`,
`require`, `verify-ca` or `verify-full`, with the CA from `[cdm] tls_ca_file`.
The URL must name one; see [Configuring the server](../operate/configuration.md#cdm).

## Concept resolution is SQL, not terminology

A source code becomes a `concept_id` through the locally loaded OHDSI
vocabulary: the CONCEPT table, the `standard_concept` flag, the domain, and the
CONCEPT_RELATIONSHIP "Maps to" traversal. This is SQL over the vocabulary
tables in the same database as the CDM rows. It never goes to the FHIR
terminology server, which is a different component serving the FHIR side.

The lookup requires a valid concept (`invalid_reason` empty, the validity
dates containing the record date) and a deterministic order, and an ambiguous
match is an error rather than the first row. An unmapped code lands as
`concept_id = 0` with the source value kept in its source column and is counted
in the run report. The CDM defines `0` as "no matching concept", and keeping
the source value is what makes the gap auditable later.

Each composition's rows are written together or not at all, through binary
`COPY`, and a re-run of the same input replaces its rows rather than adding
duplicates.

## Keys, re-runs and watermarks

Every CDM v5.4 primary key is a 32-bit `integer`, so the bridge assigns ids
from one PostgreSQL sequence per table, kept in a schema of its own beside the
CDM (`[cdm] bridge_schema`, `ferrobridge` by default). A side table there maps
each row's natural key (the EHR, the versioned composition, the archetype root,
the occurrence, and the mapping entry that wrote it) to its id, so a later
version of a composition keeps the ids of the rows it still has and deletes
the ones it no longer has. One
`ehr_id` is one `PERSON`; the bridge does not reconcile a person across EHRs.
An exhausted sequence refuses the composition with a typed error.

The `PERSON` row belongs to the person, not to the composition whose mapping
wrote it. Every clinical table names its person through a `person_id` foreign
key to `PERSON` (CDM v5.4), so when a later version of that composition no
longer writes the person, or the composition is removed, the `PERSON` row
stays while any row of the person remains. It goes only when no composition
writes it and no row of the person is left; the person map keeps the EHR's
`person_id`, so a later row of the EHR takes the same id. The newest version
that writes the `PERSON` row rewrites its columns. This is FerroBRIDGE's own
design.

Each committed composition leaves a watermark naming the version written.
`ferrobridge etl run --resume` skips every composition whose watermark names
the version the query answers and runs the rest, so a run that stopped part
way through continues exactly. A composition the mapper or the database
refuses is rolled back, counted in the run report with the reason, and the
run goes on.

The run report counts, per mapping and in total, the rows written per table,
the `concept_id = 0` assignments, the `FACT_RELATIONSHIP` rows written with
`relationship_concept_id = 0`, the source fields no mapping wrote, and every
refused record with its table, column and openEHR element. It also counts the
deletion checks below and lists each composition they removed, kept or could
not find, by its versioned object id, never with a clinical value. `etl run`
prints it as text and as JSON.

## Deleted compositions

A deletion in openEHR is a new version whose data is Void (openEHR RM Common
1.1.0 §6.2.5), so the `LATEST_VERSION` query stops answering a deleted
composition and the run never reads it again. ITS-REST 1.1.0 has no change
feed, so after the compositions each run checks every composition an earlier
run committed and this run's query did not answer. It reads the
composition's latest version with `GET /ehr/{ehr_id}/composition/{versioned_object_uid}`
and acts on the answer:

| Answer | Meaning (`composition_get`) | What the run does |
|---|---|---|
| `204` | the composition is deleted | removes its rows, its `FACT_RELATIONSHIP` links and its watermark in one transaction (under `deletions = "remove"`), or lists it and keeps them (under `deletions = "report"`) |
| `200` | the composition exists, and the query left it out | keeps the rows and counts it as extant |
| `404` | the CDR knows no such EHR or composition | keeps the rows and lists it, because no deletion is confirmed |
| anything else | no documented answer | ends the run with an error naming the composition |

A composition missing from the query because the query changed is never
removed: only the CDR's `204` removes rows. The EHR keeps its `person_id`.
The checks run in batches of `page_size` watermarks with eight in flight, and
a removal takes the same per-composition lock a commit takes. The derived
tables are rebuilt after the removals, so the observation period and the eras
of the person no longer count the deleted rows, and the visits come from the
visit query, which no longer reads the deleted composition's context. A
visit the visit query no longer answers at all is removed after the
removals, so the observation period shrinks with it ([The visits the query
no longer answers](#the-visits-the-query-no-longer-answers)). A second run finds no watermark left for the composition and changes nothing.

The CDM defines no deletion, so the removal is FerroBRIDGE's own design.
With `--since`, the query answers only the changed compositions, so every
other composition is checked each run.

## The derived tables

`OBSERVATION_PERIOD` spans the first to the last clinical event of each
person, one period per person. The CDM's EHR guidance suggests more (an end
60 days after the last event, gaps from a persistence window); FerroBRIDGE
applies none of it, and says so.

`CONDITION_ERA` and `DRUG_ERA` run the two scripts on the CDM's SQL scripts
page. The page publishes them in OHDSI SQL for SqlRender, which PostgreSQL does
not run, so the bridge carries their PostgreSQL form, changed only in dialect,
and a test fails when the published scripts change. The drug era script reads
ingredients of the `RxNorm` vocabulary only, as published.

`VISIT_OCCURRENCE` comes from the `[etl.visits]` query, grouped by EHR and
source, from the earliest start to the latest end. A time with an offset keeps
its wall-clock time, because the CDM `datetime` has no zone. The same holds for
every date and time a mapping reads from a composition: the offset is not
normalised, and the row keeps the wall-clock time the source wrote. The visit concept
and the visit type concept are configuration with no default.

## The visits the query no longer answers

Each run writes the visits the `[etl.visits]` query answers before the
compositions. After the compositions and the deletion check, and before the
derived tables, it removes every visit an earlier run wrote that the query no
longer answers, with the bridge's record of its key, unless a CDM row still
names it through `visit_occurrence_id`. Such a visit stays, counted in the
run report as unanswered and kept, so no row names a missing visit; once the
rows that name it are written again without it, a later run removes it.

The visit query is never bound to `--since`, so its answer holds every visit
the deployment derives, and only a visit the whole query no longer answers is
a candidate. A run whose visit query refused a row removes no visit, because
the refused row may belong to a visit the query did answer. This is
FerroBRIDGE's own design.

## Tying a composition to its visit

The CDM asks every clinical event to carry its visit where one exists, and
leaves the tie to the ETL. No specification governs the rule below: it is
FerroBRIDGE's own design.

- A composition belongs to the visit of its own EHR whose window, from the
  visit's start to its end, contains the composition's `context/start_time`.
  Both bounds are inclusive.
- A composition without a context, a persistent one, is tied by the first date
  its mapping resolved on its first row, with that row's time when it has one.
- When several visits contain the moment, the visit whose source (the
  `visit_source` the `[etl.visits]` query selects) equals the name of the
  composition's `context/health_care_facility` wins. When no source matches,
  or no facility is recorded, the bridge does not guess: the composition's
  rows carry no visit.
- A composition inside no visit window carries no visit either.
- Times are compared as wall-clock times to the second, with the offset
  dropped, the same reading the visits get. Where one side carries a date
  and no time, the two dates are compared.

The run report counts the committed compositions tied to a visit, those
inside no visit, and those inside several that the facility did not decide.
A run without `[etl.visits]` ties nothing and counts nothing.

## The composition query

The `[etl] aql` query selects each composition whole beside its `ehr_id` and
`version_uid`, aliased by those names, with an `ORDER BY` and no `LIMIT`. It
may also select the `versioned_object_uid`; when it does, the bridge checks it
against the version, and when it does not, the bridge reads it from the
`version_uid`, whose first part names the versioned object (openEHR RM
Common 1.1.0, `OBJECT_VERSION_ID`).

Both queries read every `COMPOSITION` below `VERSION v[LATEST_VERSION]`, and
the bridge refuses to load a query that reads one without that scope. A
deleted composition's newest version holds no data (openEHR RM Common 1.1.0
§6.2.5), so that scope leaves it out. A superseded version neither feeds a
mapping nor widens a visit, so a changed composition is written once. AQL
1.1.0 names `LATEST_VERSION` and `ALL_VERSIONS` and does not say which scope
a query without a `VERSION` gets, so the scope is written out:

```text
SELECT e/ehr_id/value AS ehr_id, v/uid/value AS version_uid, c AS composition
FROM EHR e CONTAINS VERSION v[LATEST_VERSION] CONTAINS COMPOSITION c
ORDER BY c/context/start_time/value
```

The query selects the composition whole, and the mappings read its leaves.
Leave leaf paths through a repeated node, such as two values of one
`events` list, out of the `SELECT`: AQL 1.1.0 does not define which node
each of them comes from, and a CDR may pair values of two different nodes in
one row.

## Checking a populated database with the Data Quality Dashboard

The OHDSI Data Quality Dashboard runs its checks against a populated CDM
outside CI; the round trip test in CI does not run it. It is an R package
(<https://ohdsi.github.io/DataQualityDashboard/>), with no container image of
its own, so you run it from any R session that can reach the database:

1. Populate the database: `ferrobridge cdm init`, load the vocabulary, then
   `ferrobridge etl run`. The CDM tables are in `[cdm] schema` (`cdm` by
   default); the bridge's own tables are in `[cdm] bridge_schema` and are not
   part of the CDM, so the dashboard is never pointed at them.
2. Create a schema for the results, for example `CREATE SCHEMA dqd_results;`.
3. In R, install the package and the PostgreSQL driver:

   ```r
   install.packages("DataQualityDashboard")
   DatabaseConnector::downloadJdbcDrivers("postgresql", pathToDriver = "~/jdbc")
   ```

4. Connect and run the checks against CDM v5.4:

   ```r
   connectionDetails <- DatabaseConnector::createConnectionDetails(
     dbms = "postgresql",
     server = "localhost/ferrobridge",
     port = 5432,
     user = "ferrobridge",
     password = Sys.getenv("CDM_PASSWORD"),
     pathToDriver = "~/jdbc"
   )
   DataQualityDashboard::executeDqChecks(
     connectionDetails = connectionDetails,
     cdmDatabaseSchema = "cdm",
     resultsDatabaseSchema = "dqd_results",
     cdmSourceName = "FerroBRIDGE",
     cdmVersion = "5.4",
     outputFolder = "dqd-output",
     writeToTable = TRUE
   )
   ```

   The `server` argument is `host/database`, as `DatabaseConnector` documents
   for PostgreSQL.
5. Open the result with `DataQualityDashboard::viewDqDashboard()` on the JSON
   file the run writes into `dqd-output`, and record every failing check with
   its adjudication.

## Validating OMOCL files

OMOCL publishes no JSON schema, so FerroBRIDGE authors one from the grammar
tables and the published mapping library, validates every file against it
before running anything, and offers that schema upstream. The grammar also
leaves the reading of `alternatives` open; FerroBRIDGE tries them in order and
the first present value wins, which is the only reading the published library
is consistent with. A `CustomMapping` names a converter compiled into the
binary, and an unknown name refuses the file. A mapping that fails validation
is refused at load time.

## What you need in place

A PostgreSQL CDM v5.4 database built from the OHDSI DDLs, an Athena vocabulary
export loaded into it, a CDR to read from, and the OMOCL files for the
archetypes you care about. The [deployment shape](../operate/deployment-shape.md)
page lists the neighbours in full.
