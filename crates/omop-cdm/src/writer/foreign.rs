// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The rows of a CDM table the bridge writes that no entry of its side table
//! owns: rows another ETL loaded into the same table.
//!
//! The writer takes every id from its own sequence per table, so such a row
//! can hold an id the writer assigns next, and a later commit then fails on
//! the primary key or, where the table has none, sits beside a row of the
//! bridge's. No specification governs this: our own design (the CDM leaves
//! key assignment to the ETL, <https://ohdsi.github.io/CommonDataModel/cdm54.html>).

use tokio_postgres::Client;

use crate::ddl::SchemaName;
use crate::graph;
use crate::meta::TableMeta;
use crate::writer::database;
use crate::writer::error::Step;
use crate::writer::error::WriteError;

/// The rows of one CDM table that no side table entry owns.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ForeignRows {
    /// The CDM table.
    pub table: &'static str,
    /// How many of its rows no side table entry owns.
    pub rows: u64,
}

/// Returns the side table predicate that owns a row of `table` whose
/// primary key column is `key`, read against the CDM row aliased `c`.
fn owned(bridge: &SchemaName, table: &str, key: &str) -> String {
    let record = format!(
        "EXISTS (SELECT 1 FROM {bridge}.record r WHERE r.cdm_table = '{table}' \
         AND r.surrogate_id = c.\"{key}\")"
    );
    let shared = match table {
        "person" => Some(format!("{bridge}.person_map WHERE person_id")),
        "visit_occurrence" => Some(format!("{bridge}.visit WHERE visit_occurrence_id")),
        "provider" => Some(format!("{bridge}.provider WHERE provider_id")),
        "visit_detail" => Some(format!("{bridge}.visit_detail WHERE visit_detail_id")),
        _ => None,
    };
    match shared {
        Some(shared) => format!("({record} OR EXISTS (SELECT 1 FROM {shared} = c.\"{key}\"))"),
        None => record,
    }
}

/// Returns whether the writer assigns the ids of `table`: a CDM table with a
/// primary key that the derivations do not rebuild.
fn surveyed(table: &TableMeta) -> bool {
    graph::row::cdm_table(table.name).is_ok()
        && !graph::row::DERIVED_TABLES.contains(&table.name)
        && graph::row::primary_key(table).is_some()
}

/// Counts, for every CDM table whose ids the writer assigns, the rows no side
/// table entry owns, and returns the tables that hold any.
pub(super) async fn count(
    client: &Client,
    cdm: &SchemaName,
    bridge: &SchemaName,
) -> Result<Vec<ForeignRows>, WriteError> {
    let mut foreign = Vec::new();
    for table in crate::generated::TABLES
        .iter()
        .filter(|table| surveyed(table))
    {
        let Some(key) = graph::row::primary_key(table) else {
            continue;
        };
        let statement = format!(
            "SELECT count(*) FROM {cdm}.{} c WHERE NOT {}",
            table.name,
            owned(bridge, table.name, key.name)
        );
        let row = client
            .query_one(&statement, &[])
            .await
            .map_err(database(Step::Survey(table.name)))?;
        let rows: i64 = row.try_get(0).map_err(database(Step::Survey(table.name)))?;
        if rows > 0 {
            foreign.push(ForeignRows {
                table: table.name,
                rows: rows.cast_unsigned(),
            });
        }
    }
    Ok(foreign)
}
