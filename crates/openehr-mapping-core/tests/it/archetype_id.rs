// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The `spec.openEhrConfig.archetype` value: one case per form the header
//! accepts or refuses, against the `archetype_id` production of openEHR BASE
//! Release 1.2.0 (Base Types syntax appendix), which FHIRconnect v1.0.0
//! (Header §Spec) writes as `openEHR-EHR-ACTION.informed_consent.v0`.

use core::str::FromStr;

use openehr_base::v1_2::base_types::identification::lexical::IdError;
use openehr_mapping_core::header::archetype::ArchetypeId;
use openehr_mapping_core::header::archetype::ArchetypeIdError;

/// Returns the refusal the BASE 1.2.0 production gives `value`.
fn lexical_refusal(value: &str) -> Result<ArchetypeId, ArchetypeIdError> {
    Err(ArchetypeIdError::Lexical {
        value: value.to_owned(),
        source: IdError::Archetype(value.to_owned()),
    })
}

#[test]
fn the_header_example_of_the_specification_is_accepted() -> Result<(), ArchetypeIdError> {
    let id = ArchetypeId::from_str("openEHR-EHR-ACTION.informed_consent.v0")?;
    assert_eq!(id.as_str(), "openEHR-EHR-ACTION.informed_consent.v0");
    assert_eq!(id.version_number(), 0);
    assert_eq!(id.base().value, "openEHR-EHR-ACTION.informed_consent.v0");
    Ok(())
}

#[test]
fn a_version_above_zero_is_accepted() -> Result<(), ArchetypeIdError> {
    let id = ArchetypeId::from_str("openEHR-EHR-EVALUATION.problem_diagnosis.v1")?;
    assert_eq!(id.version_number(), 1);
    Ok(())
}

#[test]
fn a_version_of_several_digits_is_accepted() -> Result<(), ArchetypeIdError> {
    let id = ArchetypeId::from_str("openEHR-EHR-OBSERVATION.blood_pressure.v12")?;
    assert_eq!(id.version_number(), 12);
    Ok(())
}

#[test]
fn a_specialised_concept_is_accepted() -> Result<(), ArchetypeIdError> {
    let id = ArchetypeId::from_str("openEHR-EHR-CLUSTER.imaging_exam-foetus.v1")?;
    assert_eq!(id.base().specialisation(), "foetus");
    Ok(())
}

#[test]
fn a_concept_specialised_twice_is_accepted() -> Result<(), ArchetypeIdError> {
    let id = ArchetypeId::from_str("openEHR-EHR-CLUSTER.device-pump-infusion.v1")?;
    assert_eq!(id.base().domain_concept(), "device-pump-infusion");
    Ok(())
}

#[test]
fn digits_and_underscores_after_the_first_letter_are_accepted() -> Result<(), ArchetypeIdError> {
    let id = ArchetypeId::from_str("org_x-EHR2-OBSERVATION.lab_test2.v3")?;
    assert_eq!(id.base().rm_originator(), "org_x");
    Ok(())
}

#[test]
fn the_display_form_is_the_value() -> Result<(), ArchetypeIdError> {
    let value = "openEHR-EHR-SECTION.adhoc.v1";
    assert_eq!(ArchetypeId::from_str(value)?.to_string(), value);
    Ok(())
}

#[test]
fn an_empty_value_is_refused() {
    assert_eq!(
        ArchetypeId::from_str(""),
        Err(ArchetypeIdError::Lexical {
            value: String::new(),
            source: IdError::Empty,
        })
    );
}

#[test]
fn a_value_without_a_version_is_refused() {
    let value = "openEHR-EHR-EVALUATION.problem_diagnosis";
    assert_eq!(ArchetypeId::from_str(value), lexical_refusal(value));
}

#[test]
fn a_version_without_its_v_is_refused() {
    let value = "openEHR-EHR-EVALUATION.problem_diagnosis.1";
    assert_eq!(ArchetypeId::from_str(value), lexical_refusal(value));
}

#[test]
fn a_version_with_a_leading_zero_is_refused() {
    let value = "openEHR-EHR-EVALUATION.problem_diagnosis.v01";
    assert_eq!(ArchetypeId::from_str(value), lexical_refusal(value));
}

#[test]
fn a_version_with_a_lifecycle_suffix_is_refused() {
    let value = "openEHR-EHR-EVALUATION.problem_diagnosis.v1draft";
    assert_eq!(ArchetypeId::from_str(value), lexical_refusal(value));
}

#[test]
fn a_full_version_is_refused() {
    let value = "openEHR-EHR-EVALUATION.problem_diagnosis.v1.0.0";
    assert_eq!(ArchetypeId::from_str(value), lexical_refusal(value));
}

#[test]
fn a_qualified_rm_entity_of_two_parts_is_refused() {
    let value = "openEHR-EHR.problem_diagnosis.v1";
    assert_eq!(ArchetypeId::from_str(value), lexical_refusal(value));
}

#[test]
fn a_value_without_its_qualified_rm_entity_is_refused() {
    let value = "ACTION.informed_consent.v0";
    assert_eq!(ArchetypeId::from_str(value), lexical_refusal(value));
}

#[test]
fn a_concept_starting_with_a_digit_is_refused() {
    let value = "openEHR-EHR-EVALUATION.1problem.v1";
    assert_eq!(ArchetypeId::from_str(value), lexical_refusal(value));
}

#[test]
fn an_empty_specialisation_is_refused() {
    let value = "openEHR-EHR-CLUSTER.imaging_exam-.v1";
    assert_eq!(ArchetypeId::from_str(value), lexical_refusal(value));
}

#[test]
fn a_namespaced_adl2_value_is_refused() {
    let value = "org.openehr::openEHR-EHR-OBSERVATION.blood_pressure.v1";
    assert_eq!(ArchetypeId::from_str(value), lexical_refusal(value));
}

#[test]
fn a_version_beyond_the_counted_range_is_refused() {
    let value = "openEHR-EHR-OBSERVATION.blood_pressure.v4294967296";
    assert!(matches!(
        ArchetypeId::from_str(value),
        Err(ArchetypeIdError::VersionRange { value: refused, .. }) if refused == value
    ));
}
