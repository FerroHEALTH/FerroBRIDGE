// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The natural-key side table: the id sequences, the id of every natural key,
//! the rows an earlier version wrote, and the links of a composition.

use std::collections::BTreeMap;
use std::collections::BTreeSet;

use tokio_postgres::Transaction;

use crate::ddl::SchemaName;
use crate::graph;
use crate::graph::RecordGraph;
use crate::graph::key::RecordKey;
use crate::meta::CdmSchema;
use crate::meta::TableMeta;
use crate::writer::database;
use crate::writer::error::Step;
use crate::writer::error::WriteError;
use crate::writer::person::mapped;
use crate::writer::quoted;
use crate::writer::stage::Staged;
use crate::writer::stage::copy_rows;
use crate::writer::stage::stage_row;

/// Returns the name of the sequence the ids of `table` come from.
pub(super) fn sequence(bridge: &SchemaName, table: &str) -> String {
    format!("{bridge}.{}", quoted(&format!("{table}_id_seq")))
}

/// Returns the next id of `table` from its sequence.
pub(super) async fn allocate(
    transaction: &Transaction<'_>,
    bridge: &SchemaName,
    table: &'static str,
) -> Result<i32, WriteError> {
    let row = transaction
        .query_one(
            &format!("SELECT nextval('{}')::integer", sequence(bridge, table)),
            &[],
        )
        .await
        .map_err(database(Step::Allocate(table)))?;
    row.try_get(0).map_err(database(Step::Allocate(table)))
}

/// The tables and sequences of the bridge schema.
///
/// Every statement leaves what already exists, and a schema an earlier
/// release created is brought to this shape: `record` loses the uniqueness of
/// `(cdm_table, surrogate_id)` for `person`, the one table several
/// compositions write the same row of.
pub(super) fn bridge_ddl(bridge: &SchemaName) -> String {
    let mut statements = vec![
        format!("CREATE SCHEMA IF NOT EXISTS {bridge}"),
        format!(
            "CREATE TABLE IF NOT EXISTS {bridge}.person_map (
                ehr_id text PRIMARY KEY,
                person_id integer NOT NULL UNIQUE)"
        ),
        format!(
            "CREATE TABLE IF NOT EXISTS {bridge}.record (
                versioned_object_uid text NOT NULL,
                archetype_root_path text NOT NULL,
                occurrence_path text NOT NULL,
                cdm_table text NOT NULL,
                ehr_id text NOT NULL,
                mapping text NOT NULL,
                entry integer NOT NULL,
                branch integer NOT NULL,
                surrogate_id integer NOT NULL,
                PRIMARY KEY (versioned_object_uid, archetype_root_path, occurrence_path,
                    mapping, entry, branch, cdm_table))"
        ),
        // NOTE: no specification governs this: our own design; every composition of a
        // person may write its PERSON row, so an id is unique in every other table only.
        format!(
            "ALTER TABLE {bridge}.record DROP CONSTRAINT IF EXISTS record_cdm_table_surrogate_id_key"
        ),
        format!(
            "CREATE UNIQUE INDEX IF NOT EXISTS record_surrogate_id
                ON {bridge}.record (cdm_table, surrogate_id) WHERE cdm_table <> 'person'"
        ),
        format!(
            "CREATE INDEX IF NOT EXISTS record_person_writer
                ON {bridge}.record (surrogate_id) WHERE cdm_table = 'person'"
        ),
        format!("CREATE SEQUENCE IF NOT EXISTS {bridge}.person_row_written_seq AS bigint"),
        format!(
            "CREATE TABLE IF NOT EXISTS {bridge}.person_row (
                versioned_object_uid text NOT NULL,
                person_id integer NOT NULL,
                written bigint NOT NULL,
                columns jsonb NOT NULL,
                PRIMARY KEY (versioned_object_uid, person_id))"
        ),
        format!(
            "CREATE INDEX IF NOT EXISTS person_row_newest
                ON {bridge}.person_row (person_id, written DESC)"
        ),
        format!(
            "CREATE TABLE IF NOT EXISTS {bridge}.link (
                versioned_object_uid text NOT NULL,
                domain_concept_id_1 integer NOT NULL,
                fact_id_1 integer NOT NULL,
                domain_concept_id_2 integer NOT NULL,
                fact_id_2 integer NOT NULL,
                relationship_concept_id integer NOT NULL)"
        ),
        format!(
            "CREATE INDEX IF NOT EXISTS link_versioned_object_uid
                ON {bridge}.link (versioned_object_uid)"
        ),
        format!(
            "CREATE TABLE IF NOT EXISTS {bridge}.visit (
                ehr_id text NOT NULL,
                visit_source text NOT NULL,
                visit_occurrence_id integer NOT NULL UNIQUE,
                PRIMARY KEY (ehr_id, visit_source))"
        ),
        format!(
            "CREATE TABLE IF NOT EXISTS {bridge}.watermark (
                versioned_object_uid text PRIMARY KEY,
                ehr_id text NOT NULL,
                version_uid text NOT NULL,
                run_id text NOT NULL,
                committed_at timestamptz NOT NULL)"
        ),
    ];
    for table in &crate::generated::TABLES {
        if graph::row::cdm_table(table.name).is_ok() && graph::row::primary_key(table).is_some() {
            statements.push(format!(
                "CREATE SEQUENCE IF NOT EXISTS {} AS integer",
                sequence(bridge, table.name)
            ));
        }
    }
    statements.join(";\n")
}

/// The id of every row of one graph, by table and natural key.
pub(super) type Ids<'g> = BTreeMap<(&'static str, &'g RecordKey), i32>;

/// Gives every row of `graph` the id its natural key held before, a person's
/// `person_id`, or a new id from its table's sequence.
pub(super) async fn assign_ids<'g>(
    transaction: &Transaction<'_>,
    bridge: &SchemaName,
    graph: &'g RecordGraph,
    earlier: &Earlier,
) -> Result<Ids<'g>, WriteError> {
    let mut ids = Ids::new();
    for row in graph.rows() {
        let table = row.table().name;
        let key = row.key();
        let id = if table == "person" {
            mapped(transaction, bridge, key.ehr_id(), true).await?
        } else if let Some(id) = earlier.ids.get(&EarlierKey::of(table, key)) {
            *id
        } else {
            allocate(transaction, bridge, table).await?
        };
        ids.insert((table, key), id);
    }
    Ok(ids)
}

/// One natural key as the side table holds it.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct EarlierKey {
    table: &'static str,
    root: String,
    occurrence: String,
    mapping: String,
    entry: i32,
    branch: i32,
}

impl EarlierKey {
    /// Returns the side table key of `key` in `table`.
    fn of(table: &'static str, key: &RecordKey) -> Self {
        let discriminator = key.discriminator();
        Self {
            table,
            root: key.archetype_root_path().as_str().to_owned(),
            occurrence: key.occurrence_path().as_str().to_owned(),
            mapping: discriminator.mapping().as_str().to_owned(),
            entry: i32::from(discriminator.entry()),
            branch: i32::from(discriminator.branch()),
        }
    }
}

/// The rows an earlier version of a composition wrote, from the side table.
pub(super) struct Earlier {
    /// The id of each natural key.
    ids: BTreeMap<EarlierKey, i32>,
}

/// Reads the side table rows of `versioned_object_uid`.
pub(super) async fn earlier_rows(
    transaction: &Transaction<'_>,
    bridge: &SchemaName,
    versioned_object_uid: &str,
) -> Result<Earlier, WriteError> {
    let rows = transaction
        .query(
            &format!(
                "SELECT cdm_table, archetype_root_path, occurrence_path, mapping, entry, branch,
                    surrogate_id
                 FROM {bridge}.record WHERE versioned_object_uid = $1"
            ),
            &[&versioned_object_uid],
        )
        .await
        .map_err(database(Step::Read))?;
    let mut ids = BTreeMap::new();
    for row in rows {
        let table: String = row.try_get(0).map_err(database(Step::Read))?;
        let root: String = row.try_get(1).map_err(database(Step::Read))?;
        let occurrence: String = row.try_get(2).map_err(database(Step::Read))?;
        let mapping: String = row.try_get(3).map_err(database(Step::Read))?;
        let entry: i32 = row.try_get(4).map_err(database(Step::Read))?;
        let branch: i32 = row.try_get(5).map_err(database(Step::Read))?;
        let id: i32 = row.try_get(6).map_err(database(Step::Read))?;
        let meta = graph::row::cdm_table(&table)
            .ok()
            .filter(|meta| graph::row::primary_key(meta).is_some())
            .ok_or_else(|| WriteError::SideTable {
                what: "a CDM table with an integer key",
                value: table.clone(),
            })?;
        ids.insert(
            EarlierKey {
                table: meta.name,
                root,
                occurrence,
                mapping,
                entry,
                branch,
            },
            id,
        );
    }
    Ok(Earlier { ids })
}

/// What [`delete_earlier`] deleted, and the persons it touched.
pub(super) struct Deleted {
    /// The rows deleted, by CDM table.
    pub(super) rows: BTreeMap<&'static str, u64>,
    /// The `person_id` of every deleted row, and of the `PERSON` row the
    /// composition wrote, whose `PERSON` row [`retire_persons`] may remove.
    pub(super) persons: BTreeSet<i32>,
    /// The `person_id` of every `PERSON` row the composition wrote, whose
    /// columns [`restore_persons`](crate::writer::person::restore_persons)
    /// takes from the newest writer that remains.
    pub(super) written: BTreeSet<i32>,
}

/// The column every row of a person names its person by.
const PERSON_ID: &str = "person_id";

/// Returns whether `table` holds rows of a person that keep the person's
/// `PERSON` row alive: a CDM table with a `person_id`, other than `PERSON`
/// and the derived tables a run rebuilds whole.
fn holds_rows_of_a_person(table: &TableMeta) -> bool {
    table.cdm_schema == CdmSchema::Cdm
        && table.name != "person"
        && !graph::row::DERIVED_TABLES.contains(&table.name)
        && table.column(PERSON_ID).is_some()
}

/// Deletes the CDM rows, the links and the side table rows an earlier
/// version of the composition wrote, and returns the rows deleted by CDM
/// table with the persons they belonged to.
///
/// The `PERSON` row is left in place: it belongs to the person, and
/// [`retire_persons`] decides it once the composition's new rows, if any,
/// are written.
pub(super) async fn delete_earlier(
    transaction: &Transaction<'_>,
    cdm: &SchemaName,
    bridge: &SchemaName,
    versioned_object_uid: &str,
    earlier: &Earlier,
) -> Result<Deleted, WriteError> {
    let mut deleted = BTreeMap::new();
    let mut persons = BTreeSet::new();
    let mut written = BTreeSet::new();
    let mut by_table: BTreeMap<&'static str, Vec<i32>> = BTreeMap::new();
    for (key, id) in &earlier.ids {
        by_table.entry(key.table).or_default().push(*id);
    }
    for (table, ids) in &by_table {
        if *table == "person" {
            persons.extend(ids.iter().copied());
            written.extend(ids.iter().copied());
            continue;
        }
        let meta = graph::row::cdm_table(table)?;
        let Some(key) = graph::row::primary_key(meta) else {
            return Err(WriteError::SideTable {
                what: "a table with an integer key",
                value: (*table).to_owned(),
            });
        };
        let statement = if meta.column(PERSON_ID).is_some() {
            format!(
                "DELETE FROM {cdm}.{} WHERE {} = ANY($1) RETURNING {}",
                quoted(table),
                quoted(key.name),
                quoted(PERSON_ID)
            )
        } else {
            format!(
                "DELETE FROM {cdm}.{} WHERE {} = ANY($1) RETURNING NULL::integer",
                quoted(table),
                quoted(key.name)
            )
        };
        let rows = transaction
            .query(&statement, &[ids])
            .await
            .map_err(database(Step::Delete(table)))?;
        for row in &rows {
            let person: Option<i32> = row.try_get(0).map_err(database(Step::Delete(table)))?;
            persons.extend(person);
        }
        deleted.insert(*table, u64::try_from(rows.len()).unwrap_or(u64::MAX));
    }
    let links = transaction
        .execute(
            &format!(
                "DELETE FROM {cdm}.fact_relationship AS f USING {bridge}.link AS l
                 WHERE l.versioned_object_uid = $1
                   AND f.domain_concept_id_1 = l.domain_concept_id_1
                   AND f.fact_id_1 = l.fact_id_1
                   AND f.domain_concept_id_2 = l.domain_concept_id_2
                   AND f.fact_id_2 = l.fact_id_2
                   AND f.relationship_concept_id = l.relationship_concept_id"
            ),
            &[&versioned_object_uid],
        )
        .await
        .map_err(database(Step::Delete("fact_relationship")))?;
    if links > 0 {
        deleted.insert("fact_relationship", links);
    }
    for table in ["link", "record", "person_row"] {
        transaction
            .execute(
                &format!("DELETE FROM {bridge}.{table} WHERE versioned_object_uid = $1"),
                &[&versioned_object_uid],
            )
            .await
            .map_err(database(Step::Record))?;
    }
    Ok(Deleted {
        rows: deleted,
        persons,
        written,
    })
}

/// Removes the `PERSON` row of each of `persons` that no composition writes
/// and no row of the person names any more, and returns how many it removed.
///
/// OMOP CDM v5.4 makes `person_id` a foreign key to `PERSON` in every
/// clinical table (<https://ohdsi.github.io/CommonDataModel/cdm54.html>), so
/// a `PERSON` row stays while any row of the person remains. The person map
/// keeps the EHR's `person_id` either way, so a later row of the EHR takes
/// the same id.
pub(super) async fn retire_persons(
    transaction: &Transaction<'_>,
    cdm: &SchemaName,
    bridge: &SchemaName,
    persons: &BTreeSet<i32>,
) -> Result<u64, WriteError> {
    if persons.is_empty() {
        return Ok(0);
    }
    let ids: Vec<i32> = persons.iter().copied().collect();
    // NOTE: no specification governs this: our own design; a person stays while a
    // composition writes its PERSON row or any row of the person remains.
    let mut kept = vec![format!(
        "NOT EXISTS (SELECT 1 FROM {bridge}.record AS r
            WHERE r.cdm_table = 'person' AND r.surrogate_id = p.person_id)"
    )];
    for table in &crate::generated::TABLES {
        if holds_rows_of_a_person(table) {
            kept.push(format!(
                "NOT EXISTS (SELECT 1 FROM {cdm}.{} AS t WHERE t.{person} = p.person_id)",
                quoted(table.name),
                person = quoted(PERSON_ID)
            ));
        }
    }
    transaction
        .execute(
            &format!(
                "DELETE FROM {cdm}.person AS p WHERE p.person_id = ANY($1) AND {}",
                kept.join(" AND ")
            ),
            &[&ids],
        )
        .await
        .map_err(database(Step::Delete("person")))
}

/// Deletes the `PERSON` rows `graph` writes before they are copied in again,
/// so the newest writer's columns replace a `PERSON` row a person kept, the
/// placeholder or the row another composition of the person wrote.
pub(super) async fn replace_persons(
    transaction: &Transaction<'_>,
    cdm: &SchemaName,
    ids: &Ids<'_>,
) -> Result<(), WriteError> {
    let persons: Vec<i32> = ids
        .iter()
        .filter(|((table, _), _)| *table == "person")
        .map(|(_, id)| *id)
        .collect();
    if persons.is_empty() {
        return Ok(());
    }
    transaction
        .execute(
            &format!("DELETE FROM {cdm}.person WHERE person_id = ANY($1)"),
            &[&persons],
        )
        .await
        .map_err(database(Step::Delete("person")))?;
    Ok(())
}

/// Records the natural key and id of every row of `graph`.
pub(super) async fn record_keys(
    transaction: &Transaction<'_>,
    bridge: &SchemaName,
    graph: &RecordGraph,
    ids: &BTreeMap<(&'static str, &RecordKey), i32>,
) -> Result<(), WriteError> {
    let mut columns: [Vec<&str>; 6] = Default::default();
    let mut surrogate = Vec::new();
    let mut entries: Vec<i32> = Vec::new();
    let mut branches: Vec<i32> = Vec::new();
    for row in graph.rows() {
        let key = row.key();
        let [vo, root, occurrence, table, ehr, mapping] = &mut columns;
        vo.push(key.versioned_object_uid().value());
        root.push(key.archetype_root_path().as_str());
        occurrence.push(key.occurrence_path().as_str());
        table.push(row.table().name);
        ehr.push(key.ehr_id().value());
        mapping.push(key.discriminator().mapping().as_str());
        entries.push(i32::from(key.discriminator().entry()));
        branches.push(i32::from(key.discriminator().branch()));
        let id =
            ids.get(&(row.table().name, key))
                .copied()
                .ok_or_else(|| WriteError::UnknownRow {
                    table: row.table().name,
                    key: Box::new(key.clone()),
                })?;
        surrogate.push(id);
    }
    let [vo, root, occurrence, table, ehr, mapping] = &columns;
    transaction
        .execute(
            &format!(
                "INSERT INTO {bridge}.record (versioned_object_uid, archetype_root_path,
                    occurrence_path, cdm_table, ehr_id, mapping, entry, branch, surrogate_id)
                 SELECT * FROM UNNEST($1::text[], $2::text[], $3::text[], $4::text[],
                    $5::text[], $6::text[], $7::integer[], $8::integer[], $9::integer[])"
            ),
            &[
                vo, root, occurrence, table, ehr, mapping, &entries, &branches, &surrogate,
            ],
        )
        .await
        .map_err(database(Step::Record))?;
    Ok(())
}

/// Writes both directions of every link of `graph` and returns the number of
/// `FACT_RELATIONSHIP` rows written.
pub(super) async fn write_links(
    transaction: &Transaction<'_>,
    cdm: &SchemaName,
    bridge: &SchemaName,
    graph: &RecordGraph,
    ids: &BTreeMap<(&'static str, &RecordKey), i32>,
) -> Result<u64, WriteError> {
    let table = graph::row::cdm_table("fact_relationship")?;
    let id_of = |end: &graph::link::LinkEnd| {
        ids.get(&(end.table().name, end.key()))
            .copied()
            .ok_or_else(|| WriteError::UnknownRow {
                table: end.table().name,
                key: Box::new(end.key().clone()),
            })
    };
    let mut rows = Vec::new();
    let mut side: [Vec<i32>; 5] = Default::default();
    for link in graph.links() {
        let first = (link.first().domain_concept_id(), id_of(link.first())?);
        let second = (link.second().domain_concept_id(), id_of(link.second())?);
        for (from, to) in [(first, second), (second, first)] {
            let fields = [
                from.0,
                from.1,
                to.0,
                to.1,
                graph::link::Link::RELATIONSHIP_CONCEPT_ID,
            ];
            for (column, value) in side.iter_mut().zip(fields) {
                column.push(value);
            }
            rows.push(stage_row(
                table,
                BTreeMap::from([
                    ("domain_concept_id_1", Staged::Integer(Some(from.0))),
                    ("fact_id_1", Staged::Integer(Some(from.1))),
                    ("domain_concept_id_2", Staged::Integer(Some(to.0))),
                    ("fact_id_2", Staged::Integer(Some(to.1))),
                    (
                        "relationship_concept_id",
                        Staged::Integer(Some(graph::link::Link::RELATIONSHIP_CONCEPT_ID)),
                    ),
                ]),
            ));
        }
    }
    let written = copy_rows(transaction, cdm, table, &rows).await?;
    let [d1, f1, d2, f2, relationship] = &side;
    transaction
        .execute(
            &format!(
                "INSERT INTO {bridge}.link (versioned_object_uid, domain_concept_id_1, fact_id_1,
                    domain_concept_id_2, fact_id_2, relationship_concept_id)
                 SELECT $1, * FROM UNNEST($2::integer[], $3::integer[], $4::integer[],
                    $5::integer[], $6::integer[])"
            ),
            &[
                &graph.source().versioned_object_uid().value(),
                d1,
                f1,
                d2,
                f2,
                relationship,
            ],
        )
        .await
        .map_err(database(Step::Record))?;
    Ok(written)
}
