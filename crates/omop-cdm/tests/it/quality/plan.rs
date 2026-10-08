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
        CheckParameters::Domains(vec!["Procedure", "Regimen"]),
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
        .take(9)
        .map(|check| (check.name(), check.column().map(|column| column.name)))
        .collect();
    assert_eq!(
        vec![
            (CheckName::CdmTable, None),
            (CheckName::CdmField, Some("person_id")),
            (CheckName::IsRequired, Some("person_id")),
            (CheckName::CdmDatatype, Some("person_id")),
            (CheckName::IsPrimaryKey, Some("person_id")),
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
