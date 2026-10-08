// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Reading the OHDSI Data Quality Dashboard's CDM v5.4 files and lowering them
//! into the catalogue the emitter renders.
//!
//! `OMOP_CDMv5.4_Check_Descriptions.csv` is the check catalogue, and
//! `OMOP_CDMv5.4_Table_Level.csv` and `OMOP_CDMv5.4_Field_Level.csv` carry the
//! per-table and per-field settings that enable and parameterise each check
//! (<https://github.com/OHDSI/DataQualityDashboard>, tag `v2.9.0`). The
//! catalogue is the root set and every record of it is emitted.
//!
//! A threshold file's columns are read by name. A column named after a check
//! of the catalogue at the file's level carries that check's value, the
//! `<check>Threshold` and `<check>Notes` columns its threshold and notes, and
//! every other column is a parameter, except the key columns and the
//! documentation columns in [`TABLE_DOCUMENTATION`] and
//! [`FIELD_DOCUMENTATION`], which the emitter leaves out.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::lower::Model;

/// The file name of the Dashboard's check catalogue.
pub const CHECK_DESCRIPTIONS: &str = "OMOP_CDMv5.4_Check_Descriptions.csv";

/// The file name of the Dashboard's table thresholds.
pub const TABLE_LEVEL: &str = "OMOP_CDMv5.4_Table_Level.csv";

/// The file name of the Dashboard's field thresholds.
pub const FIELD_LEVEL: &str = "OMOP_CDMv5.4_Field_Level.csv";

/// The table thresholds' columns the emitter leaves out: the prose the CDM
/// definitions already carry, and the schema, which `CdmSchema` carries.
pub const TABLE_DOCUMENTATION: [&str; 5] = [
    "schema",
    "validation",
    "tableDescription",
    "userGuidance",
    "etlConventions",
];

/// The field thresholds' columns the emitter leaves out, on the same ground.
pub const FIELD_DOCUMENTATION: [&str; 3] = ["databaseSchema", "userGuidance", "etlConventions"];

/// One record of the check catalogue, as the CSV spells it.
#[derive(Debug, Clone, Deserialize)]
pub struct CheckRecord {
    /// `TABLE`, `FIELD` or `CONCEPT`.
    #[serde(rename = "checkLevel")]
    pub level: String,
    /// The check name, for example `isRequired`.
    #[serde(rename = "checkName")]
    pub name: String,
    /// The description template, with `@` placeholders.
    #[serde(rename = "checkDescription")]
    pub description: String,
    /// The Kahn context.
    #[serde(rename = "kahnContext")]
    pub context: String,
    /// The Kahn category.
    #[serde(rename = "kahnCategory")]
    pub category: String,
    /// The Kahn subcategory, empty for some checks.
    #[serde(rename = "kahnSubcategory")]
    pub subcategory: String,
    /// The SQL template file.
    #[serde(rename = "sqlFile")]
    pub sql_file: String,
    /// The R filter that selects the threshold rows a check runs for.
    #[serde(rename = "evaluationFilter")]
    pub evaluation_filter: String,
    /// The severity.
    #[serde(rename = "severity")]
    pub severity: String,
}

/// One record of a threshold file: every cell beside its column name, in file
/// order.
#[derive(Debug, Clone)]
pub struct ThresholdRecord {
    /// The cells, each with its header, in column order.
    pub cells: Vec<(String, String)>,
}

impl ThresholdRecord {
    /// Returns the cell under `column`, if the file has that column.
    #[must_use]
    pub fn cell(&self, column: &str) -> Option<&str> {
        self.cells
            .iter()
            .find(|(header, _)| header == column)
            .map(|(_, cell)| cell.as_str())
    }
}

/// The three Dashboard files, in file order.
#[derive(Debug, Clone)]
pub struct Dashboard {
    /// The catalogue records.
    pub checks: Vec<CheckRecord>,
    /// The table threshold records.
    pub tables: Vec<ThresholdRecord>,
    /// The field threshold records.
    pub fields: Vec<ThresholdRecord>,
}

/// A Dashboard file could not be read.
#[derive(Debug, thiserror::Error)]
#[error("cannot read the Data Quality Dashboard file at {path}")]
pub struct LoadError {
    /// The path that was tried.
    pub path: PathBuf,
    /// The underlying error.
    #[source]
    pub source: csv::Error,
}

impl Dashboard {
    /// Reads the three files from `dir`.
    ///
    /// # Errors
    ///
    /// Returns [`LoadError`] when a file is missing or unreadable, or when a
    /// record does not match its header.
    pub fn load(dir: &Path) -> Result<Self, LoadError> {
        let path = dir.join(CHECK_DESCRIPTIONS);
        let mut reader = reader(&path)?;
        let mut checks = Vec::new();
        for record in reader.deserialize() {
            checks.push(record.map_err(|source| LoadError {
                path: path.clone(),
                source,
            })?);
        }
        Ok(Self {
            checks,
            tables: threshold_records(&dir.join(TABLE_LEVEL))?,
            fields: threshold_records(&dir.join(FIELD_LEVEL))?,
        })
    }
}

/// Opens the CSV file at `path` with its header row.
fn reader(path: &Path) -> Result<csv::Reader<std::fs::File>, LoadError> {
    csv::ReaderBuilder::new()
        .has_headers(true)
        .from_path(path)
        .map_err(|source| LoadError {
            path: path.to_path_buf(),
            source,
        })
}

/// Reads every record of a threshold file, each cell beside its header.
fn threshold_records(path: &Path) -> Result<Vec<ThresholdRecord>, LoadError> {
    let mut reader = reader(path)?;
    let headers = reader
        .headers()
        .map_err(|source| LoadError {
            path: path.to_path_buf(),
            source,
        })?
        .clone();
    let mut records = Vec::new();
    for record in reader.records() {
        let record = record.map_err(|source| LoadError {
            path: path.to_path_buf(),
            source,
        })?;
        records.push(ThresholdRecord {
            cells: headers
                .iter()
                .zip(record.iter())
                .map(|(header, cell)| (header.to_owned(), cell.to_owned()))
                .collect(),
        });
    }
    Ok(records)
}

/// The level a check runs at.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Level {
    /// Once per table.
    Table,
    /// Once per field.
    Field,
    /// Once per concept.
    Concept,
}

impl Level {
    /// Returns the variant of `omop_cdm::quality::definitions::CheckLevel`
    /// this level is.
    #[must_use]
    pub fn variant(self) -> &'static str {
        match self {
            Self::Table => "Table",
            Self::Field => "Field",
            Self::Concept => "Concept",
        }
    }
}

/// One catalogue record, its closed-set cells read into variant names.
#[derive(Debug, Clone)]
pub struct Check {
    /// The level.
    pub level: Level,
    /// The check name, as the Dashboard spells it.
    pub name: String,
    /// The description template, verbatim.
    pub description: String,
    /// The `KahnContext` variant.
    pub context: &'static str,
    /// The `KahnCategory` variant.
    pub category: &'static str,
    /// The `KahnSubcategory` variant, when the catalogue names one.
    pub subcategory: Option<&'static str>,
    /// The SQL template file.
    pub sql_file: String,
    /// The evaluation filter, verbatim.
    pub evaluation_filter: String,
    /// The `Severity` variant.
    pub severity: &'static str,
}

impl Check {
    /// Returns the name of the `static` the check is emitted as, for example
    /// `IS_STANDARD_VALID_CONCEPT` for `isStandardValidConcept`.
    #[must_use]
    pub fn static_name(&self) -> String {
        screaming_snake_case(&self.name)
    }
}

/// One check's settings in one threshold record.
#[derive(Debug, Clone)]
pub struct Setting {
    /// The check the settings belong to.
    pub check: String,
    /// The cell named after the check, when it is not empty.
    pub value: Option<String>,
    /// The `<check>Threshold` cell, a percentage from 0 to 100.
    pub threshold: Option<u8>,
    /// The `<check>Notes` cell, when it is not empty.
    pub notes: Option<String>,
}

/// One non-empty parameter cell of a threshold record.
#[derive(Debug, Clone)]
pub struct Parameter {
    /// The column name.
    pub name: String,
    /// The cell, verbatim.
    pub value: String,
}

/// The settings of one table or one field.
#[derive(Debug, Clone)]
pub struct Thresholds {
    /// The table, spelled as the generated metadata spells it.
    pub table: String,
    /// The field, for a field record.
    pub field: Option<String>,
    /// Every check with at least one non-empty cell, in column order.
    pub settings: Vec<Setting>,
    /// Every non-empty parameter cell, in column order.
    pub parameters: Vec<Parameter>,
}

/// The lowered Dashboard: the catalogue in file order, and the table and field
/// settings in the generated metadata's table and column order.
#[derive(Debug, Clone)]
pub struct Catalogue {
    /// The checks, in catalogue order.
    pub checks: Vec<Check>,
    /// The table settings.
    pub tables: Vec<Thresholds>,
    /// The field settings.
    pub fields: Vec<Thresholds>,
}

/// A Dashboard record the emitter cannot carry.
#[derive(Debug, thiserror::Error)]
pub enum DqdError {
    /// A closed-set cell of the catalogue holds a value outside the set.
    #[error("the {check} check has `{value}` in its {column} cell")]
    Catalogue {
        /// The check.
        check: String,
        /// The column.
        column: &'static str,
        /// The value.
        value: String,
    },
    /// The catalogue names a check twice.
    #[error("the catalogue names the {check} check twice")]
    DuplicateCheck {
        /// The check.
        check: String,
    },
    /// A threshold cell is not an integer percentage from 0 to 100.
    #[error("{place} has `{value}` in its {column} cell, not a percentage from 0 to 100")]
    Threshold {
        /// The table, or the table and field.
        place: String,
        /// The column.
        column: String,
        /// The value.
        value: String,
    },
    /// A threshold or notes column names no check of the catalogue.
    #[error("the {file} column {column} names no {level:?} check of the catalogue")]
    UnknownCheck {
        /// The file.
        file: &'static str,
        /// The column.
        column: String,
        /// The level the file's checks run at.
        level: Level,
    },
    /// A threshold record lacks its key column.
    #[error("a record of {file} has no {column} cell")]
    MissingKey {
        /// The file.
        file: &'static str,
        /// The key column.
        column: &'static str,
    },
    /// A threshold record names a table or field the CDM definitions do not.
    #[error("{file} names {place}, which the CDM definitions do not")]
    Unknown {
        /// The file.
        file: &'static str,
        /// The table, or the table and field.
        place: String,
    },
    /// A threshold file carries one table or field twice.
    #[error("{file} carries {place} twice")]
    Duplicate {
        /// The file.
        file: &'static str,
        /// The table, or the table and field.
        place: String,
    },
}

impl Catalogue {
    /// Lowers the three Dashboard files against the CDM model the rest of the
    /// crate is emitted from.
    ///
    /// # Errors
    ///
    /// Returns [`DqdError`] when a catalogue cell is outside its closed set, a
    /// threshold is not a percentage, a threshold or notes column names no
    /// check, or a record names a table or field the model lacks or names one
    /// twice.
    pub fn lower(dashboard: &Dashboard, model: &Model) -> Result<Self, DqdError> {
        let mut checks = Vec::with_capacity(dashboard.checks.len());
        let mut names = BTreeSet::new();
        for record in &dashboard.checks {
            if !names.insert(record.name.as_str()) {
                return Err(DqdError::DuplicateCheck {
                    check: record.name.clone(),
                });
            }
            checks.push(lower_check(record)?);
        }
        let table_checks = check_names(&checks, Level::Table);
        let field_checks = check_names(&checks, Level::Field);

        let mut tables = BTreeMap::new();
        for record in &dashboard.tables {
            let thresholds = lower_record(record, &table_checks, TABLE_FILE)?;
            let place = thresholds.table.clone();
            if model.tables.iter().all(|table| table.name != place) {
                return Err(DqdError::Unknown {
                    file: TABLE_LEVEL,
                    place,
                });
            }
            if tables.insert(place.clone(), thresholds).is_some() {
                return Err(DqdError::Duplicate {
                    file: TABLE_LEVEL,
                    place,
                });
            }
        }
        let mut fields = BTreeMap::new();
        for record in &dashboard.fields {
            let thresholds = lower_record(record, &field_checks, FIELD_FILE)?;
            let Some(field) = thresholds.field.clone() else {
                return Err(DqdError::MissingKey {
                    file: FIELD_LEVEL,
                    column: "cdmFieldName",
                });
            };
            let place = format!("{}.{field}", thresholds.table);
            let known = model.tables.iter().any(|table| {
                table.name == thresholds.table
                    && table.columns.iter().any(|column| column.name == field)
            });
            if !known {
                return Err(DqdError::Unknown {
                    file: FIELD_LEVEL,
                    place,
                });
            }
            if fields
                .insert((thresholds.table.clone(), field), thresholds)
                .is_some()
            {
                return Err(DqdError::Duplicate {
                    file: FIELD_LEVEL,
                    place,
                });
            }
        }

        // NOTE: no specification governs this order: our own design; the
        // settings follow the generated metadata so both read in one order.
        let mut ordered_tables = Vec::with_capacity(tables.len());
        let mut ordered_fields = Vec::with_capacity(fields.len());
        for table in &model.tables {
            if let Some(thresholds) = tables.remove(&table.name) {
                ordered_tables.push(thresholds);
            }
            for column in &table.columns {
                if let Some(thresholds) = fields.remove(&(table.name.clone(), column.name.clone()))
                {
                    ordered_fields.push(thresholds);
                }
            }
        }
        Ok(Self {
            checks,
            tables: ordered_tables,
            fields: ordered_fields,
        })
    }
}

/// The names of the catalogue's checks at `level`.
fn check_names(checks: &[Check], level: Level) -> BTreeSet<String> {
    checks
        .iter()
        .filter(|check| check.level == level)
        .map(|check| check.name.clone())
        .collect()
}

/// Lowers one catalogue record, refusing a closed-set cell outside its set.
fn lower_check(record: &CheckRecord) -> Result<Check, DqdError> {
    let refuse = |column: &'static str, value: &str| DqdError::Catalogue {
        check: record.name.clone(),
        column,
        value: value.to_owned(),
    };
    let level = match record.level.as_str() {
        "TABLE" => Level::Table,
        "FIELD" => Level::Field,
        "CONCEPT" => Level::Concept,
        other => return Err(refuse("checkLevel", other)),
    };
    let context = match record.context.as_str() {
        "Verification" => "Verification",
        "Validation" => "Validation",
        other => return Err(refuse("kahnContext", other)),
    };
    let category = match record.category.as_str() {
        "Conformance" => "Conformance",
        "Completeness" => "Completeness",
        "Plausibility" => "Plausibility",
        other => return Err(refuse("kahnCategory", other)),
    };
    let subcategory = match record.subcategory.as_str() {
        "" => None,
        "Relational" => Some("Relational"),
        "Value" => Some("Value"),
        "Computational" => Some("Computational"),
        "Temporal" => Some("Temporal"),
        "Atemporal" => Some("Atemporal"),
        other => return Err(refuse("kahnSubcategory", other)),
    };
    let severity = match record.severity.as_str() {
        "fatal" => "Fatal",
        "convention" => "Convention",
        "characterization" => "Characterization",
        other => return Err(refuse("severity", other)),
    };
    if record.name.is_empty() || !record.name.chars().all(|c| c.is_ascii_alphanumeric()) {
        return Err(refuse("checkName", &record.name));
    }
    Ok(Check {
        level,
        name: record.name.clone(),
        description: record.description.clone(),
        context,
        category,
        subcategory,
        sql_file: record.sql_file.clone(),
        evaluation_filter: record.evaluation_filter.clone(),
        severity,
    })
}

/// What distinguishes the two threshold files.
#[derive(Debug, Clone, Copy)]
struct ThresholdFile {
    /// The file name.
    name: &'static str,
    /// The level of the checks the file configures.
    level: Level,
    /// The documentation columns the emitter leaves out.
    documentation: &'static [&'static str],
    /// Whether a record is keyed by a field as well as a table.
    keyed_by_field: bool,
}

/// `OMOP_CDMv5.4_Table_Level.csv`.
const TABLE_FILE: ThresholdFile = ThresholdFile {
    name: TABLE_LEVEL,
    level: Level::Table,
    documentation: &TABLE_DOCUMENTATION,
    keyed_by_field: false,
};

/// `OMOP_CDMv5.4_Field_Level.csv`.
const FIELD_FILE: ThresholdFile = ThresholdFile {
    name: FIELD_LEVEL,
    level: Level::Field,
    documentation: &FIELD_DOCUMENTATION,
    keyed_by_field: true,
};

/// Lowers one threshold record: its key, then every column as a check
/// setting or a parameter.
fn lower_record(
    record: &ThresholdRecord,
    checks: &BTreeSet<String>,
    file: ThresholdFile,
) -> Result<Thresholds, DqdError> {
    let key = |column: &'static str| {
        record
            .cell(column)
            .map(|cell| cell.trim().to_lowercase())
            .filter(|cell| !cell.is_empty())
            .ok_or(DqdError::MissingKey {
                file: file.name,
                column,
            })
    };
    let table = key("cdmTableName")?;
    let field = if file.keyed_by_field {
        Some(key("cdmFieldName")?)
    } else {
        None
    };
    let place = field
        .as_ref()
        .map_or_else(|| table.clone(), |field| format!("{table}.{field}"));

    let mut settings: Vec<Setting> = Vec::new();
    let mut parameters = Vec::new();
    for (column, cell) in &record.cells {
        if column == "cdmTableName"
            || column == "cdmFieldName"
            || file.documentation.contains(&column.as_str())
        {
            continue;
        }
        let (check, part) = if checks.contains(column) {
            (column.as_str(), Part::Value)
        } else if let Some(check) = column.strip_suffix("Threshold") {
            (check, Part::Threshold)
        } else if let Some(check) = column.strip_suffix("Notes") {
            (check, Part::Notes)
        } else {
            if !cell.is_empty() {
                parameters.push(Parameter {
                    name: column.clone(),
                    value: cell.clone(),
                });
            }
            continue;
        };
        if !checks.contains(check) {
            return Err(DqdError::UnknownCheck {
                file: file.name,
                column: column.clone(),
                level: file.level,
            });
        }
        if settings.iter().all(|setting| setting.check != check) {
            settings.push(Setting {
                check: check.to_owned(),
                value: None,
                threshold: None,
                notes: None,
            });
        }
        let Some(setting) = settings.iter_mut().find(|setting| setting.check == check) else {
            continue;
        };
        let text = Some(cell.clone()).filter(|cell| !cell.is_empty());
        match part {
            Part::Value => setting.value = text,
            Part::Notes => setting.notes = text,
            Part::Threshold => setting.threshold = percentage(&place, column, cell)?,
        }
    }
    settings.retain(|setting| {
        setting.value.is_some() || setting.threshold.is_some() || setting.notes.is_some()
    });
    Ok(Thresholds {
        table,
        field,
        settings,
        parameters,
    })
}

/// Which of a check's three columns a cell is.
#[derive(Debug, Clone, Copy)]
enum Part {
    /// The column named after the check.
    Value,
    /// The `<check>Threshold` column.
    Threshold,
    /// The `<check>Notes` column.
    Notes,
}

/// Reads a threshold cell: empty is no threshold, and anything else must be
/// an integer from 0 to 100.
fn percentage(place: &str, column: &str, cell: &str) -> Result<Option<u8>, DqdError> {
    if cell.is_empty() {
        return Ok(None);
    }
    let refuse = || DqdError::Threshold {
        place: place.to_owned(),
        column: column.to_owned(),
        value: cell.to_owned(),
    };
    if !cell.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(refuse());
    }
    match cell.parse::<u8>() {
        Ok(value) if value <= 100 => Ok(Some(value)),
        _ => Err(refuse()),
    }
}

/// Converts a camel-case check name into a screaming-snake-case `static`
/// name.
#[must_use]
pub fn screaming_snake_case(name: &str) -> String {
    let mut out = String::with_capacity(name.len() + 8);
    for c in name.chars() {
        if c.is_ascii_uppercase() && !out.is_empty() {
            out.push('_');
        }
        out.push(c.to_ascii_uppercase());
    }
    out
}
