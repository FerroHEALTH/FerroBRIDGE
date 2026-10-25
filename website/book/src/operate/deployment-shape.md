<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# What FerroBRIDGE runs beside

This page describes the shape of a deployment: which systems the bridge talks
to, what it stores, and what you have to supply. Read it to plan, then install
the image or a release binary as [The container](container.md) describes.

<!-- toc -->

## One process, four neighbours

FerroBRIDGE is its own process and its own image. It is not a crate inside a
CDR, not a plugin, and not a library a CDR links. It reaches every neighbour
over a network protocol, which is what lets it work against a CDR it did not
build.

| Neighbour | Protocol | Needed by |
|---|---|---|
| An openEHR CDR | openEHR ITS-REST 1.1.0 | both sides |
| A FHIR terminology server | FHIR terminology operations, R4 or R4B | the FHIR side |
| An OMOP CDM v5.4 database | SQL, PostgreSQL | the OMOP side |
| The OHDSI vocabulary, loaded into that database | SQL | the OMOP side |

You run each of these, or you point FerroBRIDGE at ones you already run. None
of them is a compile-time dependency, and the bridge treats any conformant
implementation the same way.

## What FerroBRIDGE stores

Clinical data lives in the CDR and, on the OMOP side, in the CDM database.
FerroBRIDGE stores two things of its own: the id-map, a persistent store
holding the patient identifier to `ehr_id` relation, the external to internal
resource id relation and the resource id to composition relation; and, on the
OMOP side, a side table in its own schema beside the CDM that maps each source
record to its rows, which is what makes a re-run replace rather than
duplicate. Everything else it holds is configuration: the loaded
mapping files, the terminology server address, and the CDR address.

## What it asks of the CDR

The FHIR side uses the composition and EHR endpoints:
`POST /ehr` and `GET /ehr?subject_id=&subject_namespace=`,
`POST`, `PUT`, and `GET` on `/ehr/{ehr_id}/composition[/{uid}]`, and
`POST /ehr/{ehr_id}/contribution` for an atomic multi-object commit. Both sides
use `POST /query/aql`. Templates come from
`GET /definition/template/adl1.4/{template_id}` as canonical XML; FerroBRIDGE
builds the Web Template from that itself, so the CDR need not serve one.
Compositions cross the wire as canonical JSON.

A CDR that does not serve the operational template cannot drive the bridge,
because a mapping path alone does not carry the leaf RM type.

## What it asks of the terminology server

Three operations: `CodeSystem/$lookup`, `ConceptMap/$translate`, and
`ValueSet/$validate-code`. `$lookup` matters most, because openEHR's
`DV_CODED_TEXT.value` is mandatory while FHIR's `display` is not, so a display
often has to be resolved rather than copied. The client tolerates a server
answering R4 or R4B.

## What the OMOP side asks of the database

A CDM v5.4 schema built from the OHDSI DDLs, and an Athena vocabulary download
loaded into the vocabulary tables. Concept resolution is SQL over CONCEPT,
`standard_concept`, the domain, and the CONCEPT_RELATIONSHIP "Maps to"
traversal. Without a loaded vocabulary every source code resolves to
`concept_id = 0`, which is legal in the CDM and useless for analysis.

### Loading the vocabulary

You download the vocabulary from <https://athena.ohdsi.org> under your own
account, unpack it, and load the directory:

```sh
ferrobridge vocab load /srv/athena/2026-10 --schema cdm
```

The job reads the `[cdm]` section for the connection and loads into the
schema `--schema` names, which is normally `[cdm] schema`. It needs the nine
files every download carries (`CONCEPT.csv`, `VOCABULARY.csv`, `DOMAIN.csv`,
`CONCEPT_CLASS.csv`, `CONCEPT_RELATIONSHIP.csv`, `RELATIONSHIP.csv`,
`CONCEPT_SYNONYM.csv`, `CONCEPT_ANCESTOR.csv`, `DRUG_STRENGTH.csv`) and loads
`SOURCE_TO_CONCEPT_MAP.csv` when you put one beside them. Every other file is
named in the output and left alone. Each table is replaced in a transaction of
its own, so a refused table keeps its rows and the tables before it keep their
new ones; the job prints the rows of each table and the vocabulary version.

Load the vocabulary before `cdm init --with-constraints`: PostgreSQL refuses
to `TRUNCATE` a table another table's foreign key references.

The format the loader reads is the one the Athena application writes, read
from its source at a pinned commit: tab-separated, unquoted, UTF-8, with a
header row and `yyyyMMdd` dates. OHDSI publishes no format document, so a
download that the loader refuses for its shape is worth reporting on issue
#88.

### Vocabulary licences

The vocabulary is a licensed deployment input you obtain and accept yourself;
FerroBRIDGE ships none of it, in the image, a release asset or a test.

- Athena offers some vocabularies only to an account whose licence request
  their owner has granted. The terms are the owner's, and you accept them
  when you request the vocabulary.
- CPT4 arrives as `CONCEPT_CPT4.csv` with every `concept_name` empty. The
  loader leaves that file alone, so CPT4 concepts are absent until you
  produce their names under your own licence and load them into `CONCEPT`.
- Check the licence of every vocabulary you select before you load it into a
  database you share, and record which ones the database holds.

### One writer per CDM table

FerroBRIDGE assumes it owns the id space of every CDM table it writes. It
takes each primary key from a sequence of its own per table, kept in the
bridge schema, and records which source record holds which id. Another ETL
that loads rows into the same tables can pick ids the bridge assigns next, and
the bridge's commit then fails on the primary key. Run one bridge per set of
CDM tables, or load other sources into a separate CDM schema.

`cdm init` and every `etl run` count the rows of those tables that no entry of
the bridge's side table owns, and log a warning naming each table and its
count. Treat a warning as a sign that something else writes the tables.

After a run the bridge rebuilds `OBSERVATION_PERIOD`, `CONDITION_ERA` and
`DRUG_ERA` for the persons it writes, the persons its person map names: their
derived rows are deleted and derived again, and every other person's derived
rows stay as another ETL wrote them. With `[etl] derived_tables =
"whole_table"` it rebuilds each table whole from every row in the schema, for a
deployment where the bridge is the only writer. The
era scripts the CDM publishes number eras by `person_id` alone, so two eras of
one person can swap ids between rebuilds. A re-run writes the same rows, but
an era id is not stable across rebuilds; never store one outside the database.

## Process model

No specification governs the process model, the storage mechanics, or the
deployment topology: that is FerroBRIDGE's own design. The decisions on record
are a single binary, a container image, and a batch ETL that runs as a job
rather than as a listener. Change notification is not part of ITS-REST 1.1.0,
so an incremental OMOP mode would be a FerroBRIDGE extension, labelled as one,
with the batch path staying the conformant baseline.
