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
    reference, sent, settings,
};
use crate::support::hdr;
use eehrxf::crosswalk::discharge::HDR_COMPOSITION;
use eehrxf::document::discharge::{HDR_BUNDLE, HDR_SLOTS};
use eehrxf::mapping::left_out;
use eehrxf::mappings::HDR;
use eehrxf::receive::ReceivedDocument;
use fhirconnect::engine::context::CallContext;
use fhirconnect::engine::outcome::Loss;
use openehr_mapping_core::index::WebTemplateIndex;
use openehr_sdt::flat::example::{DetailLevel, apply_output_uid, example_composition};
use serde_json::Value;

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

/// A discharge report of FerroBRIDGE's template that records one occurrence
/// of every node the template holds, generated from its Web Template
/// (`openehr-sdt`'s example at the `Medium` detail level), with the stay's
/// end, a completed procedure and a medicine's timing, which the example
/// leaves out or sets otherwise.
fn every_element() -> Result<String, Box<dyn Error>> {
    let index = WebTemplateIndex::build(&super::template()?)?;
    let mut example = example_composition(index.web_template(), DetailLevel::Medium)?;
    apply_output_uid(&mut example, index.template_id());
    example["context"]["end_time"] =
        serde_json::json!({ "_type": "DV_DATE_TIME", "value": "2026-10-20T11:00:00Z" });
    let procedure = &mut example["content"][1]["items"][0]["ism_transition"];
    procedure["careflow_step"]["defining_code"]["code_string"] = Value::from("at0043");
    procedure["current_state"]["defining_code"]["code_string"] = Value::from("532");
    example["content"][2]["items"][0]["activities"][0]["timing"] = serde_json::json!({
        "_type": "DV_PARSABLE",
        "value": "R/2026-10-20T08:00:00Z/P1D",
        "formalism": "ISO8601"
    });
    Ok(serde_json::to_string(&example)?)
}

#[test]
fn the_project_declares_every_element_of_an_entry_it_leaves_out() -> Result<(), Box<dyn Error>> {
    let (_, warnings) = mapping()?.to_fhir_with_warnings(
        &every_element()?,
        &settings(),
        CallContext::new().with_patient(reference("Patient/synthetic-patient")),
    )?;
    let named = left_out(&warnings);
    let declared = HDR.left_out();
    assert_eq!(
        named.len(),
        declared.len(),
        "the run leaves out exactly the elements the project declares: {named:#?}"
    );
    for element in declared {
        let at = format!("[{}]/{}", element.archetype, element.path);
        assert!(
            named
                .iter()
                .any(|path| path.ends_with(&at) || path.contains(&format!("{at}/"))),
            "{at} is declared and the run names it: {named:#?}"
        );
    }
    for path in &named {
        let slice = HDR
            .slice_of(path)
            .ok_or(format!("{path} is in no section"))?;
        assert!(
            HDR_SLOTS.iter().any(|slot| slot.slice == slice),
            "{slice} is a slice of composition-eu-hdr"
        );
    }
    Ok(())
}

/// The narrative `div` of the section of the discharge report `text` whose
/// LOINC code is `code`.
fn narrative(text: &str, code: &str) -> Result<String, Box<dyn Error>> {
    let root: Value = serde_json::from_str(text)?;
    let section = root["entry"][0]["resource"]["section"]
        .as_array()
        .ok_or("the sections")?
        .iter()
        .find(|section| section["code"]["coding"][0]["code"] == code)
        .ok_or(format!("the section {code}"))?;
    Ok(section["text"]["div"]
        .as_str()
        .ok_or("the narrative")?
        .to_owned())
}

#[test]
fn a_written_report_names_each_left_out_element_in_its_section_narrative()
-> Result<(), Box<dyn Error>> {
    let text = sent(
        &mapping()?,
        &recorded_report(),
        "urn:uuid:5b0c3f7e-0000-4000-8000-00000000d1a1",
    )?;
    ReceivedDocument::read(&text)?.check(&hdr(HDR_BUNDLE)?, &hdr(HDR_COMPOSITION)?)?;
    let named = [
        (
            "11535-2",
            "/items[openEHR-EHR-EVALUATION.problem_diagnosis.v1]/data[at0001]/items[at0009]/value",
        ),
        (
            "10185-7",
            "/items[openEHR-EHR-ACTION.procedure.v1]/description[at0001]/items[at0065]/value",
        ),
        (
            "10185-7",
            "/items[openEHR-EHR-ACTION.procedure.v1]/description[at0001]/items[at0067]/value",
        ),
        (
            "75311-1",
            "/items[openEHR-EHR-INSTRUCTION.medication_order.v3]/activities[at0001]/timing",
        ),
    ];
    for (code, tail) in named {
        let div = narrative(&text, code)?;
        assert!(
            div.contains(tail) && div.contains("which this document does not carry"),
            "the section {code} names {tail}: {div}"
        );
    }
    for code in ["11535-2", "10185-7", "75311-1"] {
        let div = narrative(&text, code)?;
        assert!(
            !div.contains("A synthetic clinical description")
                && !div.contains("A synthetic method")
                && !div.contains("R/2026"),
            "the narrative names no value: {div}"
        );
    }
    Ok(())
}

#[test]
fn a_written_report_that_carries_every_element_names_nothing() -> Result<(), Box<dyn Error>> {
    let text = sent(
        &mapping()?,
        &composition_of(&[
            ("Diagnostic summary", &[diagnosis()]),
            ("Significant procedures", &[procedure()]),
        ]),
        "urn:uuid:5b0c3f7e-0000-4000-8000-00000000d1a2",
    )?;
    for code in ["11535-2", "10185-7"] {
        let div = narrative(&text, code)?;
        assert!(!div.contains("does not carry"), "{div}");
    }
    Ok(())
}
