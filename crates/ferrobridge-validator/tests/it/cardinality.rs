// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Cardinality per parent occurrence, from the element table and from the
//! profile snapshot (ElementDefinition.min and ElementDefinition.max,
//! <https://hl7.org/fhir/R4/elementdefinition.html>).

use ferrobridge_validator::issue::MessageId;
use ferrobridge_validator::set::PackageSet;
use serde_json::json;

use crate::support::TestResult;
use crate::support::against_patient;
use crate::support::core;
use crate::support::locations;
use crate::support::patient;
use crate::support::validate;

#[test]
fn a_missing_required_element_of_the_base_is_reported_at_its_parent() -> TestResult {
    let set = PackageSet::load([core()])?;
    let report = validate(
        &set,
        &json!({"resourceType": "Observation", "code": {"text": "t"}}),
        None,
    )?;
    assert_eq!(
        locations(&report, MessageId::MinCardinality),
        ["Observation"]
    );
    Ok(())
}

#[test]
fn a_primitive_present_only_through_its_sibling_counts_as_present() -> TestResult {
    // json.html §2.6.2.3: a primitive with an extension and no value is
    // written as the `_` member alone, and it is an occurrence.
    let set = PackageSet::load([core()])?;
    let observation = json!({
        "resourceType": "Observation",
        "_status": {"extension": [{"url": "http://hl7.org/fhir/StructureDefinition/data-absent-reason",
                                    "valueCode": "unknown"}]},
        "code": {"text": "t"}
    });
    let report = validate(&set, &observation, None)?;
    assert!(
        locations(&report, MessageId::MinCardinality).is_empty(),
        "{:#?}",
        report.issues()
    );
    Ok(())
}

#[test]
fn a_minimum_the_profile_raises_is_reported() -> TestResult {
    let mut resource = patient();
    resource.as_object_mut().expect("an object").remove("name");
    let report = against_patient(&resource)?;
    assert_eq!(locations(&report, MessageId::MinCardinality), ["Patient"]);
    Ok(())
}

#[test]
fn a_maximum_the_profile_lowers_is_reported() -> TestResult {
    let mut resource = patient();
    resource["name"] = json!([{"family": "Doe"}, {"family": "Roe"}, {"family": "Poe"}]);
    let report = against_patient(&resource)?;
    assert_eq!(
        locations(&report, MessageId::MaxCardinality),
        ["Patient.name"]
    );
    Ok(())
}

#[test]
fn a_child_minimum_applies_to_each_parent_occurrence() -> TestResult {
    let mut resource = patient();
    resource["name"] = json!([{"family": "Doe"}, {"given": ["Bea"]}]);
    let report = against_patient(&resource)?;
    assert_eq!(
        locations(&report, MessageId::MinCardinality),
        ["Patient.name[1]"]
    );
    Ok(())
}
