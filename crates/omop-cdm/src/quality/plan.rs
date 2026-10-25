// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Which check instances exist for the CDM v5.4 tables.
//!
//! A check runs where the CDM v5.4.3 definitions say its fact holds:
//! `cdmTable` for every table, `cdmField` for every column, `isRequired`
//! where the column is required, `cdmDatatype` where it is an integer,
//! `isPrimaryKey` where it is the primary key, `isForeignKey` where it is a
//! foreign key, and `fkDomain` and `fkClass` where a foreign key names a
//! domain or a class. `isStandardValidConcept` and the completeness checks
//! have no fact in the definitions and run where the Dashboard's table or
//! field settings enable them. The thresholds and
//! the notes always come from the Dashboard's settings.

use std::collections::BTreeSet;

use crate::generated;
use crate::meta::{ColumnMeta, TableMeta};
use crate::quality::check::{Check, CheckName, Kind};
use crate::quality::definitions::{self, CheckSetting};

/// What a plan covers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlanOptions {
    /// The tables the plan leaves out, spelled as [`TableMeta::name`] spells
    /// them; the comparison ignores case.
    ///
    /// The default is the Dashboard's default `tablesToExclude`, the eight
    /// standardized vocabulary tables it names that CDM v5.4 defines.
    pub excluded_tables: BTreeSet<String>,
}

impl Default for PlanOptions {
    fn default() -> Self {
        // NOTE: DQD v2.9.0 `R/executeDqChecks.R` lines 86 to 89; the list's
        // PACK_CONTENT and the two *_METADATA tables name no CDM v5.4 table.
        Self {
            excluded_tables: [
                "concept",
                "vocabulary",
                "concept_ancestor",
                "concept_relationship",
                "concept_class",
                "concept_synonym",
                "relationship",
                "domain",
            ]
            .into_iter()
            .map(String::from)
            .collect(),
        }
    }
}

/// The plan could not be built.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum PlanError {
    /// An excluded table is not a CDM v5.4 table.
    #[error("the excluded table {table} is not a CDM v5.4 table")]
    UnknownTable {
        /// The name the caller gave.
        table: String,
    },
    /// A check's settings or template name a column its table does not have.
    #[error("the {check} check of {table}.{field} names the column {column}, which {table} lacks")]
    UnknownColumn {
        /// The check.
        check: &'static str,
        /// The table.
        table: &'static str,
        /// The checked field.
        field: &'static str,
        /// The column the settings or the template name.
        column: String,
    },
}

/// The check instances for the CDM, in table order, then column order, then
/// catalogue order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Plan {
    checks: Vec<Check>,
}

impl Plan {
    /// Builds the plan `options` describe.
    ///
    /// # Errors
    ///
    /// Returns [`PlanError::UnknownTable`] when an excluded table names no
    /// table of [`generated::TABLES`], and [`PlanError::UnknownColumn`] when
    /// a completeness check's settings or template name a column its table
    /// lacks.
    pub fn new(options: &PlanOptions) -> Result<Self, PlanError> {
        let mut excluded = BTreeSet::new();
        for table in &options.excluded_tables {
            let name = table.to_lowercase();
            if crate::meta::table(&name).is_none() {
                return Err(PlanError::UnknownTable {
                    table: table.clone(),
                });
            }
            excluded.insert(name);
        }
        let mut checks = Vec::new();
        for table in generated::TABLES
            .iter()
            .filter(|table| !excluded.contains(table.name))
        {
            checks.push(Check::new(table, Kind::CdmTable, None, None));
            table_checks(table, &mut checks);
            for column in table.columns {
                field_checks(table, column, &mut checks)?;
            }
        }
        Ok(Self { checks })
    }

    /// Returns the check instances, in plan order.
    #[must_use]
    pub fn checks(&self) -> &[Check] {
        &self.checks
    }
}

/// Appends the table-level completeness checks the Dashboard's table
/// settings enable for `table`, in catalogue order.
fn table_checks(table: &'static TableMeta, checks: &mut Vec<Check>) {
    let settings = definitions::table_thresholds(table.name);
    for (name, kind) in [
        (
            CheckName::MeasurePersonCompleteness,
            Kind::MeasurePersonCompleteness,
        ),
        (
            CheckName::MeasureConditionEraCompleteness,
            Kind::MeasureConditionEraCompleteness,
        ),
    ] {
        // NOTE: DQD v2.9.0 `OMOP_CDMv5.4_Check_Descriptions.csv` filters both
        // checks on the table setting being `Yes`.
        if let Some(setting) = settings
            .and_then(|settings| settings.setting(name.as_str()))
            .filter(|setting| setting.value == Some("Yes"))
        {
            checks.push(Check::new(table, kind, setting.threshold, setting.notes));
        }
    }
}

/// Appends the field checks of `column`, in catalogue order.
fn field_checks(
    table: &'static TableMeta,
    column: &'static ColumnMeta,
    checks: &mut Vec<Check>,
) -> Result<(), PlanError> {
    let settings = definitions::field_thresholds(table.name, column.name);
    let setting = |name: CheckName| -> Option<&'static CheckSetting> {
        settings.and_then(|settings| settings.setting(name.as_str()))
    };
    let enabled =
        |name: CheckName| setting(name).is_some_and(|setting| setting.value == Some("Yes"));
    let mut push = |name: CheckName, kind: Kind| {
        let setting = setting(name);
        checks.push(Check::new(
            table,
            kind,
            setting.and_then(|setting| setting.threshold),
            setting.and_then(|setting| setting.notes),
        ));
    };

    // NOTE: DQD v2.9.0 `R/executeDqChecks.R` line 238 drops `offset` because its
    // SQL cannot quote it; every identifier here is quoted, so the column is checked.
    push(CheckName::CdmField, Kind::CdmField(column));
    if column.required {
        push(CheckName::IsRequired, Kind::IsRequired(column));
    }
    // NOTE: the definitions spell one integer `Integer`, which the emitter
    // normalises, so the catalogue's `cdmDatatype=='integer'` filter reaches it.
    if column.cdm_datatype == "integer" {
        push(CheckName::CdmDatatype, Kind::CdmDatatype(column));
    }
    if column.primary_key {
        push(CheckName::IsPrimaryKey, Kind::IsPrimaryKey(column));
    }
    if let Some((fk_table, fk_field)) = column.foreign_key {
        push(
            CheckName::IsForeignKey,
            Kind::IsForeignKey {
                column,
                table: fk_table,
                field: fk_field,
            },
        );
        if !column.fk_domain.is_empty() {
            push(
                CheckName::FkDomain,
                Kind::FkDomain {
                    column,
                    domains: column.fk_domain,
                },
            );
        }
        if let Some(class) = column.fk_class {
            push(CheckName::FkClass, Kind::FkClass { column, class });
        }
    }
    // NOTE: DQD v2.9.0 `OMOP_CDMv5.4_Check_Descriptions.csv` filters this check and
    // the four completeness checks below on the field setting being `Yes`.
    if enabled(CheckName::IsStandardValidConcept) {
        push(
            CheckName::IsStandardValidConcept,
            Kind::IsStandardValidConcept(column),
        );
    }
    if enabled(CheckName::MeasureValueCompleteness) {
        push(
            CheckName::MeasureValueCompleteness,
            Kind::MeasureValueCompleteness(column),
        );
    }
    for name in [
        CheckName::StandardConceptRecordCompleteness,
        CheckName::SourceConceptRecordCompleteness,
    ] {
        if enabled(name) {
            push(name, concept_record_kind(name, table, column)?);
        }
    }
    if enabled(CheckName::SourceValueCompleteness) {
        let named = settings
            .and_then(|settings| settings.parameter("standardConceptFieldName"))
            .unwrap_or_default();
        let standard = table
            .column(&named.to_lowercase())
            .ok_or_else(|| unknown(CheckName::SourceValueCompleteness, table, column, named))?;
        push(
            CheckName::SourceValueCompleteness,
            Kind::SourceValueCompleteness { column, standard },
        );
    }
    Ok(())
}

/// Returns the refusal of a `name` check on `table.column` whose settings or
/// template name `missing`, a column `table` lacks.
fn unknown(
    name: CheckName,
    table: &'static TableMeta,
    column: &'static ColumnMeta,
    missing: &str,
) -> PlanError {
    PlanError::UnknownColumn {
        check: name.as_str(),
        table: table.name,
        field: column.name,
        column: missing.to_owned(),
    }
}

/// Returns the `name` concept record completeness check of `table.column`,
/// with the source value column its template names.
fn concept_record_kind(
    name: CheckName,
    table: &'static TableMeta,
    column: &'static ColumnMeta,
) -> Result<Kind, PlanError> {
    let source_value = match source_value_rule(table.name, column.name) {
        Some(named) => Some(
            table
                .column(named)
                .ok_or_else(|| unknown(name, table, column, named))?,
        ),
        None => None,
    };
    Ok(if name == CheckName::StandardConceptRecordCompleteness {
        Kind::StandardConceptRecordCompleteness {
            column,
            source_value,
        }
    } else {
        Kind::SourceConceptRecordCompleteness {
            column,
            source_value,
        }
    })
}

/// Returns the source value column whose value, beside a NULL concept field,
/// also violates a concept record completeness check of `table.field`.
///
/// The rules are the conditional `OR` clauses of DQD v2.9.0
/// `inst/sql/sql_server/field_concept_record_completeness.sql` lines 42 to 61,
/// matched on the upper-case names as the template matches them.
fn source_value_rule(table: &str, field: &str) -> Option<&'static str> {
    let table = table.to_uppercase();
    let field = field.to_uppercase();
    let either = |base: &str| {
        field == format!("{base}_CONCEPT_ID") || field == format!("{base}_SOURCE_CONCEPT_ID")
    };
    let rule = match field.as_str() {
        "UNIT_CONCEPT_ID" | "UNIT_SOURCE_CONCEPT_ID" if table != "DOSE_ERA" => "unit_source_value",
        "ADMITTED_FROM_CONCEPT_ID" => "admitted_from_source_value",
        "ADMITTING_SOURCE_CONCEPT_ID" => "admitting_source_value",
        "DISCHARGED_TO_CONCEPT_ID" => "discharged_to_source_value",
        "DISCHARGE_TO_CONCEPT_ID" => "discharge_to_source_value",
        "CONDITION_STATUS_CONCEPT_ID" => "condition_status_source_value",
        "ROUTE_CONCEPT_ID" => "route_source_value",
        "MODIFIER_CONCEPT_ID" => "modifier_source_value",
        "QUALIFIER_CONCEPT_ID" => "qualifier_source_value",
        "ANATOMIC_SITE_CONCEPT_ID" => "anatomic_site_source_value",
        "DISEASE_STATUS_CONCEPT_ID" => "disease_status_source_value",
        "COUNTRY_CONCEPT_ID" => "country_source_value",
        "PLACE_OF_SERVICE_CONCEPT_ID" => "place_of_service_source_value",
        _ if either("CAUSE") => "cause_source_value",
        _ if either("SPECIALTY") => "specialty_source_value",
        _ if table == "PROVIDER" && either("GENDER") => "gender_source_value",
        _ if either("PAYER") => "payer_source_value",
        _ if either("PLAN") => "plan_source_value",
        _ if either("SPONSOR") => "sponsor_source_value",
        _ if either("STOP_REASON") => "stop_reason_source_value",
        _ => return None,
    };
    Some(rule)
}
