// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The seven temporal plausibility checks against PostgreSQL, each on a
//! database seeded to pass it and on one seeded to violate it.
//!
//! The container-backed tests run only when `FERROBRIDGE_E2E=1` admits the
//! harness. Person 1 is born in 1980 and person 2 in 1990, both with no month,
//! day or birth datetime. Every seed is synthetic.

use std::error::Error;

use omop_cdm::quality::check::CheckName;

use super::run::{PERSONS, assert_failed, assert_passed, cdm, counts};

/// Two observation periods of person 1, from 2020-01-01 to `first_end` and
/// from `second_start` to 2020-12-31.
fn periods(first_end: &str, second_start: &str) -> String {
    format!(
        "INSERT INTO observation_period (observation_period_id, person_id, \
         observation_period_start_date, observation_period_end_date, period_type_concept_id) \
         VALUES (1, 1, '2020-01-01', '{first_end}', 32817), \
         (2, 1, '{second_start}', '2020-12-31', 32817)"
    )
}

/// A condition of `person` starting on `start` and ending on `end`, in visit
/// `visit` when one is given.
fn condition(id: u32, person: u32, start: &str, end: &str, visit: Option<u32>) -> String {
    let visit = visit.map_or_else(|| String::from("NULL"), |visit| visit.to_string());
    format!(
        "INSERT INTO condition_occurrence (condition_occurrence_id, person_id, \
         condition_concept_id, condition_start_date, condition_end_date, \
         condition_type_concept_id, visit_occurrence_id) \
         VALUES ({id}, {person}, 0, '{start}', '{end}', 32817, {visit})"
    )
}

/// A visit of `person` from `start` to `end`.
fn visit(id: u32, person: u32, start: &str, end: &str) -> String {
    format!(
        "INSERT INTO visit_occurrence (visit_occurrence_id, person_id, visit_concept_id, \
         visit_start_date, visit_end_date, visit_type_concept_id) \
         VALUES ({id}, {person}, 0, '{start}', '{end}', 32817)"
    )
}

/// Person 1's death on 2020-01-01.
const DEATH: &str = "INSERT INTO death (person_id, death_date, death_type_concept_id) \
    VALUES (1, '2020-01-01', 32817)";

#[tokio::test]
async fn observation_periods_that_meet_or_overlap_violate_the_overlap_check()
-> Result<(), Box<dyn Error>> {
    let Some(cdm) = cdm().await? else {
        return Ok(());
    };
    cdm.execute(PERSONS).await?;
    cdm.execute(&periods("2020-06-30", "2020-07-01")).await?;
    let result = cdm
        .check(
            CheckName::MeasureObservationPeriodOverlap,
            "observation_period",
            None,
        )
        .await?;
    assert_failed(&result, Some(counts(1, 1)));
    Ok(())
}

#[tokio::test]
async fn observation_periods_with_a_gap_pass_the_overlap_check() -> Result<(), Box<dyn Error>> {
    let Some(cdm) = cdm().await? else {
        return Ok(());
    };
    cdm.execute(PERSONS).await?;
    cdm.execute(&periods("2020-06-30", "2020-08-01")).await?;
    let result = cdm
        .check(
            CheckName::MeasureObservationPeriodOverlap,
            "observation_period",
            None,
        )
        .await?;
    assert_passed(&result, counts(0, 1));
    Ok(())
}

#[tokio::test]
async fn a_visit_before_the_first_of_june_of_the_birth_year_violates_temporal_after()
-> Result<(), Box<dyn Error>> {
    let Some(cdm) = cdm().await? else {
        return Ok(());
    };
    cdm.execute(PERSONS).await?;
    cdm.execute(&visit(1, 1, "1980-03-01", "1980-03-02"))
        .await?;
    cdm.execute(&visit(2, 2, "2020-03-01", "2020-03-02"))
        .await?;
    let result = cdm
        .check(
            CheckName::PlausibleTemporalAfter,
            "visit_occurrence",
            Some("visit_start_date"),
        )
        .await?;
    assert_failed(&result, Some(counts(1, 2)));
    Ok(())
}

#[tokio::test]
async fn an_end_after_its_start_in_the_same_table_passes_temporal_after()
-> Result<(), Box<dyn Error>> {
    let Some(cdm) = cdm().await? else {
        return Ok(());
    };
    cdm.execute(PERSONS).await?;
    cdm.execute(&visit(1, 1, "2020-03-01", "2020-03-02"))
        .await?;
    let result = cdm
        .check(
            CheckName::PlausibleTemporalAfter,
            "visit_occurrence",
            Some("visit_end_date"),
        )
        .await?;
    assert_passed(&result, counts(0, 1));
    cdm.execute("UPDATE visit_occurrence SET visit_end_date = '2020-02-28'")
        .await?;
    let result = cdm
        .check(
            CheckName::PlausibleTemporalAfter,
            "visit_occurrence",
            Some("visit_end_date"),
        )
        .await?;
    assert_failed(&result, Some(counts(1, 1)));
    Ok(())
}

#[tokio::test]
async fn a_condition_more_than_sixty_days_after_death_fails_during_life()
-> Result<(), Box<dyn Error>> {
    let Some(cdm) = cdm().await? else {
        return Ok(());
    };
    cdm.execute(PERSONS).await?;
    cdm.execute(DEATH).await?;
    cdm.execute(&condition(1, 1, "2020-06-01", "2020-06-01", None))
        .await?;
    cdm.execute(&condition(2, 2, "2020-06-01", "2020-06-01", None))
        .await?;
    let check = || {
        cdm.check(
            CheckName::PlausibleDuringLife,
            "condition_occurrence",
            Some("condition_start_date"),
        )
    };
    assert_failed(&check().await?, Some(counts(1, 1)));
    cdm.execute(
        "UPDATE condition_occurrence SET condition_start_date = '2020-02-01' \
         WHERE condition_occurrence_id = 1",
    )
    .await?;
    assert_passed(&check().await?, counts(0, 1));
    Ok(())
}

#[tokio::test]
async fn a_condition_a_week_outside_its_visit_fails_within_visit_dates()
-> Result<(), Box<dyn Error>> {
    let Some(cdm) = cdm().await? else {
        return Ok(());
    };
    cdm.execute(PERSONS).await?;
    cdm.execute(&visit(1, 1, "2020-01-10", "2020-01-12"))
        .await?;
    cdm.execute(&condition(1, 1, "2020-01-25", "2020-01-25", Some(1)))
        .await?;
    let check = || {
        cdm.check(
            CheckName::WithinVisitDates,
            "condition_occurrence",
            Some("condition_start_date"),
        )
    };
    assert_failed(&check().await?, Some(counts(1, 1)));
    cdm.execute("UPDATE condition_occurrence SET condition_start_date = '2020-01-19'")
        .await?;
    assert_passed(&check().await?, counts(0, 1));
    Ok(())
}

#[tokio::test]
async fn a_condition_before_the_first_of_january_of_the_birth_year_fails_after_birth()
-> Result<(), Box<dyn Error>> {
    let Some(cdm) = cdm().await? else {
        return Ok(());
    };
    cdm.execute(PERSONS).await?;
    cdm.execute(&condition(1, 1, "1979-12-31", "1979-12-31", None))
        .await?;
    let check = || {
        cdm.check(
            CheckName::PlausibleAfterBirth,
            "condition_occurrence",
            Some("condition_end_date"),
        )
    };
    assert_failed(&check().await?, Some(counts(1, 1)));
    cdm.execute("UPDATE condition_occurrence SET condition_end_date = '1980-01-01'")
        .await?;
    assert_passed(&check().await?, counts(0, 1));
    Ok(())
}

#[tokio::test]
async fn a_person_with_year_of_birth_zero_has_no_birth_date_and_no_error()
-> Result<(), Box<dyn Error>> {
    let Some(cdm) = cdm().await? else {
        return Ok(());
    };
    cdm.execute(
        "INSERT INTO person (person_id, gender_concept_id, year_of_birth, race_concept_id, \
         ethnicity_concept_id) VALUES (3, 0, 0, 0, 0)",
    )
    .await?;
    cdm.execute(&condition(1, 3, "2020-01-01", "2020-01-01", None))
        .await?;
    let result = cdm
        .check(
            CheckName::PlausibleAfterBirth,
            "condition_occurrence",
            Some("condition_end_date"),
        )
        .await?;
    assert_passed(&result, counts(0, 1));
    Ok(())
}

#[tokio::test]
async fn a_condition_ending_sixty_one_days_after_death_fails_before_death()
-> Result<(), Box<dyn Error>> {
    let Some(cdm) = cdm().await? else {
        return Ok(());
    };
    cdm.execute(PERSONS).await?;
    cdm.execute(DEATH).await?;
    cdm.execute(&condition(1, 1, "2020-01-01", "2020-03-02", None))
        .await?;
    let check = || {
        cdm.check(
            CheckName::PlausibleBeforeDeath,
            "condition_occurrence",
            Some("condition_end_date"),
        )
    };
    assert_failed(&check().await?, Some(counts(1, 1)));
    cdm.execute("UPDATE condition_occurrence SET condition_end_date = '2020-03-01'")
        .await?;
    assert_passed(&check().await?, counts(0, 1));
    Ok(())
}

#[tokio::test]
async fn an_era_starting_after_its_end_fails_start_before_end() -> Result<(), Box<dyn Error>> {
    let Some(cdm) = cdm().await? else {
        return Ok(());
    };
    cdm.execute(PERSONS).await?;
    cdm.execute(
        "INSERT INTO condition_era (condition_era_id, person_id, condition_concept_id, \
         condition_era_start_date, condition_era_end_date) \
         VALUES (1, 1, 0, '2020-02-01', '2020-01-01')",
    )
    .await?;
    let check = || {
        cdm.check(
            CheckName::PlausibleStartBeforeEnd,
            "condition_era",
            Some("condition_era_start_date"),
        )
    };
    assert_failed(&check().await?, Some(counts(1, 1)));
    cdm.execute("UPDATE condition_era SET condition_era_end_date = '2020-02-01'")
        .await?;
    assert_passed(&check().await?, counts(0, 1));
    Ok(())
}
