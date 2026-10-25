<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# The synthetic OHDSI vocabulary

Ten files, one per vocabulary table of the OMOP CDM v5.4
(<https://ohdsi.github.io/CommonDataModel/cdm54.html#Vocabulary_Tables>),
that the `omop-cdm` concept resolver tests load into a PostgreSQL built from
the vendored OHDSI DDL.

## Provenance

Every row was written by hand for the tests on 2026-09-25. No concept code,
concept name or vocabulary comes from an OHDSI vocabulary release or from any
licensed terminology. The vocabularies are `FB-SOURCE`, `FB-STANDARD`,
`FB-UNIT`, `FB-GENDER` and `FB-META`, plus the stand-ins `LOINC`, `UCUM`
and `SNOMED` described below, each with the version `synthetic-1`. The
domain identifiers are the CDM's own names (`Measurement`, `Condition`,
`Unit`, `Drug`, `Specimen`, `Meas Value Operator`, `Visit`, `Type Concept`,
`Gender`, `Metadata`) because the engine checks a resolved concept's domain
against the table a mapping writes, and the CDM field definitions name the
domains that way. The relationship identifiers `Maps to` and `Mapped from`
are the ones the CDM conventions name
(<https://ohdsi.github.io/CommonDataModel/dataModelConventions.html>).

## The file shape is an Athena download's

The files have the shape the Athena application writes a download in, read
from its source (`docs/specs/athena/PROVENANCE.md`): each is named after its
table in upper case (`CONCEPT.csv`) and carries a header row of the table's
column names, then rows of tab-separated, unquoted values in UTF-8, `\n` after
each row, `yyyyMMdd` dates and an empty field for NULL. A download carries the
nine tables Athena serves; `SOURCE_TO_CONCEPT_MAP.csv` is a site's own table,
which the loader takes when it is present. The tests load the directory with
the production loader, `omop_cdm::athena`, which `ferrobridge vocab load`
runs, so the fixture exercises the loader a deployment uses.

## What the rows cover

Each row isolates one rule of the resolver, so a few are shaped more simply
than a vocabulary release would ship them (a concept marked `U` with its
default end date, for example).

| Source key | What it exercises |
|---|---|
| `FB-STANDARD` `STD-MEAS` | a standard concept, returned as itself |
| `FB-SOURCE` `SRC-ONE` | one `Maps to` hop to one standard concept |
| `FB-SOURCE` `SRC-FAN` | one source concept mapped to two standard concepts, listed out of order |
| `FB-SOURCE` `SRC-DELETED` | a source concept with `invalid_reason` set |
| `FB-SOURCE` `SRC-STALE` | three mappings: a deleted relationship, an invalid target, one valid target |
| `FB-STANDARD` `STD-FUTURE`, `STD-PAST` | concepts whose validity dates exclude the record date |
| `FB-SOURCE` `SRC-LATER` | a relationship whose validity starts after the record date |
| `FB-SOURCE` `SRC-TWIN` | two valid concepts under one key |
| `FB-SOURCE` `SRC-SUCCEEDED` | two concepts under one key where only one is valid on the date |
| `FB-SOURCE` `SRC-NOMAP` | a valid source concept with no `Maps to` row |
| `FB-UNIT` `U-SYNTH` | a standard unit concept, with a multi-byte name |
| `LOINC` `SYN-LAB-1` | a standard laboratory measurement, as the laboratory compositions code it |
| `LOINC` `SYN-LAB-2` | a non-standard laboratory code with one `Maps to` hop to `STD-LAB-2` |
| `UCUM` `mmol/L`, `g/L` | the standard units of the laboratory quantities |
| `SNOMED` `SYN-SPEC-1` | a standard specimen type |
| `Meas Value Operator` `<`, `<=`, `=`, `>=`, `>` | the operator concepts the resolver finds by name |
| `FB-STANDARD` `STD-VISIT` | the `visit_concept_id` the round trip configures |
| `FB-STANDARD` `STD-TYPE-RECORD`, `STD-TYPE-PERIOD` | the record and observation period type concepts the round trip configures |
| `FB-GENDER` `SYN-GENDER-F` | a standard `Gender` concept, the administrative gender of the synthetic KDS person, reached through a configured vocabulary alias |
| `VOCABULARY` `None` | the release row whose `vocabulary_version` the loader reports, as a vocabulary build writes it |

The laboratory rows sit under the vocabulary identifiers `LOINC`, `UCUM` and
`SNOMED`, because the OMOCL engine looks a code up under the vocabulary its
terminology names. Every concept id, name and code there is invented, except
the two unit codes, which are the UCUM unit expressions the synthetic
compositions write; the `vocabulary` rows name each one a FerroBRIDGE
stand-in with the version `synthetic-1`. The vocabulary lists no `LOINC`
`SYN-LAB-9`, the code one synthetic laboratory composition carries, so that
analyte lands as concept `0`.

The remaining tables carry a few rows each so the loader reads every one of
them: the `FB-META` concepts give the vocabularies, domains, classes and
relationships the concept their table references.
