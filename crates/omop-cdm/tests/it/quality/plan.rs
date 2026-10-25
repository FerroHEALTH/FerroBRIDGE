// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Which checks the plan holds: the facts from the CDM v5.4.3 definitions where
//! the Dashboard's settings disagree with them, the thresholds and the
//! enablement from the Dashboard, the default exclusions, and the order.

use std::collections::BTreeSet;
use std::error::Error;

use omop_cdm::quality::check::{CheckName, CheckParameters};
use omop_cdm::quality::definitions::Percentage;
use omop_cdm::quality::plan::{Plan, PlanError, PlanOptions};

use super::{find, planned};

/// The default plan.
fn plan() -> Result<Plan, Box<dyn Error>> {
    Ok(Plan::new(&PlanOptions::default())?)
}

#[test]
fn a_foreign_key_only_the_definitions_declare_is_checked() -> Result<(), Box<dyn Error>> {
    let check = planned(
        CheckName::IsForeignKey,
        "episode",
        Some("episode_parent_id"),
    )?;
    assert_eq!(
        CheckParameters::ForeignKey {
            table: "episode",
            field: "episode_id"
        },
        check.parameters()
    );
    Ok(())
}

#[test]
fn a_primary_key_only_the_dashboard_declares_is_not_checked() -> Result<(), Box<dyn Error>> {
    assert!(
        find(
            &plan()?,
            CheckName::IsPrimaryKey,
            "death",
            Some("person_id")
        )
        .is_none()
    );
    Ok(())
}

#[test]
fn a_two_domain_definition_is_checked_against_both_domains() -> Result<(), Box<dyn Error>> {
    let check = planned(
        CheckName::FkDomain,
        "episode",
        Some("episode_object_concept_id"),
    )?;
    assert_eq!(
        CheckParameters::Domains(&["Procedure", "Regimen"]),
        check.parameters()
    );
    Ok(())
}

#[test]
fn a_domain_only_the_dashboard_names_is_not_checked() -> Result<(), Box<dyn Error>> {
    let plan = plan()?;
    assert!(
        find(
            &plan,
            CheckName::FkDomain,
            "measurement",
            Some("operator_concept_id")
        )
        .is_none()
    );
    Ok(())
}

#[test]
fn the_capital_i_integer_field_is_datatype_checked() -> Result<(), Box<dyn Error>> {
    planned(
        CheckName::CdmDatatype,
        "visit_occurrence",
        Some("visit_type_concept_id"),
    )?;
    Ok(())
}

#[test]
fn a_non_integer_field_is_not_datatype_checked() -> Result<(), Box<dyn Error>> {
    let plan = plan()?;
    assert!(
        find(
            &plan,
            CheckName::CdmDatatype,
            "person",
            Some("person_source_value")
        )
        .is_none()
    );
    Ok(())
}

#[test]
fn the_offset_column_is_field_checked() -> Result<(), Box<dyn Error>> {
    planned(CheckName::CdmField, "note_nlp", Some("offset"))?;
    Ok(())
}

#[test]
fn a_class_the_definitions_name_is_checked() -> Result<(), Box<dyn Error>> {
    let check = planned(CheckName::FkClass, "drug_era", Some("drug_concept_id"))?;
    assert_eq!(
        CheckParameters::ConceptClass("Ingredient"),
        check.parameters()
    );
    Ok(())
}

#[test]
fn a_class_only_the_dashboard_names_is_not_checked() -> Result<(), Box<dyn Error>> {
    let plan = plan()?;
    assert!(
        find(
            &plan,
            CheckName::FkClass,
            "drug_strength",
            Some("ingredient_concept_id")
        )
        .is_none()
    );
    Ok(())
}

#[test]
fn an_optional_field_is_not_required_checked() -> Result<(), Box<dyn Error>> {
    let plan = plan()?;
    assert!(
        find(
            &plan,
            CheckName::IsRequired,
            "person",
            Some("month_of_birth")
        )
        .is_none()
    );
    planned(CheckName::IsRequired, "person", Some("year_of_birth"))?;
    Ok(())
}

#[test]
fn standard_concepts_are_checked_where_the_dashboard_enables_them() -> Result<(), Box<dyn Error>> {
    let plan = plan()?;
    assert!(
        find(
            &plan,
            CheckName::IsStandardValidConcept,
            "person",
            Some("gender_concept_id")
        )
        .is_some()
    );
    assert!(
        find(
            &plan,
            CheckName::IsStandardValidConcept,
            "person",
            Some("gender_source_concept_id")
        )
        .is_none()
    );
    Ok(())
}

#[test]
fn the_vocabulary_tables_are_excluded_by_default() -> Result<(), Box<dyn Error>> {
    let plan = plan()?;
    for table in [
        "concept",
        "vocabulary",
        "concept_ancestor",
        "concept_relationship",
        "concept_class",
        "concept_synonym",
        "relationship",
        "domain",
    ] {
        assert!(
            plan.checks()
                .iter()
                .all(|check| check.table().name != table),
            "the default plan checks the excluded {table} table"
        );
    }
    assert_eq!(
        31,
        plan.checks()
            .iter()
            .filter(|check| check.name() == CheckName::CdmTable)
            .count(),
        "every table but the eight excluded ones gets cdmTable"
    );
    Ok(())
}

#[test]
fn an_empty_exclusion_list_checks_the_vocabulary_tables() -> Result<(), Box<dyn Error>> {
    let plan = Plan::new(&PlanOptions {
        excluded_tables: BTreeSet::new(),
    })?;
    assert!(find(&plan, CheckName::CdmTable, "concept", None).is_some());
    Ok(())
}

#[test]
fn an_excluded_table_is_matched_ignoring_case() -> Result<(), Box<dyn Error>> {
    let plan = Plan::new(&PlanOptions {
        excluded_tables: BTreeSet::from([String::from("PERSON")]),
    })?;
    assert!(find(&plan, CheckName::CdmTable, "person", None).is_none());
    assert!(find(&plan, CheckName::CdmTable, "concept", None).is_some());
    Ok(())
}

#[test]
fn an_unknown_excluded_table_is_refused() {
    let result = Plan::new(&PlanOptions {
        excluded_tables: BTreeSet::from([String::from("pack_content")]),
    });
    assert_eq!(
        Err(PlanError::UnknownTable {
            table: String::from("pack_content")
        }),
        result
    );
}

#[test]
fn the_thresholds_come_from_the_dashboard() -> Result<(), Box<dyn Error>> {
    let check = planned(CheckName::IsRequired, "person", Some("gender_concept_id"))?;
    assert_eq!(Percentage::new(0), check.threshold());
    Ok(())
}

#[test]
fn a_check_without_a_threshold_column_has_no_threshold() -> Result<(), Box<dyn Error>> {
    assert_eq!(
        None,
        planned(CheckName::CdmTable, "person", None)?.threshold()
    );
    assert_eq!(
        None,
        planned(CheckName::CdmField, "person", Some("person_id"))?.threshold()
    );
    Ok(())
}

#[test]
fn a_disabled_dashboard_setting_carries_no_threshold() -> Result<(), Box<dyn Error>> {
    // NOTE: DQD v2.9.0 `OMOP_CDMv5.4_Field_Level.csv` marks episode.episode_parent_id
    // isForeignKey `No` with no threshold, so the definitions' key runs without one.
    let check = planned(
        CheckName::IsForeignKey,
        "episode",
        Some("episode_parent_id"),
    )?;
    assert_eq!(None, check.threshold());
    Ok(())
}

#[test]
fn the_plan_runs_in_table_then_column_then_catalogue_order() -> Result<(), Box<dyn Error>> {
    let plan = plan()?;
    let head: Vec<(CheckName, Option<&str>)> = plan
        .checks()
        .iter()
        .take(10)
        .map(|check| (check.name(), check.column().map(|column| column.name)))
        .collect();
    assert_eq!(
        vec![
            (CheckName::CdmTable, None),
            (CheckName::CdmField, Some("person_id")),
            (CheckName::IsRequired, Some("person_id")),
            (CheckName::CdmDatatype, Some("person_id")),
            (CheckName::IsPrimaryKey, Some("person_id")),
            (CheckName::MeasureValueCompleteness, Some("person_id")),
            (CheckName::CdmField, Some("gender_concept_id")),
            (CheckName::IsRequired, Some("gender_concept_id")),
            (CheckName::CdmDatatype, Some("gender_concept_id")),
            (CheckName::IsForeignKey, Some("gender_concept_id")),
        ],
        head
    );
    Ok(())
}

#[test]
fn every_planned_check_id_is_unique() -> Result<(), Box<dyn Error>> {
    let plan = plan()?;
    let ids: BTreeSet<String> = plan
        .checks()
        .iter()
        .map(omop_cdm::quality::check::Check::id)
        .collect();
    assert_eq!(plan.checks().len(), ids.len());
    Ok(())
}

#[test]
fn person_completeness_runs_where_the_table_settings_enable_it() -> Result<(), Box<dyn Error>> {
    let check = planned(
        CheckName::MeasurePersonCompleteness,
        "visit_occurrence",
        None,
    )?;
    assert_eq!(Percentage::new(95), check.threshold());
    assert!(
        find(
            &plan()?,
            CheckName::MeasurePersonCompleteness,
            "person",
            None
        )
        .is_none(),
        "the person table's setting says No"
    );
    Ok(())
}

#[test]
fn condition_era_completeness_runs_on_the_condition_era_alone() -> Result<(), Box<dyn Error>> {
    let plan = plan()?;
    let tables: Vec<&str> = plan
        .checks()
        .iter()
        .filter(|check| check.name() == CheckName::MeasureConditionEraCompleteness)
        .map(|check| check.table().name)
        .collect();
    assert_eq!(vec!["condition_era"], tables);
    Ok(())
}

#[test]
fn a_concept_record_check_names_the_source_value_its_template_names() -> Result<(), Box<dyn Error>>
{
    let route = planned(
        CheckName::StandardConceptRecordCompleteness,
        "drug_exposure",
        Some("route_concept_id"),
    )?;
    assert_eq!(
        CheckParameters::SourceValue(Some("route_source_value")),
        route.parameters()
    );
    assert_eq!(Percentage::new(100), route.threshold());
    let condition = planned(
        CheckName::StandardConceptRecordCompleteness,
        "condition_occurrence",
        Some("condition_concept_id"),
    )?;
    assert_eq!(CheckParameters::SourceValue(None), condition.parameters());
    Ok(())
}

#[test]
fn the_dose_era_unit_is_not_paired_with_a_source_value() -> Result<(), Box<dyn Error>> {
    let plan = plan()?;
    for check in plan.checks().iter().filter(|check| {
        check.table().name == "dose_era"
            && matches!(
                check.name(),
                CheckName::StandardConceptRecordCompleteness
                    | CheckName::SourceConceptRecordCompleteness
            )
    }) {
        assert_eq!(
            CheckParameters::SourceValue(None),
            check.parameters(),
            "{}",
            check.id()
        );
    }
    Ok(())
}

#[test]
fn a_source_value_check_maps_through_its_standard_concept_field() -> Result<(), Box<dyn Error>> {
    let check = planned(
        CheckName::SourceValueCompleteness,
        "person",
        Some("gender_source_value"),
    )?;
    assert_eq!(
        CheckParameters::StandardConceptField("gender_concept_id"),
        check.parameters()
    );
    Ok(())
}

#[test]
fn every_completeness_check_the_settings_enable_is_planned() -> Result<(), Box<dyn Error>> {
    let plan = plan()?;
    let completeness = [
        CheckName::MeasureValueCompleteness,
        CheckName::StandardConceptRecordCompleteness,
        CheckName::SourceConceptRecordCompleteness,
        CheckName::SourceValueCompleteness,
    ];
    let excluded = PlanOptions::default().excluded_tables;
    for field in omop_cdm::generated::dqd::FIELD_THRESHOLDS {
        if excluded.contains(field.table) {
            continue;
        }
        for name in completeness {
            let enabled = field
                .setting(name.as_str())
                .is_some_and(|setting| setting.value == Some("Yes"));
            assert_eq!(
                enabled,
                find(&plan, name, field.table, Some(field.field)).is_some(),
                "{} on {}.{}",
                name.as_str(),
                field.table,
                field.field
            );
        }
    }
    Ok(())
}

#[test]
fn every_temporal_check_the_settings_enable_is_planned() -> Result<(), Box<dyn Error>> {
    let plan = plan()?;
    let temporal = [
        CheckName::PlausibleTemporalAfter,
        CheckName::PlausibleDuringLife,
        CheckName::WithinVisitDates,
        CheckName::PlausibleAfterBirth,
        CheckName::PlausibleBeforeDeath,
        CheckName::PlausibleStartBeforeEnd,
    ];
    let excluded = PlanOptions::default().excluded_tables;
    for field in omop_cdm::generated::dqd::FIELD_THRESHOLDS {
        if excluded.contains(field.table) {
            continue;
        }
        for name in temporal {
            let enabled = field
                .setting(name.as_str())
                .is_some_and(|setting| setting.value == Some("Yes"));
            assert_eq!(
                enabled,
                find(&plan, name, field.table, Some(field.field)).is_some(),
                "{} on {}.{}",
                name.as_str(),
                field.table,
                field.field
            );
        }
    }
    assert!(
        find(
            &plan,
            CheckName::MeasureObservationPeriodOverlap,
            "observation_period",
            None
        )
        .is_some()
    );
    Ok(())
}

#[test]
fn temporal_after_reads_the_earlier_date_from_person_or_from_its_own_table()
-> Result<(), Box<dyn Error>> {
    let plan = plan()?;
    let after = |table: &str, field: &str| {
        find(&plan, CheckName::PlausibleTemporalAfter, table, Some(field))
            .map(|check| check.parameters())
    };
    assert_eq!(
        Some(CheckParameters::TemporalAfter {
            table: "person",
            field: "birth_datetime"
        }),
        after("measurement", "measurement_date")
    );
    assert_eq!(
        Some(CheckParameters::TemporalAfter {
            table: "visit_occurrence",
            field: "visit_start_date"
        }),
        after("visit_occurrence", "visit_end_date")
    );
    let check = find(
        &plan,
        CheckName::PlausibleTemporalAfter,
        "measurement",
        Some("measurement_date"),
    )
    .ok_or("a planned check")?;
    assert_eq!(
        "The number and percent of records with a value in the MEASUREMENT_DATE field of the \
         MEASUREMENT that occurs prior to the date in the BIRTH_DATETIME field of the PERSON table.",
        check.description()
    );
    Ok(())
}

#[test]
fn start_before_end_names_its_end_field() -> Result<(), Box<dyn Error>> {
    let plan = plan()?;
    let check = find(
        &plan,
        CheckName::PlausibleStartBeforeEnd,
        "condition_era",
        Some("condition_era_start_date"),
    )
    .ok_or("a planned check")?;
    assert_eq!(
        CheckParameters::EndField("condition_era_end_date"),
        check.parameters()
    );
    assert!(
        check
            .description()
            .ends_with("in the CONDITION_ERA_END_DATE."),
        "{}",
        check.description()
    );
    Ok(())
}
