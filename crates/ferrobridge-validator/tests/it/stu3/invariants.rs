// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The invariants of STU3 definitions, evaluated as FHIRPath over the STU3
//! types (<https://hl7.org/fhir/STU3/conformance-rules.html#constraints>).

use ferrobridge_validator::issue::Rule;
use ferrobridge_validator::report::Report;
use serde_json::json;

use crate::stu3::stu3_profile;
use crate::stu3::stu3_set;
use crate::stu3::validate_stu3;
use crate::support::BASE;
use crate::support::TestResult;

/// The keys and locations of the invariants `report` raises.
fn broken(report: &Report) -> Vec<(String, String)> {
    report
        .issues()
        .iter()
        .filter_map(|issue| match &issue.rule {
            Rule::Constraint { key, .. } => Some((key.clone(), issue.expression.clone())),
            Rule::Message(_) => None,
        })
        .collect()
}

#[test]
fn a_contact_without_any_detail_breaks_the_stu3_core_invariant_pat_1() -> TestResult {
    let (_synthetic, set) = stu3_set(&[])?;
    let report = validate_stu3(
        &set,
        &json!({"resourceType": "Patient", "contact": [{"gender": "male"}]}),
        None,
    )?;
    assert_eq!(
        broken(&report),
        [("pat-1".to_owned(), "Patient.contact[0]".to_owned())]
    );
    Ok(())
}

#[test]
fn a_contact_with_a_name_meets_pat_1() -> TestResult {
    let (_synthetic, set) = stu3_set(&[])?;
    let report = validate_stu3(
        &set,
        &json!({"resourceType": "Patient", "contact": [{"name": {"family": "Doe"}}]}),
        None,
    )?;
    assert!(broken(&report).is_empty(), "{:#?}", report.issues());
    Ok(())
}

#[test]
fn an_invariant_an_stu3_profile_declares_is_evaluated() -> TestResult {
    let definition = stu3_profile(
        "stu3-named",
        "Patient",
        &json!([
            {"id": "Patient", "path": "Patient", "min": 0, "max": "*",
             "constraint": [{"key": "named-1", "severity": "error",
                             "human": "A patient has a name", "expression": "name.exists()",
                             "xpath": "exists(f:name)"}]}
        ]),
    );
    let (_synthetic, set) = stu3_set(&[definition])?;
    let report = validate_stu3(
        &set,
        &json!({"resourceType": "Patient", "active": true}),
        Some(&format!("{BASE}stu3-named")),
    )?;
    assert_eq!(
        broken(&report),
        [("named-1".to_owned(), "Patient".to_owned())]
    );
    Ok(())
}
