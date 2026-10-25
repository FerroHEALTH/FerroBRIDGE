// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The not-applicable status (DQD v2.9.0 `R/calculateNotApplicableStatus.R`)
//! over results built from planned checks and synthetic counts.

use std::error::Error;

use omop_cdm::quality::applicable;
use omop_cdm::quality::check::CheckName;
use omop_cdm::quality::evaluate::{self, CheckError, CheckResult, Counts, Outcome};

use super::planned;

/// The result of the default plan's `name` check on `table` and `field`,
/// counted `violated` of `denominator`.
fn counted(
    name: CheckName,
    table: &str,
    field: Option<&str>,
    violated: u64,
    denominator: u64,
) -> Result<CheckResult, Box<dyn Error>> {
    let check = planned(name, table, field)?;
    let counts = Counts {
        violated,
        denominator,
    };
    Ok(evaluate::evaluate(&check, Outcome::Counted(counts), None))
}

/// The probe results of a present `table` and its `field`.
fn present(table: &str, field: &str) -> Result<Vec<CheckResult>, Box<dyn Error>> {
    Ok(vec![
        counted(CheckName::CdmTable, table, None, 0, 1)?,
        counted(CheckName::CdmField, table, Some(field), 0, 1)?,
    ])
}

#[test]
fn a_check_of_an_empty_table_does_not_apply() -> Result<(), Box<dyn Error>> {
    let mut results = present("drug_exposure", "route_concept_id")?;
    results.push(counted(
        CheckName::MeasureValueCompleteness,
        "drug_exposure",
        Some("route_concept_id"),
        0,
        0,
    )?);
    results.push(counted(
        CheckName::StandardConceptRecordCompleteness,
        "drug_exposure",
        Some("route_concept_id"),
        0,
        0,
    )?);
    applicable::apply(&mut results);
    let record = results.last().ok_or("a result")?;
    assert!(record.not_applicable);
    assert!(!record.passed && !record.failed);
    assert_eq!(
        Some("Table DRUG_EXPOSURE is empty."),
        record.not_applicable_reason.as_deref()
    );
    let table = results.first().ok_or("a result")?;
    assert!(
        !table.not_applicable && table.passed,
        "cdmTable always applies"
    );
    Ok(())
}

#[test]
fn a_check_of_an_unpopulated_field_does_not_apply_but_its_value_completeness_does()
-> Result<(), Box<dyn Error>> {
    let mut results = present("drug_exposure", "route_concept_id")?;
    results.push(counted(
        CheckName::MeasureValueCompleteness,
        "drug_exposure",
        Some("route_concept_id"),
        4,
        4,
    )?);
    results.push(counted(
        CheckName::StandardConceptRecordCompleteness,
        "drug_exposure",
        Some("route_concept_id"),
        4,
        4,
    )?);
    applicable::apply(&mut results);
    let value = results.get(2).ok_or("a result")?;
    assert!(
        !value.not_applicable,
        "measureValueCompleteness still applies"
    );
    let record = results.get(3).ok_or("a result")?;
    assert!(record.not_applicable);
    assert!(!record.failed, "a check that does not apply is not failed");
    assert_eq!(
        Some("Field DRUG_EXPOSURE.ROUTE_CONCEPT_ID is not populated."),
        record.not_applicable_reason.as_deref()
    );
    Ok(())
}

#[test]
fn a_check_of_a_missing_table_does_not_apply_and_names_it() -> Result<(), Box<dyn Error>> {
    let table = planned(CheckName::CdmTable, "visit_occurrence", None)?;
    let missing = evaluate::evaluate(&table, Outcome::Absent(CheckError::NoRow), None);
    let mut results = vec![
        missing,
        counted(
            CheckName::CdmField,
            "visit_occurrence",
            Some("visit_concept_id"),
            0,
            1,
        )?,
        counted(
            CheckName::MeasureValueCompleteness,
            "person",
            Some("year_of_birth"),
            0,
            3,
        )?,
        counted(
            CheckName::MeasurePersonCompleteness,
            "visit_occurrence",
            None,
            3,
            3,
        )?,
    ];
    applicable::apply(&mut results);
    let table = results.first().ok_or("a result")?;
    assert!(
        table.failed && !table.not_applicable,
        "a missing table fails cdmTable"
    );
    let field = results.get(1).ok_or("a result")?;
    assert!(field.not_applicable);
    let person = results.get(3).ok_or("a result")?;
    assert!(person.not_applicable);
    assert_eq!(
        Some("Table VISIT_OCCURRENCE does not exist."),
        person.not_applicable_reason.as_deref()
    );
    Ok(())
}

#[test]
fn condition_era_completeness_does_not_apply_without_conditions() -> Result<(), Box<dyn Error>> {
    let mut results = present("condition_occurrence", "condition_concept_id")?;
    results.push(counted(
        CheckName::MeasureValueCompleteness,
        "condition_occurrence",
        Some("condition_concept_id"),
        0,
        0,
    )?);
    results.push(counted(
        CheckName::MeasureConditionEraCompleteness,
        "condition_era",
        None,
        0,
        0,
    )?);
    applicable::apply(&mut results);
    let era = results.last().ok_or("a result")?;
    assert!(era.not_applicable);
    assert_eq!(
        Some("Table CONDITION_OCCURRENCE is empty."),
        era.not_applicable_reason.as_deref()
    );
    Ok(())
}

#[test]
fn an_error_result_is_never_marked_not_applicable_on_a_present_table() -> Result<(), Box<dyn Error>>
{
    let mut results = present("drug_exposure", "route_concept_id")?;
    results.push(counted(
        CheckName::MeasureValueCompleteness,
        "drug_exposure",
        Some("route_concept_id"),
        4,
        4,
    )?);
    let record = planned(
        CheckName::StandardConceptRecordCompleteness,
        "drug_exposure",
        Some("route_concept_id"),
    )?;
    results.push(evaluate::evaluate(
        &record,
        Outcome::Failed(CheckError::NoRow),
        None,
    ));
    applicable::apply(&mut results);
    let erred = results.last().ok_or("a result")?;
    assert!(erred.is_error && !erred.not_applicable && !erred.passed);
    Ok(())
}

#[test]
fn without_the_three_checks_the_rules_are_not_applied() -> Result<(), Box<dyn Error>> {
    let mut results = vec![counted(
        CheckName::StandardConceptRecordCompleteness,
        "drug_exposure",
        Some("route_concept_id"),
        0,
        0,
    )?];
    applicable::apply(&mut results);
    let only = results.first().ok_or("a result")?;
    assert!(!only.not_applicable && only.passed);
    Ok(())
}
