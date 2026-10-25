// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The staging of a graph's rows and their binary `COPY` into PostgreSQL.

use std::collections::BTreeMap;

use tokio_postgres::Transaction;
use tokio_postgres::binary_copy::BinaryCopyInWriter;
use tokio_postgres::types::ToSql;
use tokio_postgres::types::Type;

use crate::ddl::SchemaName;
use crate::graph;
use crate::graph::RecordGraph;
use crate::graph::report::Report;
use crate::graph::row::Cell;
use crate::graph::row::Value;
use crate::meta::ColumnMeta;
use crate::meta::TableMeta;
use crate::writer::Resolver;
use crate::writer::database;
use crate::writer::error::Step;
use crate::writer::error::WriteError;
use crate::writer::quoted;

/// One staged column value: binary `integer`, or text PostgreSQL casts.
#[derive(Debug, Clone, PartialEq)]
pub(super) enum Staged {
    /// An `integer` column.
    Integer(Option<i32>),
    /// Any other column, as text.
    Text(Option<String>),
}

impl Staged {
    /// Returns the value as a `COPY` field.
    fn as_sql(&self) -> &(dyn ToSql + Sync) {
        match self {
            Self::Integer(value) => value,
            Self::Text(value) => value,
        }
    }
}

/// Returns whether `column` stages as a binary `integer`.
pub(super) fn is_integer(column: &ColumnMeta) -> bool {
    column.cdm_datatype == "integer"
}

/// The staging column that carries each row's date shift in days.
const SHIFT: &str = "\"ferrobridge_shift\"";

/// Returns the date part `column` of `PERSON` holds, when it is one of the
/// three birth columns.
fn birth_part(column: &str) -> Option<&'static str> {
    match column {
        "year_of_birth" => Some("year"),
        "month_of_birth" => Some("month"),
        "day_of_birth" => Some("day"),
        _ => None,
    }
}

/// Returns the SQL that moves the staged `column` of `table` into the CDM
/// column, every date and datetime moved by the row's shift.
///
/// The three birth columns of `PERSON` follow the shift when all three are
/// known, so they stay the parts of the shifted birth date.
pub(super) fn cast(table: &TableMeta, column: &ColumnMeta) -> String {
    let name = quoted(column.name);
    if let (true, Some(part)) = (table.name == "person", birth_part(column.name)) {
        return format!(
            "CASE WHEN {SHIFT} <> 0 AND \"year_of_birth\" > 0 \
             AND \"month_of_birth\" IS NOT NULL AND \"day_of_birth\" IS NOT NULL \
             THEN extract({part} FROM make_date(\"year_of_birth\", \"month_of_birth\", \
             \"day_of_birth\") + {SHIFT})::integer ELSE {name} END"
        );
    }
    match column.cdm_datatype {
        "date" => format!("{name}::date + {SHIFT}"),
        "datetime" => format!("{name}::timestamp + make_interval(days => {SHIFT})"),
        "float" => format!("{name}::numeric"),
        _ => name,
    }
}

/// Returns the staged form of `value`.
pub(super) fn staged(value: &Value) -> Staged {
    match value {
        Value::Integer(number) => Staged::Integer(Some(*number)),
        // NOTE: Rust documentation, `impl Display for f64`: the output is the
        // shortest decimal that round-trips, so PostgreSQL's numeric reads it exactly.
        Value::Float(number) => Staged::Text(Some(number.to_string())),
        Value::Text(text) => Staged::Text(Some(text.clone())),
        Value::Date(date) => Staged::Text(Some(date.as_str().to_owned())),
        Value::Datetime(datetime) => Staged::Text(Some(datetime.as_str().to_owned())),
    }
}

/// One staged row: its values in column order, and the days its dates move.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct StagedRow {
    values: Vec<Staged>,
    shift: i32,
}

impl StagedRow {
    /// Returns the row with its dates moved by `days`.
    pub(super) fn shifted(self, days: i32) -> Self {
        Self {
            shift: days,
            ..self
        }
    }
}

/// Returns one staged row of `table`, in column order, with every column
/// `values` does not name set to NULL and its dates unmoved.
pub(super) fn stage_row(
    table: &TableMeta,
    mut values: BTreeMap<&'static str, Staged>,
) -> StagedRow {
    let values = table
        .columns
        .iter()
        .map(|column| {
            values.remove(column.name).unwrap_or(if is_integer(column) {
                Staged::Integer(None)
            } else {
                Staged::Text(None)
            })
        })
        .collect();
    StagedRow { values, shift: 0 }
}

/// Copies `rows` of `table` into the CDM through a staging table.
pub(super) async fn copy_rows(
    transaction: &Transaction<'_>,
    cdm: &SchemaName,
    table: &'static TableMeta,
    rows: &[StagedRow],
) -> Result<u64, WriteError> {
    if rows.is_empty() {
        return Ok(0);
    }
    let stage = quoted(&format!("ferrobridge_stage_{}", table.name));
    let definition = table
        .columns
        .iter()
        .map(|column| {
            let kind = if is_integer(column) {
                "integer"
            } else {
                "text"
            };
            format!("{} {kind}", quoted(column.name))
        })
        .chain([format!("{SHIFT} integer")])
        .collect::<Vec<_>>()
        .join(", ");
    let names = table
        .columns
        .iter()
        .map(|column| quoted(column.name))
        .collect::<Vec<_>>()
        .join(", ");
    let casts = table
        .columns
        .iter()
        .map(|column| cast(table, column))
        .collect::<Vec<_>>()
        .join(", ");
    let types: Vec<Type> = table
        .columns
        .iter()
        .map(|column| {
            if is_integer(column) {
                Type::INT4
            } else {
                Type::TEXT
            }
        })
        .chain([Type::INT4])
        .collect();
    let step = Step::Copy(table.name);
    transaction
        .batch_execute(&format!(
            "CREATE TEMP TABLE IF NOT EXISTS {stage} ({definition}) ON COMMIT DELETE ROWS"
        ))
        .await
        .map_err(database(step))?;
    let sink = transaction
        .copy_in(&format!(
            "COPY pg_temp.{stage} ({names}, {SHIFT}) FROM STDIN (FORMAT binary)"
        ))
        .await
        .map_err(database(step))?;
    let writer = BinaryCopyInWriter::new(sink, &types);
    let mut writer = std::pin::pin!(writer);
    for row in rows {
        let mut fields: Vec<&(dyn ToSql + Sync)> = row.values.iter().map(Staged::as_sql).collect();
        fields.push(&row.shift);
        writer
            .as_mut()
            .write(&fields)
            .await
            .map_err(database(step))?;
    }
    writer.as_mut().finish().await.map_err(database(step))?;
    let inserted = transaction
        .execute(
            &format!(
                "INSERT INTO {cdm}.{} ({names}) SELECT {casts} FROM pg_temp.{stage}",
                quoted(table.name)
            ),
            &[],
        )
        .await
        .map_err(database(Step::Insert(table.name)))?;
    transaction
        .batch_execute(&format!("TRUNCATE pg_temp.{stage}"))
        .await
        .map_err(database(Step::Insert(table.name)))?;
    Ok(inserted)
}

/// The staged rows of one graph, by table.
pub(super) type Tables = BTreeMap<&'static str, (&'static TableMeta, Vec<StagedRow>)>;

/// Stages every row of `graph` with its references resolved, recording what
/// each mapping writes into `report`.
pub(super) async fn stage_graph(
    resolver: &mut Resolver<'_, '_>,
    graph: &RecordGraph,
    report: &mut Report,
) -> Result<Tables, WriteError> {
    let mut tables = Tables::new();
    let pseudonymiser = resolver.pseudonymiser;
    let shift = pseudonymiser.map_or(0, |p| p.shift(graph.source().ehr_id().value()));
    for row in graph.rows() {
        let table = row.table();
        let key_column = graph::row::primary_key(table);
        if key_column.is_none() && !graph::row::person_keyed(table) {
            return Err(WriteError::SideTable {
                what: "a table with an integer key or keyed by its person",
                value: table.name.to_owned(),
            });
        }
        let id = resolver
            .ids
            .get(&(table.name, row.key()))
            .copied()
            .ok_or_else(|| WriteError::UnknownRow {
                table: table.name,
                key: Box::new(row.key().clone()),
            })?;
        // NOTE: no specification governs this: our own design; a person-keyed row has no key
        // column, and its person_id cell names the person its id is.
        let mut values: BTreeMap<&'static str, Staged> = key_column
            .map(|key| (key.name, Staged::Integer(Some(id))))
            .into_iter()
            .collect();
        let mut concept_zero: BTreeMap<&'static str, u64> = BTreeMap::new();
        for (column, cell) in row.cells() {
            let meta = table.column(column);
            let value = match cell {
                Cell::Value(value) => {
                    let concept =
                        meta.and_then(|meta| meta.foreign_key) == Some(("concept", "concept_id"));
                    if concept && matches!(value, Value::Integer(0)) {
                        concept_zero.insert(column, 1);
                    }
                    match (pseudonymiser, meta) {
                        (Some(p), Some(meta)) => p.staged(table, meta, staged(value)),
                        _ => staged(value),
                    }
                }
                Cell::Reference(reference) => {
                    let target = meta
                        .and_then(|meta| meta.foreign_key)
                        .map_or(table.name, |(target, _)| target);
                    Staged::Integer(Some(resolver.resolve(reference, target).await?))
                }
            };
            values.insert(column, value);
        }
        report.record_written(row.mapping(), table.name, 1, &concept_zero);
        tables
            .entry(table.name)
            .or_insert_with(|| (table, Vec::new()))
            .1
            .push(stage_row(table, values).shifted(shift));
    }
    Ok(tables)
}
