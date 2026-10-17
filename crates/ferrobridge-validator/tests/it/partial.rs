// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! A snapshot that omits children: each omitted child is held to the core
//! definition of the type that declares it. A snapshot is "a standalone
//! form that can be used and interpreted without considering the base
//! `StructureDefinition`"
//! (<https://hl7.org/fhir/R4/structuredefinition-definitions.html#StructureDefinition.snapshot>),
//! so R4 defines no reading of one that leaves children out; completing it
//! per child from the core definition is our own design.

use ferrobridge_validator::issue::MessageId;
use ferrobridge_validator::issue::Rule;
use ferrobridge_validator::set::PackageSet;
use serde_json::json;

use crate::support::TestResult;
use crate::support::against_patient;
use crate::support::core;
use crate::support::locations;
use crate::support::patient;
use crate::support::validate;

#[test]
fn a_child_of_a_datatype_the_snapshot_omits_keeps_its_core_binding() -> TestResult {
    // The patient profile expands Patient.name with family only; HumanName.use is bound
    // required to name-use in the core HumanName.
    let mut resource = patient();
    resource["name"][0]["use"] = json!("nickname-of-sorts");
    let set = PackageSet::load([core()])?;
    let against_core = validate(&set, &resource, None)?;
    assert_eq!(
        locations(&against_core, MessageId::NotInValueSet),
        ["Patient.name[0].use"]
    );
    let report = against_patient(&resource)?;
    assert_eq!(
        locations(&report, MessageId::NotInValueSet),
        ["Patient.name[0].use"],
        "{:#?}",
        report.issues()
    );
    Ok(())
}

#[test]
fn a_backbone_element_the_snapshot_omits_keeps_its_core_invariant() -> TestResult {
    // The core Patient.contact carries pat-1: a contact names a person, a telecom, an
    // address or an organization.
    let mut resource = patient();
    resource["contact"] = json!([{"gender": "male"}]);
    let report = against_patient(&resource)?;
    let pat_1: Vec<&str> = report
        .issues()
        .iter()
        .filter(|issue| matches!(&issue.rule, Rule::Constraint { key, .. } if key == "pat-1"))
        .map(|issue| issue.expression.as_str())
        .collect();
    assert_eq!(pat_1, ["Patient.contact[0]"], "{:#?}", report.issues());
    Ok(())
}

#[test]
fn a_child_the_snapshot_defines_keeps_the_profile_constraint() -> TestResult {
    // Patient.name.family is in the snapshot with a maximum length of five.
    let mut resource = patient();
    resource["name"][0]["family"] = json!("Longfamily");
    let report = against_patient(&resource)?;
    assert_eq!(
        locations(&report, MessageId::MaxLength),
        ["Patient.name[0].family"]
    );
    Ok(())
}
