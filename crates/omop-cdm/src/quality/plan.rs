// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Which check instances exist for the CDM v5.4 tables.
//!
//! A check runs where the CDM v5.4.3 definitions say its fact holds:
//! `cdmTable` for every table, `cdmField` for every column, `isRequired`
//! where the column is required, `cdmDatatype` where it is an integer,
//! `isPrimaryKey` where it is the primary key, `isForeignKey` where it is a
//! foreign key, and `fkDomain` and `fkClass` where a foreign key names a
//! domain or a class. `isStandardValidConcept` has no fact in the definitions
//! and runs where the Dashboard's field settings enable it. The thresholds and
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
    /// table of [`generated::TABLES`].
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
            for column in table.columns {
                field_checks(table, column, &mut checks);
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

/// Appends the field checks of `column`, in catalogue order.
fn field_checks(table: &'static TableMeta, column: &'static ColumnMeta, checks: &mut Vec<Check>) {
    let settings = definitions::field_thresholds(table.name, column.name);
    let setting = |name: CheckName| -> Option<&'static CheckSetting> {
        settings.and_then(|settings| settings.setting(name.as_str()))
    };
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
    // NOTE: DQD v2.9.0 `OMOP_CDMv5.4_Check_Descriptions.csv` filters this check on
    // `isStandardValidConcept=='Yes'`; the CDM definitions carry no such fact.
    let standard = setting(CheckName::IsStandardValidConcept)
        .is_some_and(|setting| setting.value == Some("Yes"));
    if standard {
        push(
            CheckName::IsStandardValidConcept,
            Kind::IsStandardValidConcept(column),
        );
    }
}
