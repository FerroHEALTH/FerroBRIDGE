// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The elements of a discharge entry the HDR project carries into the
//! HL7 Europe Base and Core 2.0.1 profile of its resource, and the ones a run
//! out of openEHR declares because the profile has no place for them.

#![expect(
    clippy::panic_in_result_fn,
    reason = "a test asserts, and returns its setup errors"
)]

use std::error::Error;

use super::{
    composition_of, diagnosis, element, instant, mapping, of_type, only, order, procedure,
    reference, settings,
};
use fhirconnect::engine::context::CallContext;
use fhirconnect::engine::outcome::Loss;

/// `entry` with `extra` added before the first element of its first item
/// list.
fn with_items(entry: &str, extra: &[String]) -> String {
    entry.replacen(
        r#""items": ["#,
        &format!(r#""items": [{},"#, extra.join(",")),
        1,
    )
}

/// A coded text of a local code of the entry's archetype.
fn local(value: &str, code: &str) -> String {
    format!(
        r#"{{ "_type": "DV_CODED_TEXT", "value": "{value}", "defining_code": {{ "_type": "CODE_PHRASE", "terminology_id": {{ "_type": "TERMINOLOGY_ID", "value": "local" }}, "code_string": "{code}" }} }}"#
    )
}

/// A plain text.
fn text(value: &str) -> String {
    format!(r#"{{ "_type": "DV_TEXT", "value": "{value}" }}"#)
}

/// A synthetic discharge report whose entries record every element the
/// issue names beside the ones the project already carried.
fn recorded_report() -> String {
    let diagnosis = with_items(
        &diagnosis(),
        &[
            element("at0005", "Severity", &local("Moderate", "at0048")),
            element(
                "at0030",
                "Date/time of resolution",
                &instant("2026-10-19T09:00:00Z"),
            ),
            element(
                "at0073",
                "Diagnostic certainty",
                &local("Confirmed", "at0076"),
            ),
            element(
                "at0009",
                "Clinical description",
                &text("A synthetic clinical description."),
            ),
        ],
    );
    let procedure = with_items(
        &procedure(),
        &[
            element("at0014", "Reason", &text("A synthetic reason")),
            element("at0048", "Outcome", &text("A synthetic outcome")),
            element("at0006", "Complication", &text("A synthetic complication")),
            element("at0065", "Method", &text("A synthetic method")),
            element(
                "at0067",
                "Procedure type",
                &text("A synthetic procedure type"),
            ),
        ],
    );
    let details = format!(
        r#"{{ "_type": "CLUSTER", "name": {{ "_type": "DV_TEXT", "value": "Order details" }}, "archetype_node_id": "at0113", "items": [{}] }}"#,
        element(
            "at0012",
            "Order start date/time",
            &instant("2026-10-20T08:00:00Z")
        )
    );
    let order = with_items(
        &order(),
        &[element("at0091", "Route", &text("Oral")), details],
    );
    composition_of(&[
        ("Diagnostic summary", &[diagnosis]),
        ("Significant procedures", &[procedure]),
        ("Discharge medications", &[order]),
    ])
}

#[test]
fn every_element_condition_eu_core_has_a_place_for_is_served() -> Result<(), Box<dyn Error>> {
    let run = mapping()?.to_fhir_with(
        &recorded_report(),
        &settings(),
        CallContext::new().with_patient(reference("Patient/synthetic-patient")),
    )?;
    let condition = only(run.bundle(), "Condition")?;
    assert_eq!(
        condition["severity"]["coding"][0]["code"], "at0048",
        "{condition}"
    );
    assert_eq!(condition["severity"]["text"], "Moderate", "{condition}");
    assert_eq!(
        condition["abatementDateTime"], "2026-10-19T09:00:00Z",
        "{condition}"
    );
    // NOTE: R4 Condition.verificationStatus binds condition-ver-status as required.
    assert_eq!(
        condition["verificationStatus"]["coding"][0],
        serde_json::json!({
            "system": "http://terminology.hl7.org/CodeSystem/condition-ver-status",
            "code": "confirmed"
        }),
        "{condition}"
    );
    Ok(())
}

#[test]
fn every_element_procedure_eu_core_has_a_place_for_is_served() -> Result<(), Box<dyn Error>> {
    let run = mapping()?.to_fhir_with(
        &recorded_report(),
        &settings(),
        CallContext::new().with_patient(reference("Patient/synthetic-patient")),
    )?;
    let procedure = only(run.bundle(), "Procedure")?;
    assert_eq!(procedure["reasonCode"][0]["text"], "A synthetic reason");
    assert_eq!(procedure["outcome"]["text"], "A synthetic outcome");
    assert_eq!(
        procedure["complication"][0]["text"],
        "A synthetic complication"
    );
    Ok(())
}

#[test]
fn the_route_and_start_of_a_discharge_medicine_fill_its_one_dosage() -> Result<(), Box<dyn Error>> {
    let run = mapping()?.to_fhir_with(
        &recorded_report(),
        &settings(),
        CallContext::new().with_patient(reference("Patient/synthetic-patient")),
    )?;
    let request = only(run.bundle(), "MedicationRequest")?;
    let dosages = request["dosageInstruction"]
        .as_array()
        .ok_or("the dosage")?;
    assert_eq!(dosages.len(), 1, "{request}");
    let dosage = &dosages[0];
    assert_eq!(dosage["text"], "One synthetic tablet each morning");
    assert_eq!(dosage["route"]["text"], "Oral", "{dosage}");
    assert_eq!(
        dosage["timing"]["repeat"]["boundsPeriod"]["start"], "2026-10-20T08:00:00Z",
        "{dosage}"
    );
    Ok(())
}

#[test]
fn every_element_no_profile_has_a_place_for_is_declared_in_the_run() -> Result<(), Box<dyn Error>> {
    let (run, warnings) = mapping()?.to_fhir_with_warnings(
        &recorded_report(),
        &settings(),
        CallContext::new().with_patient(reference("Patient/synthetic-patient")),
    )?;
    let unread: Vec<(&str, &str)> = warnings
        .iter()
        .filter_map(|warning| match *warning.loss() {
            Loss::Unread { ref node } => Some((
                node.as_str(),
                warning.location().openehr().unwrap_or_default(),
            )),
            _ => None,
        })
        .collect();
    let nodes: Vec<&str> = unread.iter().map(|&(node, _)| node).collect();
    assert_eq!(
        nodes,
        ["at0009", "at0065", "at0067", "timing"],
        "the clinical description, the method, the procedure type and the timing: {unread:?}"
    );
    for (node, path) in &unread {
        assert!(path.contains(node), "{node} is named by its path: {path}");
    }
    let outcomes = of_type(run.bundle(), "OperationOutcome")?;
    let [outcome] = outcomes.as_slice() else {
        return Err("the run's one OperationOutcome".into());
    };
    let named: Vec<&str> = outcome["issue"]
        .as_array()
        .ok_or("the issues")?
        .iter()
        .filter_map(|issue| issue["details"]["coding"][0]["code"].as_str())
        .collect();
    for (_, path) in &unread {
        assert!(named.contains(path), "the outcome names {path}: {outcome}");
    }
    Ok(())
}

#[test]
fn a_report_that_records_only_what_the_project_carries_declares_nothing()
-> Result<(), Box<dyn Error>> {
    let (_, warnings) = mapping()?.to_fhir_with_warnings(
        &composition_of(&[
            ("Diagnostic summary", &[diagnosis()]),
            ("Significant procedures", &[procedure()]),
        ]),
        &settings(),
        CallContext::new().with_patient(reference("Patient/synthetic-patient")),
    )?;
    assert!(
        warnings
            .iter()
            .all(|warning| !matches!(warning.loss(), Loss::Unread { .. })),
        "{warnings:?}"
    );
    Ok(())
}
