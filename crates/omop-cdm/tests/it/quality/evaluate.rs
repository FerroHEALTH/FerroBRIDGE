// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The verdict (DQD v2.9.0 `R/evaluateThresholds.R`), the check id
//! (`R/getCheckId.R`), the rendered description (`R/recordResult.R`), the
//! query text, and the run summary (`R/summarizeResults.R`).

use std::error::Error;

use omop_cdm::quality::check::CheckName;
use omop_cdm::quality::definitions::Percentage;
use omop_cdm::quality::definitions::{CheckLevel, KahnCategory, KahnContext, KahnSubcategory};
use omop_cdm::quality::evaluate::{self, CheckError, Counts, Outcome, Summary};

use super::planned;

/// The counts `violated` of `denominator`.
fn counts(violated: u64, denominator: u64) -> Counts {
    Counts {
        violated,
        denominator,
    }
}

#[test]
fn without_a_threshold_no_violation_passes() {
    assert!(!counts(0, 10).exceed(None));
}

#[test]
fn without_a_threshold_one_violation_fails() {
    assert!(counts(1, 1_000_000).exceed(None));
}

#[test]
fn a_zero_threshold_fails_on_one_violation() {
    assert!(counts(1, 1_000_000).exceed(Percentage::new(0)));
    assert!(!counts(0, 10).exceed(Percentage::new(0)));
}

#[test]
fn a_percentage_equal_to_the_threshold_passes() {
    assert!(!counts(5, 100).exceed(Percentage::new(5)));
}

#[test]
fn a_percentage_above_the_threshold_fails() {
    assert!(counts(6, 100).exceed(Percentage::new(5)));
}

#[test]
fn the_comparison_is_exact_just_above_the_threshold() {
    // 1 of 19 is 5.26 %, 1 of 20 is exactly 5 %.
    assert!(counts(1, 19).exceed(Percentage::new(5)));
    assert!(!counts(1, 20).exceed(Percentage::new(5)));
}

#[test]
fn the_comparison_does_not_overflow_at_the_largest_counts() {
    assert!(!counts(u64::MAX, u64::MAX).exceed(Percentage::new(100)));
    assert!(counts(u64::MAX, u64::MAX).exceed(Percentage::new(99)));
}

#[test]
fn a_zero_denominator_is_zero_percent() {
    assert!(!counts(0, 0).exceed(Percentage::new(5)));
    assert!(counts(0, 0).fraction().abs() < f64::EPSILON);
}

#[test]
fn the_fraction_is_violated_over_denominator() {
    assert!((counts(1, 4).fraction() - 0.25).abs() < f64::EPSILON);
}

#[test]
fn counts_within_the_threshold_pass() -> Result<(), Box<dyn Error>> {
    let check = planned(CheckName::IsRequired, "person", Some("year_of_birth"))?;
    let result = evaluate::evaluate(&check, Outcome::Counted(counts(0, 3)), None);
    assert!(result.passed);
    assert!(!result.failed);
    assert!(!result.is_error);
    assert_eq!(Some(counts(0, 3)), result.counts);
    Ok(())
}

#[test]
fn counts_above_the_threshold_fail() -> Result<(), Box<dyn Error>> {
    let check = planned(CheckName::IsRequired, "person", Some("year_of_birth"))?;
    let result = evaluate::evaluate(&check, Outcome::Counted(counts(1, 3)), None);
    assert!(result.failed);
    assert!(!result.passed);
    assert!(!result.is_error);
    Ok(())
}

#[test]
fn an_error_never_passes() -> Result<(), Box<dyn Error>> {
    let check = planned(CheckName::IsRequired, "person", Some("year_of_birth"))?;
    let result = evaluate::evaluate(&check, Outcome::Failed(CheckError::NoRow), None);
    assert!(result.is_error);
    assert!(!result.passed);
    assert!(
        !result.failed,
        "the Dashboard records an error as not failed"
    );
    assert_eq!(None, result.counts);
    assert!(matches!(result.error, Some(CheckError::NoRow)));
    Ok(())
}

#[test]
fn a_missing_table_fails_the_table_probe_without_error() -> Result<(), Box<dyn Error>> {
    let check = planned(CheckName::CdmTable, "person", None)?;
    let result = evaluate::evaluate(&check, Outcome::Absent(CheckError::NoRow), None);
    assert!(result.failed);
    assert!(!result.is_error);
    assert!(!result.passed);
    assert!(
        result.error.is_some(),
        "the database's error stays on the result"
    );
    Ok(())
}

#[test]
fn a_missing_column_fails_the_field_probe_without_error() -> Result<(), Box<dyn Error>> {
    let check = planned(CheckName::CdmField, "person", Some("year_of_birth"))?;
    let result = evaluate::evaluate(&check, Outcome::Absent(CheckError::NoRow), None);
    assert!(result.failed);
    assert!(!result.is_error);
    Ok(())
}

#[test]
fn a_missing_table_is_an_error_for_a_counting_check() -> Result<(), Box<dyn Error>> {
    let check = planned(CheckName::IsRequired, "person", Some("year_of_birth"))?;
    let result = evaluate::evaluate(&check, Outcome::Absent(CheckError::NoRow), None);
    assert!(result.is_error);
    assert!(!result.failed);
    assert!(!result.passed);
    Ok(())
}

#[test]
fn not_applicable_is_never_set_by_a_conformance_check() -> Result<(), Box<dyn Error>> {
    let check = planned(CheckName::IsRequired, "person", Some("year_of_birth"))?;
    let result = evaluate::evaluate(&check, Outcome::Counted(counts(0, 0)), None);
    assert!(!result.not_applicable);
    assert_eq!(None, result.not_applicable_reason);
    Ok(())
}

#[test]
fn a_result_carries_the_catalogue_fields() -> Result<(), Box<dyn Error>> {
    let check = planned(CheckName::FkDomain, "person", Some("gender_concept_id"))?;
    let result = evaluate::evaluate(&check, Outcome::Counted(counts(0, 1)), None);
    assert_eq!(CheckName::FkDomain, result.check_name);
    assert_eq!(CheckLevel::Field, result.check_level);
    assert_eq!("PERSON", result.cdm_table_name);
    assert_eq!(Some("GENDER_CONCEPT_ID"), result.cdm_field_name.as_deref());
    assert_eq!("field_fk_domain.sql", result.sql_file);
    assert_eq!(KahnCategory::Conformance, result.category);
    assert_eq!(Some(KahnSubcategory::Value), result.subcategory);
    assert_eq!(KahnContext::Verification, result.context);
    assert_eq!(check.sql(), result.query_text);
    assert_eq!(Percentage::new(0), result.threshold_value);
    Ok(())
}

#[test]
fn a_field_check_id_joins_level_name_table_and_field() -> Result<(), Box<dyn Error>> {
    // NOTE: DQD v2.9.0 `R/getCheckId.R` lines 38 to 49 over FIELD, isRequired,
    // PERSON and PERSON_ID, the upper-cased names `R/executeDqChecks.R` passes.
    let check = planned(CheckName::IsRequired, "person", Some("person_id"))?;
    assert_eq!("field_isrequired_person_person_id", check.id());
    Ok(())
}

#[test]
fn a_table_check_id_leaves_the_field_out() -> Result<(), Box<dyn Error>> {
    let check = planned(CheckName::CdmTable, "visit_occurrence", None)?;
    assert_eq!("table_cdmtable_visit_occurrence", check.id());
    Ok(())
}

#[test]
fn a_table_description_names_the_table_in_upper_case() -> Result<(), Box<dyn Error>> {
    let check = planned(CheckName::CdmTable, "person", None)?;
    assert_eq!(
        "A yes or no value indicating if PERSON table is present as expected based on the specification. ",
        check.description()
    );
    Ok(())
}

#[test]
fn a_foreign_key_description_names_the_referenced_table() -> Result<(), Box<dyn Error>> {
    let check = planned(CheckName::IsForeignKey, "person", Some("gender_concept_id"))?;
    assert_eq!(
        "The number and percent of records that have a value in the GENDER_CONCEPT_ID field in \
         the PERSON table that does not exist in the CONCEPT table.",
        check.description()
    );
    Ok(())
}

#[test]
fn a_domain_description_names_every_domain() -> Result<(), Box<dyn Error>> {
    let check = planned(
        CheckName::FkDomain,
        "episode",
        Some("episode_object_concept_id"),
    )?;
    assert_eq!(
        "The number and percent of records that have a value in the EPISODE_OBJECT_CONCEPT_ID \
         field in the EPISODE table that do not conform to the PROCEDURE, REGIMEN domain.",
        check.description()
    );
    Ok(())
}

#[test]
fn a_class_description_names_the_class() -> Result<(), Box<dyn Error>> {
    let check = planned(CheckName::FkClass, "drug_era", Some("drug_concept_id"))?;
    assert_eq!(
        "The number and percent of records that have a value in the DRUG_CONCEPT_ID field in \
         the DRUG_ERA table that do not conform to the INGREDIENT class.",
        check.description()
    );
    Ok(())
}

#[test]
fn the_offset_column_is_quoted_in_its_query() -> Result<(), Box<dyn Error>> {
    let check = planned(CheckName::CdmField, "note_nlp", Some("offset"))?;
    assert!(
        check.sql().contains("cdm_table.\"offset\""),
        "{}",
        check.sql()
    );
    Ok(())
}

#[test]
fn a_two_domain_query_lists_both_domains() -> Result<(), Box<dyn Error>> {
    let check = planned(
        CheckName::FkDomain,
        "episode",
        Some("episode_object_concept_id"),
    )?;
    assert!(
        check.sql().contains("NOT IN ('Procedure', 'Regimen')"),
        "{}",
        check.sql()
    );
    Ok(())
}

#[test]
fn the_summary_counts_a_pass_from_the_passed_flag() -> Result<(), Box<dyn Error>> {
    let check = planned(CheckName::IsRequired, "person", Some("year_of_birth"))?;
    let results = [
        evaluate::evaluate(&check, Outcome::Counted(counts(0, 1)), None),
        evaluate::evaluate(&check, Outcome::Counted(counts(1, 1)), None),
        evaluate::evaluate(&check, Outcome::Failed(CheckError::NoRow), None),
    ];
    let summary = Summary::new(&results);
    assert_eq!(3, summary.count_total);
    assert_eq!(1, summary.count_passed, "an error is not a pass");
    assert_eq!(1, summary.count_error_failed);
    assert_eq!(1, summary.count_threshold_failed);
    assert_eq!(1, summary.count_overall_failed);
    assert_eq!(3, summary.conformance.total);
    assert_eq!(1, summary.conformance.passed);
    assert_eq!(1, summary.conformance.failed);
    assert_eq!(0, summary.completeness.total);
    let percent = summary.percent_passed().ok_or("no percentage")?;
    assert!((percent - 50.0).abs() < f64::EPSILON, "{percent}");
    Ok(())
}

#[test]
fn an_empty_summary_has_no_percentage() {
    let summary = Summary::new(&[]);
    assert_eq!(None, summary.percent_passed());
    assert_eq!(None, summary.percent_failed());
}
