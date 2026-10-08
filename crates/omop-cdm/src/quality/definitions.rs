// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The vocabulary of the Data Quality Dashboard's own files, which the
//! generated [`crate::generated::dqd`] module is written in.
//!
//! `OMOP_CDMv5.4_Check_Descriptions.csv` becomes one [`CheckDescription`] per
//! check, and `OMOP_CDMv5.4_Table_Level.csv` and
//! `OMOP_CDMv5.4_Field_Level.csv` one [`TableThresholds`] or
//! [`FieldThresholds`] per record, keyed by the generated metadata's table and
//! column names (<https://github.com/OHDSI/DataQualityDashboard>, tag
//! `v2.9.0`).

use crate::generated::dqd;

/// The level a check runs at (`checkLevel`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum CheckLevel {
    /// Once per table.
    Table,
    /// Once per field.
    Field,
    /// Once per concept.
    Concept,
}

impl CheckLevel {
    /// Returns the level as the catalogue spells it, for example `FIELD`.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Table => "TABLE",
            Self::Field => "FIELD",
            Self::Concept => "CONCEPT",
        }
    }
}

/// The Kahn context of a check (`kahnContext`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum KahnContext {
    /// Conformance with the data's own specification.
    Verification,
    /// Conformance with an external benchmark.
    Validation,
}

impl KahnContext {
    /// Returns the context as the catalogue spells it.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Verification => "Verification",
            Self::Validation => "Validation",
        }
    }
}

/// The Kahn category of a check (`kahnCategory`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum KahnCategory {
    /// The data follow the model's structure and value rules.
    Conformance,
    /// The data are present where they are expected.
    Completeness,
    /// The data are believable.
    Plausibility,
}

impl KahnCategory {
    /// Returns the category as the catalogue spells it.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Conformance => "Conformance",
            Self::Completeness => "Completeness",
            Self::Plausibility => "Plausibility",
        }
    }
}

/// The Kahn subcategory of a check (`kahnSubcategory`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum KahnSubcategory {
    /// Relations between tables and fields.
    Relational,
    /// The values a field may hold.
    Value,
    /// Values computed from other values.
    Computational,
    /// Order in time.
    Temporal,
    /// Values without regard to time.
    Atemporal,
}

impl KahnSubcategory {
    /// Returns the subcategory as the catalogue spells it.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Relational => "Relational",
            Self::Value => "Value",
            Self::Computational => "Computational",
            Self::Temporal => "Temporal",
            Self::Atemporal => "Atemporal",
        }
    }
}

/// The severity of a check (`severity`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Severity {
    /// The data cannot be used.
    Fatal,
    /// The data break a convention of the model.
    Convention,
    /// The data are described, not judged.
    Characterization,
}

impl Severity {
    /// Returns the severity as the catalogue spells it.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Fatal => "fatal",
            Self::Convention => "convention",
            Self::Characterization => "characterization",
        }
    }
}

/// One record of the check catalogue.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CheckDescription {
    /// The level the check runs at.
    pub level: CheckLevel,
    /// The check name, for example `isRequired`.
    pub name: &'static str,
    /// The description template, with `@cdmTableName`-style placeholders.
    pub description: &'static str,
    /// The Kahn context.
    pub kahn_context: KahnContext,
    /// The Kahn category.
    pub kahn_category: KahnCategory,
    /// The Kahn subcategory, when the catalogue names one.
    pub kahn_subcategory: Option<KahnSubcategory>,
    /// The Dashboard's SQL template file, for example `field_fk_domain.sql`.
    pub sql_file: &'static str,
    /// The Dashboard's R filter over the settings, verbatim.
    pub evaluation_filter: &'static str,
    /// The severity.
    pub severity: Severity,
}

/// A threshold: the percentage of violating rows a check tolerates, from 0 to
/// 100.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Percentage(pub(crate) u8);

impl Percentage {
    /// Returns the percentage `value`, or `None` when it is above 100.
    #[must_use]
    pub const fn new(value: u8) -> Option<Self> {
        if value <= 100 {
            Some(Self(value))
        } else {
            None
        }
    }

    /// Returns the percentage as a number from 0 to 100.
    #[must_use]
    pub const fn value(self) -> u8 {
        self.0
    }
}

/// One check's cells in one threshold record.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CheckSetting {
    /// The check the cells belong to.
    pub check: &'static CheckDescription,
    /// The cell named after the check, for example `Yes` or `integer`.
    pub value: Option<&'static str>,
    /// The `<check>Threshold` cell.
    pub threshold: Option<Percentage>,
    /// The `<check>Notes` cell.
    pub notes: Option<&'static str>,
}

impl CheckSetting {
    /// Creates the setting of `check`.
    #[must_use]
    pub const fn new(
        check: &'static CheckDescription,
        value: Option<&'static str>,
        threshold: Option<Percentage>,
        notes: Option<&'static str>,
    ) -> Self {
        Self {
            check,
            value,
            threshold,
            notes,
        }
    }
}

/// One non-empty parameter cell of a threshold record, for example
/// `standardConceptFieldName`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Parameter {
    /// The column name.
    pub name: &'static str,
    /// The cell, verbatim.
    pub value: &'static str,
}

impl Parameter {
    /// Creates the parameter `name` with `value`.
    #[must_use]
    pub const fn new(name: &'static str, value: &'static str) -> Self {
        Self { name, value }
    }
}

/// The settings of one table, from `OMOP_CDMv5.4_Table_Level.csv`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TableThresholds {
    /// The table, as [`crate::meta::TableMeta::name`] spells it.
    pub table: &'static str,
    /// Every check with a non-empty cell, in column order.
    pub checks: &'static [CheckSetting],
    /// Every non-empty parameter, in column order.
    pub parameters: &'static [Parameter],
}

impl TableThresholds {
    /// Returns the setting of the check named `check`.
    #[must_use]
    pub fn setting(&self, check: &str) -> Option<&'static CheckSetting> {
        setting(self.checks, check)
    }

    /// Returns the parameter named `name`.
    #[must_use]
    pub fn parameter(&self, name: &str) -> Option<&'static str> {
        parameter(self.parameters, name)
    }
}

/// The settings of one field, from `OMOP_CDMv5.4_Field_Level.csv`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FieldThresholds {
    /// The table, as [`crate::meta::TableMeta::name`] spells it.
    pub table: &'static str,
    /// The field, as [`crate::meta::ColumnMeta::name`] spells it.
    pub field: &'static str,
    /// Every check with a non-empty cell, in column order.
    pub checks: &'static [CheckSetting],
    /// Every non-empty parameter, in column order.
    pub parameters: &'static [Parameter],
}

impl FieldThresholds {
    /// Returns the setting of the check named `check`.
    #[must_use]
    pub fn setting(&self, check: &str) -> Option<&'static CheckSetting> {
        setting(self.checks, check)
    }

    /// Returns the parameter named `name`.
    #[must_use]
    pub fn parameter(&self, name: &str) -> Option<&'static str> {
        parameter(self.parameters, name)
    }
}

/// Returns the setting of the check named `check` among `checks`.
fn setting(checks: &'static [CheckSetting], check: &str) -> Option<&'static CheckSetting> {
    checks.iter().find(|setting| setting.check.name == check)
}

/// Returns the value of the parameter named `name` among `parameters`.
fn parameter(parameters: &'static [Parameter], name: &str) -> Option<&'static str> {
    parameters
        .iter()
        .find(|parameter| parameter.name == name)
        .map(|parameter| parameter.value)
}

/// Returns the catalogue record of the check named `name`.
///
/// # Examples
///
/// ```
/// use omop_cdm::quality::definitions::{self, KahnCategory};
///
/// let check = definitions::check_description("fkDomain").ok_or("no fkDomain check")?;
/// assert_eq!(KahnCategory::Conformance, check.kahn_category);
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
#[must_use]
pub fn check_description(name: &str) -> Option<&'static CheckDescription> {
    dqd::CHECK_DESCRIPTIONS
        .iter()
        .find(|check| check.name == name)
        .copied()
}

/// Returns the settings of the table named `table`.
#[must_use]
pub fn table_thresholds(table: &str) -> Option<&'static TableThresholds> {
    dqd::TABLE_THRESHOLDS
        .iter()
        .find(|thresholds| thresholds.table == table)
}

/// Returns the settings of the field `field` of the table `table`.
#[must_use]
pub fn field_thresholds(table: &str, field: &str) -> Option<&'static FieldThresholds> {
    dqd::FIELD_THRESHOLDS
        .iter()
        .find(|thresholds| thresholds.table == table && thresholds.field == field)
}
