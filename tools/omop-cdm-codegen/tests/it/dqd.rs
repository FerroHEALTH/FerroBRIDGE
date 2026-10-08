// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The Data Quality Dashboard files: what the emitter reads, how a column
//! becomes a setting or a parameter, and the cells it refuses.

use std::error::Error;
use std::path::Path;

use omop_cdm_codegen::definitions::Definitions;
use omop_cdm_codegen::dqd::{
    Catalogue, CheckRecord, Dashboard, DqdError, ThresholdRecord, screaming_snake_case,
};
use omop_cdm_codegen::lower::Model;

use crate::definitions::{CSV_DIR, DQD_DIR};

/// Lowers the vendored CDM definitions.
fn model() -> Result<Model, Box<dyn Error>> {
    Ok(Model::lower(&Definitions::load(Path::new(CSV_DIR))?)?)
}

/// One catalogue record with the given level and name, every other cell
/// valid.
fn check(level: &str, name: &str) -> CheckRecord {
    CheckRecord {
        level: level.to_owned(),
        name: name.to_owned(),
        description: String::from("@cdmTableName"),
        context: String::from("Verification"),
        category: String::from("Conformance"),
        subcategory: String::from("Relational"),
        sql_file: String::from("field_is_not_nullable.sql"),
        evaluation_filter: String::from("isRequired=='Yes'"),
        severity: String::from("fatal"),
    }
}

/// One threshold record of the given cells.
fn record(cells: &[(&str, &str)]) -> ThresholdRecord {
    ThresholdRecord {
        cells: cells
            .iter()
            .map(|(header, cell)| ((*header).to_owned(), (*cell).to_owned()))
            .collect(),
    }
}

/// A Dashboard with the `isRequired` field check and one field record.
fn dashboard(field: &[(&str, &str)]) -> Dashboard {
    Dashboard {
        checks: vec![check("FIELD", "isRequired")],
        tables: Vec::new(),
        fields: vec![record(field)],
    }
}

#[test]
fn the_vendored_files_carry_27_checks_39_tables_and_432_fields() -> Result<(), Box<dyn Error>> {
    let dashboard = Dashboard::load(Path::new(DQD_DIR))?;
    assert_eq!(27, dashboard.checks.len());
    assert_eq!(39, dashboard.tables.len());
    assert_eq!(432, dashboard.fields.len());
    Ok(())
}

#[test]
fn every_vendored_record_lowers() -> Result<(), Box<dyn Error>> {
    let catalogue = Catalogue::lower(&Dashboard::load(Path::new(DQD_DIR))?, &model()?)?;
    assert_eq!(27, catalogue.checks.len());
    assert_eq!(39, catalogue.tables.len());
    assert_eq!(432, catalogue.fields.len());
    Ok(())
}

#[test]
fn a_field_record_keeps_its_checks_and_parameters_and_drops_its_prose() -> Result<(), Box<dyn Error>>
{
    let catalogue = Catalogue::lower(&Dashboard::load(Path::new(DQD_DIR))?, &model()?)?;
    let field = catalogue
        .fields
        .iter()
        .find(|field| {
            field.table == "person" && field.field.as_deref() == Some("gender_concept_id")
        })
        .ok_or("no person.gender_concept_id record")?;
    let required = field
        .settings
        .iter()
        .find(|setting| setting.check == "isRequired")
        .ok_or("no isRequired setting")?;
    assert_eq!(Some("Yes"), required.value.as_deref());
    assert_eq!(Some(0), required.threshold);
    assert!(
        field
            .parameters
            .iter()
            .any(|parameter| parameter.name == "fkTableName" && parameter.value == "CONCEPT"),
        "the fkTableName parameter is missing"
    );
    for prose in ["userGuidance", "etlConventions", "databaseSchema"] {
        assert!(
            field
                .parameters
                .iter()
                .all(|parameter| parameter.name != prose),
            "the documentation column {prose} became a parameter"
        );
    }
    Ok(())
}

#[test]
fn the_settings_follow_the_generated_table_order() -> Result<(), Box<dyn Error>> {
    let model = model()?;
    let catalogue = Catalogue::lower(&Dashboard::load(Path::new(DQD_DIR))?, &model)?;
    let tables: Vec<&str> = catalogue.tables.iter().map(|t| t.table.as_str()).collect();
    let order: Vec<&str> = model.tables.iter().map(|t| t.name.as_str()).collect();
    assert_eq!(order, tables);
    Ok(())
}

#[test]
fn an_empty_threshold_is_no_threshold() -> Result<(), Box<dyn Error>> {
    let catalogue = Catalogue::lower(
        &dashboard(&[
            ("cdmTableName", "PERSON"),
            ("cdmFieldName", "person_id"),
            ("isRequired", "Yes"),
            ("isRequiredThreshold", ""),
        ]),
        &model()?,
    )?;
    let field = catalogue.fields.first().ok_or("no field")?;
    let setting = field.settings.first().ok_or("no setting")?;
    assert_eq!(None, setting.threshold);
    Ok(())
}

#[test]
fn a_threshold_of_100_is_accepted() -> Result<(), Box<dyn Error>> {
    let catalogue = Catalogue::lower(
        &dashboard(&[
            ("cdmTableName", "PERSON"),
            ("cdmFieldName", "person_id"),
            ("isRequiredThreshold", "100"),
        ]),
        &model()?,
    )?;
    let field = catalogue.fields.first().ok_or("no field")?;
    let setting = field.settings.first().ok_or("no setting")?;
    assert_eq!(Some(100), setting.threshold);
    Ok(())
}

#[test]
fn a_threshold_above_100_is_refused() -> Result<(), Box<dyn Error>> {
    let result = Catalogue::lower(
        &dashboard(&[
            ("cdmTableName", "PERSON"),
            ("cdmFieldName", "person_id"),
            ("isRequiredThreshold", "101"),
        ]),
        &model()?,
    );
    assert!(
        matches!(result, Err(DqdError::Threshold { .. })),
        "{result:?}"
    );
    Ok(())
}

#[test]
fn a_fractional_threshold_is_refused() -> Result<(), Box<dyn Error>> {
    let result = Catalogue::lower(
        &dashboard(&[
            ("cdmTableName", "PERSON"),
            ("cdmFieldName", "person_id"),
            ("isRequiredThreshold", "5.5"),
        ]),
        &model()?,
    );
    assert!(
        matches!(result, Err(DqdError::Threshold { .. })),
        "{result:?}"
    );
    Ok(())
}

#[test]
fn a_negative_threshold_is_refused() -> Result<(), Box<dyn Error>> {
    let result = Catalogue::lower(
        &dashboard(&[
            ("cdmTableName", "PERSON"),
            ("cdmFieldName", "person_id"),
            ("isRequiredThreshold", "-1"),
        ]),
        &model()?,
    );
    assert!(
        matches!(result, Err(DqdError::Threshold { .. })),
        "{result:?}"
    );
    Ok(())
}

#[test]
fn a_threshold_column_naming_no_check_is_refused() -> Result<(), Box<dyn Error>> {
    let result = Catalogue::lower(
        &dashboard(&[
            ("cdmTableName", "PERSON"),
            ("cdmFieldName", "person_id"),
            ("isMadeUpThreshold", "0"),
        ]),
        &model()?,
    );
    assert!(
        matches!(result, Err(DqdError::UnknownCheck { .. })),
        "{result:?}"
    );
    Ok(())
}

#[test]
fn a_field_the_definitions_lack_is_refused() -> Result<(), Box<dyn Error>> {
    let result = Catalogue::lower(
        &dashboard(&[("cdmTableName", "PERSON"), ("cdmFieldName", "shoe_size")]),
        &model()?,
    );
    assert!(
        matches!(result, Err(DqdError::Unknown { .. })),
        "{result:?}"
    );
    Ok(())
}

#[test]
fn a_field_carried_twice_is_refused() -> Result<(), Box<dyn Error>> {
    let cells = [("cdmTableName", "PERSON"), ("cdmFieldName", "person_id")];
    let mut twice = dashboard(&cells);
    twice.fields.push(record(&cells));
    let result = Catalogue::lower(&twice, &model()?);
    assert!(
        matches!(result, Err(DqdError::Duplicate { .. })),
        "{result:?}"
    );
    Ok(())
}

#[test]
fn a_category_outside_the_kahn_set_is_refused() -> Result<(), Box<dyn Error>> {
    let mut checks = dashboard(&[]);
    checks.fields.clear();
    let mut odd = check("FIELD", "isRequired");
    odd.category = String::from("Correctness");
    checks.checks = vec![odd];
    let result = Catalogue::lower(&checks, &model()?);
    assert!(
        matches!(result, Err(DqdError::Catalogue { .. })),
        "{result:?}"
    );
    Ok(())
}

#[test]
fn a_check_named_twice_is_refused() -> Result<(), Box<dyn Error>> {
    let mut checks = dashboard(&[]);
    checks.fields.clear();
    checks.checks = vec![check("FIELD", "isRequired"), check("FIELD", "isRequired")];
    let result = Catalogue::lower(&checks, &model()?);
    assert!(
        matches!(result, Err(DqdError::DuplicateCheck { .. })),
        "{result:?}"
    );
    Ok(())
}

#[test]
fn a_check_name_becomes_its_static_name() {
    assert_eq!("CDM_TABLE", screaming_snake_case("cdmTable"));
    assert_eq!(
        "IS_STANDARD_VALID_CONCEPT",
        screaming_snake_case("isStandardValidConcept")
    );
}
