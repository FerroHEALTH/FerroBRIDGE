// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The PostgreSQL query of each check.
//!
//! Each query is a translation of the Dashboard's SQL Server template for the
//! check (`docs/specs/dqd/inst/sql/sql_server/`, named by
//! [`CheckName::sql_file`](crate::quality::check::CheckName::sql_file)); no
//! specification governs this translation: our own design. Every query
//! returns one row of two `bigint` columns, `num_violated_rows` and
//! `num_denominator_rows`, and the fraction is computed from them in Rust.
//! The cohort branches of the templates are left out.
//!
//! Tables are named unqualified, for a `CdmPool` whose `search_path` is the
//! one CDM schema that also holds the vocabulary. Every identifier and literal
//! comes from the generated metadata, never from a caller, and every
//! identifier is double-quoted.

use crate::meta::ColumnMeta;
use crate::quality::check::{Check, Kind};

/// The alias of the checked table, as the templates spell it.
const CDM_TABLE: &str = "cdm_table";

impl Check {
    /// Returns the PostgreSQL query that counts this check's violating and
    /// denominator rows.
    #[must_use]
    pub fn sql(&self) -> String {
        let table = ident(self.table().name);
        let all_rows = format!("SELECT count(*) FROM {table} AS {CDM_TABLE}");
        match self.kind() {
            // NOTE: DQD v2.9.0 `inst/sql/sql_server/table_cdm_table.sql` lines 14
            // to 38; the probe reads the table and reports 0 of 1.
            Kind::CdmTable => format!(
                "SELECT CASE WHEN count(*) = 0 THEN 0 ELSE 0 END::bigint AS num_violated_rows, \
                 1::bigint AS num_denominator_rows FROM {table} AS {CDM_TABLE}"
            ),
            // NOTE: DQD v2.9.0 `inst/sql/sql_server/field_cdm_field.sql` lines 15
            // to 34; the probe reads the column and reports 0 of 1.
            Kind::CdmField(column) => format!(
                "SELECT CASE WHEN count({}) = 0 THEN 0 ELSE 0 END::bigint AS num_violated_rows, \
                 1::bigint AS num_denominator_rows FROM {table} AS {CDM_TABLE}",
                field(column)
            ),
            // NOTE: DQD v2.9.0 `field_is_not_nullable.sql` lines 19 to 54 and
            // `field_measure_value_completeness.sql` lines 18 to 55: a NULL, over every row.
            Kind::IsRequired(column) | Kind::MeasureValueCompleteness(column) => counts(
                &format!("{all_rows} WHERE {} IS NULL", field(column)),
                &all_rows,
            ),
            // NOTE: DQD v2.9.0 `inst/sql/sql_server/field_cdm_datatype.sql` lines
            // 33 to 37: a non-null value whose text is not an optionally signed digit run.
            Kind::CdmDatatype(column) => {
                let field = field(column);
                counts(
                    &format!(
                        "{all_rows} WHERE {field} IS NOT NULL AND {field}::text !~ '^[-+]?[0-9]+$'"
                    ),
                    &all_rows,
                )
            }
            // NOTE: DQD v2.9.0 `inst/sql/sql_server/field_is_primary_key.sql` lines
            // 33 to 48: every row whose value occurs more than once; NULL never matches IN.
            Kind::IsPrimaryKey(column) => {
                let name = ident(column.name);
                counts(
                    &format!(
                        "{all_rows} WHERE {} IN (SELECT {name} FROM {table} \
                         GROUP BY {name} HAVING count(*) > 1)",
                        field(column)
                    ),
                    &all_rows,
                )
            }
            // NOTE: DQD v2.9.0 `inst/sql/sql_server/is_foreign_key.sql` lines 33 to
            // 48: a non-null value with no row in the referenced table.
            Kind::IsForeignKey {
                column,
                table: fk_table,
                field: fk_field,
            } => {
                let field = field(column);
                let fk_field = format!("fk_table.{}", ident(fk_field));
                counts(
                    &format!(
                        "{all_rows} LEFT JOIN {} AS fk_table ON {field} = {fk_field} \
                         WHERE {fk_field} IS NULL AND {field} IS NOT NULL",
                        ident(fk_table)
                    ),
                    &all_rows,
                )
            }
            // NOTE: DQD v2.9.0 `inst/sql/sql_server/field_fk_domain.sql` lines 32 to
            // 44: a concept other than 0 outside the domains; NULL finds no concept.
            Kind::FkDomain { column, domains } => {
                let domains = domains
                    .iter()
                    .map(|domain| literal(domain))
                    .collect::<Vec<_>>()
                    .join(", ");
                counts(
                    &format!(
                        "{all_rows} {} AND co.\"domain_id\" NOT IN ({domains})",
                        concept_join(column)
                    ),
                    &all_rows,
                )
            }
            // NOTE: DQD v2.9.0 `inst/sql/sql_server/field_fk_class.sql` lines 33 to
            // 46: a concept other than 0 of another class; NULL finds no concept.
            Kind::FkClass { column, class } => counts(
                &format!(
                    "{all_rows} {} AND co.\"concept_class_id\" != {}",
                    concept_join(column),
                    literal(class)
                ),
                &all_rows,
            ),
            // NOTE: DQD v2.9.0 `field_is_standard_valid_concept.sql` lines 32 to 63:
            // a non-standard or invalid concept other than 0, of the non-null rows.
            Kind::IsStandardValidConcept(column) => counts(
                &format!(
                    "{all_rows} {} AND (co.\"standard_concept\" IS NULL \
                     OR co.\"standard_concept\" != 'S' OR co.\"invalid_reason\" IS NOT NULL)",
                    concept_join(column)
                ),
                &format!("{all_rows} WHERE {} IS NOT NULL", field(column)),
            ),
            Kind::MeasurePersonCompleteness => person_completeness(&table),
            Kind::MeasureConditionEraCompleteness => condition_era_completeness(&table),
            Kind::StandardConceptRecordCompleteness {
                column,
                source_value,
            }
            | Kind::SourceConceptRecordCompleteness {
                column,
                source_value,
            } => concept_record_completeness(&all_rows, column, *source_value),
            Kind::SourceValueCompleteness { column, standard } => {
                source_value_completeness(&table, column, standard)
            }
        }
    }
}

/// Returns the `measurePersonCompleteness` query of the quoted `table`.
fn person_completeness(table: &str) -> String {
    // NOTE: DQD v2.9.0 `table_person_completeness.sql` lines 18 to 56: a person
    // with no row in the table, over every person.
    counts(
        &format!(
            "SELECT count(*) FROM \"person\" AS {CDM_TABLE} LEFT JOIN {table} AS cdm_table2 \
             ON {CDM_TABLE}.\"person_id\" = cdm_table2.\"person_id\" \
             WHERE cdm_table2.\"person_id\" IS NULL"
        ),
        &format!("SELECT count(*) FROM \"person\" AS {CDM_TABLE}"),
    )
}

/// Returns the `measureConditionEraCompleteness` query of the quoted `table`.
fn condition_era_completeness(table: &str) -> String {
    // NOTE: DQD v2.9.0 `table_condition_era_completeness.sql` lines 17 to 57: a
    // person with a condition other than concept 0 and no era, over those persons.
    counts(
        &format!(
            "SELECT count(*) FROM (SELECT DISTINCT co.\"person_id\" \
             FROM \"condition_occurrence\" AS co LEFT JOIN {table} AS {CDM_TABLE} \
             ON co.\"person_id\" = {CDM_TABLE}.\"person_id\" \
             WHERE {CDM_TABLE}.\"person_id\" IS NULL \
             AND co.\"condition_concept_id\" != 0) AS violated_rows"
        ),
        "SELECT count(DISTINCT co.\"person_id\") FROM \"condition_occurrence\" AS co \
         WHERE co.\"condition_concept_id\" != 0",
    )
}

/// Returns the concept record completeness query of `column`, with the
/// source value column the template names beside it.
fn concept_record_completeness(
    all_rows: &str,
    column: &ColumnMeta,
    source_value: Option<&ColumnMeta>,
) -> String {
    // NOTE: DQD v2.9.0 `field_concept_record_completeness.sql` lines 17 to 95:
    // concept 0, or a NULL beside a source value the template names, over every row.
    let field = field(column);
    let beside = source_value.map_or_else(String::new, |source| {
        format!(
            " OR ({field} IS NULL AND {CDM_TABLE}.{} IS NOT NULL)",
            ident(source.name)
        )
    });
    counts(&format!("{all_rows} WHERE {field} = 0{beside}"), all_rows)
}

/// Returns the `sourceValueCompleteness` query of `column` in the quoted
/// `table`, mapped through `standard`.
fn source_value_completeness(table: &str, column: &ColumnMeta, standard: &ColumnMeta) -> String {
    // NOTE: DQD v2.9.0 `field_source_value_completeness.sql` lines 17 to 51: the
    // distinct source values, NULL as one, whose standard concept is 0.
    let field = field(column);
    counts(
        &format!(
            "SELECT count(*) FROM (SELECT DISTINCT {field} FROM {table} AS {CDM_TABLE} \
             WHERE {CDM_TABLE}.{} = 0) AS violated_rows",
            ident(standard.name)
        ),
        &format!(
            "SELECT count(DISTINCT {field}) \
             + count(DISTINCT CASE WHEN {field} IS NULL THEN 1 END) \
             FROM {table} AS {CDM_TABLE}"
        ),
    )
}

/// Returns the query that reports the two counts as one row.
fn counts(violated: &str, denominator: &str) -> String {
    format!(
        "SELECT ({violated})::bigint AS num_violated_rows, \
         ({denominator})::bigint AS num_denominator_rows"
    )
}

/// Returns `column` of the checked table, qualified by its alias.
fn field(column: &ColumnMeta) -> String {
    format!("{CDM_TABLE}.{}", ident(column.name))
}

/// Returns the join of the checked column to `concept`, with the templates'
/// filter on concept 0.
fn concept_join(column: &ColumnMeta) -> String {
    format!(
        "JOIN \"concept\" AS co ON {} = co.\"concept_id\" WHERE co.\"concept_id\" != 0",
        field(column)
    )
}

/// Returns `name` as a double-quoted PostgreSQL identifier.
fn ident(name: &str) -> String {
    format!("\"{}\"", name.replace('"', "\"\""))
}

/// Returns `value` as a single-quoted PostgreSQL string literal.
fn literal(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}
