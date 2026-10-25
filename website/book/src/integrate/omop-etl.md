<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# The OMOP ETL

The OMOP side of FerroBRIDGE turns openEHR content into rows in an OMOP Common
Data Model v5.4 database, driven by OMOCL mapping files. OMOP is a relational
analytics schema rather than an API, so the deliverable is populated tables.
The sink, the runner, the OMOCL engine and `ferrobridge cdm init` are built,
and `ferrobridge etl run` maps each composition with the files in
`[mappings] omocl` and, when the operator API is on, the files the console
saved under `[operator] mapping_dir/omocl/`. A provisioned file owns a path
both directories hold, and two files that define the same mapping refuse the
run.

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
EHR ([The PERSON row](#the-person-row)). VISIT_OCCURRENCE comes from a configured AQL query, checked when the
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

## The PERSON row

Every clinical table names its person through `person_id`, a required
foreign key to `PERSON` (CDM v5.4), so every `person_id` a run writes names a
`PERSON` row. Its columns come from the first of these that applies:

1. A `PERSON` row an OMOCL mapping writes. The OMOCL corpus maps the
   `openEHR-EHR-ADMIN_ENTRY.person_data.v0` archetype to `PERSON`
   (`person_data/person_data_v0.yml`); when one of the person's compositions
   carries it, its row replaces whatever row the person held, keeping the
   `person_id`. Several compositions of one EHR may write it; the one
   committed last rewrites the columns, and the row stays while any of them
   or any row of the person remains.
   The corpus file reads gender at `items[at0019]` of
   `openEHR-EHR-EVALUATION.gender.v1`, a node the KDS person template
   (`KDS_Person`) does not carry, so on KDS data every person takes
   `gender_concept_id` 0. For KDS data, use FerroBRIDGE's own file
   `crates/omocl/tests/fixtures/projects/ferrobridge/kds_person/kds_person_data.yml`
   in place of the corpus file: it reads the administrative gender at
   `items[at0022]`, resolves its code through the loaded vocabulary and keeps
   the code in `gender_source_value`.
2. Otherwise, `[cdm] person_policy`:
   - `create_on_first_sight`, the default, writes a placeholder row the first
     time a row or a visit names the EHR: `gender_concept_id`,
     `race_concept_id` and `ethnicity_concept_id` 0 (the CDM's concept for
     "no matching concept"), `year_of_birth` 0, and `person_source_value` the
     `ehr_id` when it fits the column's 50 characters.
   - `existing` refuses every row whose EHR has no `PERSON` row.

The CDM requires `year_of_birth`, and its ETL convention drops every person
with no year of birth. `existing` follows that convention. The placeholder's
0 is a value no person was born in: a year taken from the person's first
event would look real and corrupt every age computed from it, while 0 fails
the Data Quality Dashboard's plausible-value check for `year_of_birth`, so
every placeholder shows up there. The run report prints the `PERSON` rows
and the placeholders among them (`persons` in the JSON). Both policies are
FerroBRIDGE's own design.

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
a candidate. A visit query that names `$since` refuses the start, since the
parameter would reach the CDR unbound. A run whose visit query refused a row removes no visit, because
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

## The DEATH row

The OMOCL `Death` target writes DEATH, which the CDM gives no key of its own:
a person has at most one DEATH row, identified by `person_id`. When several
compositions of a person write one, the composition committed last gives the
row its columns. When that composition is removed, or a new version of it no
longer writes a death, the newest of the remaining writers rewrites the row,
and the row goes when none remains. This rule is FerroBRIDGE's own design:
the CDM leaves the choice among several death records to the ETL.

## Providers and visit details

An OMOCL mapping may write `provider_id` (under `ConditionOccurrence` and
`DrugExposure`) and `visit_detail_id` (under `ConditionOccurrence` and
`DeviceExposure`). The CDM keys PROVIDER and VISIT_DETAIL by integers no
composition carries, so the bridge keys them by the text the mapping's path
reads. No specification governs these keys: they are FerroBRIDGE's own design.

- **A provider** is keyed by that text alone. The first composition that
  names it creates a PROVIDER row with the text in `provider_source_value`
  (when it fits the column's 50 characters), and every later row that names
  the same text refers to that row.
- **A visit detail** is keyed by the composition's visit and the text. The
  first row that names it creates a VISIT_DETAIL row inside the visit, taking
  the person, `visit_detail_concept_id`, dates and type concept from the
  visit as it stands at that moment, with the text in
  `visit_detail_source_value`. A composition tied to no visit cannot name a
  visit detail, so such a row is refused and counted in the run report.

Both rows are created in the composition's own transaction, and the bridge
schema keeps their keys in `provider` and `visit_detail`. Neither row is
removed when a composition that named it is removed, because other
compositions may still name it.

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

The bridge carries a Rust port of the OHDSI Data Quality Dashboard's
conformance and completeness checks (v2.9.0, Blacketer et al., JAMIA 2021),
planned where the Dashboard's own settings enable them, with their
thresholds:

```sh
ferrobridge dq check                 # one line per failing check, then the counts
ferrobridge dq check --format json   # the Dashboard's result fields
```

The job reads the `[cdm]` section, runs every check over the CDM schema
(the vocabulary tables left out, as the Dashboard leaves them out by
default), and exits `1` when a check fails or cannot be evaluated. A check
of a missing or empty table, or of a missing or unpopulated field, is marked
not applicable with its reason, as the Dashboard marks it, and counts as
neither. The JSON document holds `CheckResults` (one object per check, in the
Dashboard's result columns), `Overview` (its counts) and the run's
timestamps. With `[etl] quality_checks = true`, `etl run` runs the same
checks over the tables it wrote and adds their verdicts to the run report; a
failed check never rolls a committed load back.

The plausibility checks are not ported. To run them, or the R tool's own
viewer, run the Dashboard itself as below.

### Running the R package

The Dashboard itself is an R package
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
