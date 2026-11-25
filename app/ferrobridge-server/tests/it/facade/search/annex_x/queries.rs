// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The queries the Annex X searches send and the answers they include.

use std::error::Error;

use http::StatusCode;

use super::super::super::VERSION_ONE;
use super::super::SUBJECT;
use super::super::get;
use super::super::matches;
use super::super::mount_rows;
use super::super::queries;
use super::super::row;
use super::BASE;
use super::annex_x;
use super::composition_of;
use super::entry_row;
use super::groups::diagnostic_report;
use super::groups::groups;
use super::node_of;
use super::serve;
use super::type_of;

#[tokio::test]
async fn a_medication_statement_includes_the_medication_it_references() -> Result<(), Box<dyn Error>>
{
    // PCC-44 2:3.44.4.1.2.1.5: MedicationStatement with
    // _include=MedicationStatement:medication; draft Annex X Table 22 makes
    // the medication a group of its own.
    let harness = annex_x().await?;
    let group = groups()
        .into_iter()
        .find(|group| group.name == "EHDSMedicationUse")
        .ok_or("the medication use group")?;
    let composition = composition_of(&harness, &group);
    mount_rows(&harness.cdr, &[row(VERSION_ONE, &composition)]).await;
    let (status, body) = get(
        &harness,
        &format!("MedicationStatement?patient={SUBJECT}&_include=MedicationStatement:medication"),
    )
    .await?;
    assert_eq!(StatusCode::OK, status, "{body}");
    let included: Vec<&serde_json::Value> = body["entry"]
        .as_array()
        .map(|entries| {
            entries
                .iter()
                .filter(|entry| entry["search"]["mode"] == "include")
                .collect()
        })
        .unwrap_or_default();
    assert_eq!(1, included.len(), "{body}");
    let medication = &included[0]["resource"];
    assert_eq!(Some("Medication"), medication["resourceType"].as_str());
    assert_eq!(
        Some("387517004"),
        medication["code"]["coding"][0]["code"].as_str()
    );
    assert_eq!(
        serde_json::json!([format!("{BASE}/medication-eu-core")]),
        medication["meta"]["profile"]
    );
    let reference = matches(&body)[0]["resource"]["medicationReference"]["reference"]
        .as_str()
        .ok_or("the statement references its medication")?
        .to_owned();
    assert_eq!(
        Some(reference.as_str()),
        medication["id"]
            .as_str()
            .map(|id| format!("Medication/{id}"))
            .as_deref()
    );
    Ok(())
}

#[tokio::test]
async fn an_observation_of_another_category_or_code_answers_no_query() -> Result<(), Box<dyn Error>>
{
    // Each Observation context writes its category as a constant and the body
    // weight its LOINC code too, so a value neither carries matches no
    // resource and queries nothing.
    let harness = annex_x().await?;
    for search in [
        "category=exam",
        "category=vital-signs&code=http://loinc.org|8302-2",
    ] {
        let (status, body) =
            get(&harness, &format!("Observation?patient={SUBJECT}&{search}")).await?;
        assert_eq!(StatusCode::OK, status, "{search}: {body}");
        assert!(matches(&body).is_empty(), "{search}: {body}");
    }
    assert_eq!(0, queries(&harness).await.len());
    Ok(())
}

#[tokio::test]
async fn the_statement_lists_each_group_s_searches() -> Result<(), Box<dyn Error>> {
    let harness = annex_x().await?;
    let (status, body) = get(&harness, "metadata").await?;
    assert_eq!(StatusCode::OK, status, "{body}");
    let resources = body["rest"][0]["resource"]
        .as_array()
        .ok_or("the statement names resources")?;
    for (resource_type, parameters) in [
        ("AllergyIntolerance", &["patient"][..]),
        ("Condition", &["patient", "onset-date"][..]),
        ("Procedure", &["patient", "date"][..]),
        ("MedicationStatement", &["patient"][..]),
        ("Immunization", &["patient", "date"][..]),
        ("Observation", &["patient", "category", "code", "date"][..]),
        (
            "DiagnosticReport",
            &["patient", "category", "code", "date"][..],
        ),
        ("DeviceUseStatement", &["patient"][..]),
        ("Specimen", &["patient"][..]),
    ] {
        let resource = resources
            .iter()
            .find(|resource| resource["type"] == resource_type)
            .ok_or(resource_type)?;
        let names: Vec<&str> = resource["searchParam"]
            .as_array()
            .map(|params| {
                params
                    .iter()
                    .filter_map(|param| param["name"].as_str())
                    .collect()
            })
            .unwrap_or_default();
        for parameter in parameters {
            assert!(names.contains(parameter), "{resource_type}: {names:?}");
        }
    }
    let statement = resources
        .iter()
        .find(|resource| resource["type"] == "MedicationStatement")
        .ok_or("MedicationStatement")?;
    assert_eq!(
        serde_json::json!(["MedicationStatement:medication"]),
        statement["searchInclude"]
    );
    Ok(())
}

#[tokio::test]
async fn each_date_search_compares_the_instant_the_mapping_writes() -> Result<(), Box<dyn Error>> {
    // A date search reads the RM attribute the mapping fills from the
    // resource's own date, so the stub's rows and a CDR's WHERE agree.
    let harness = annex_x().await?;
    for (name, pointer, path, date) in [
        ("EHDSProcedure", "/time/value", "/time/value", "2026-02-01"),
        (
            "EHDSImmunisation",
            "/time/value",
            "/time/value",
            "2026-04-01",
        ),
        (
            "EHDSObservation",
            "/data/origin/value",
            "/data[at0002]/origin/value",
            "2026-05-01",
        ),
    ] {
        let group = groups()
            .into_iter()
            .find(|group| group.name == name)
            .ok_or(name)?;
        let composition = composition_of(&harness, &group);
        let entry = node_of(&composition, group.archetype).ok_or(name)?;
        assert!(
            entry
                .pointer(pointer)
                .and_then(serde_json::Value::as_str)
                .is_some_and(|value| value.starts_with(date)),
            "{name}: {entry}"
        );
        mount_rows(&harness.cdr, &[entry_row(&composition, group.archetype)?]).await;
        let resource_type = type_of(&group);
        let (status, body) = get(
            &harness,
            &format!("{resource_type}?patient={SUBJECT}&date=ge{date}"),
        )
        .await?;
        assert_eq!(StatusCode::OK, status, "{name}: {body}");
        let sent = queries(&harness).await;
        assert!(
            sent.iter()
                .filter_map(|query| query["q"].as_str())
                .any(|text| text.contains(&format!("n0{path}"))),
            "{name}: {sent:?}"
        );
    }
    Ok(())
}

#[tokio::test]
async fn a_laboratory_report_answers_its_qedm_searches_and_includes_its_results()
-> Result<(), Box<dyn Error>> {
    // PCC-44 2:3.44.4.1.2.1.4; R4 search.html §3.1.1.5.4 for the include.
    let report = diagnostic_report();
    serve(&report).await?;
    let harness = annex_x().await?;
    let composition = composition_of(&harness, &report);
    mount_rows(&harness.cdr, &[entry_row(&composition, report.archetype)?]).await;
    let (status, body) = get(
        &harness,
        &format!("DiagnosticReport?patient={SUBJECT}&_include=DiagnosticReport:result"),
    )
    .await?;
    assert_eq!(StatusCode::OK, status, "{body}");
    let found = matches(&body);
    assert_eq!(
        Some("LAB"),
        found[0]["resource"]["category"][0]["coding"][0]["code"].as_str(),
        "{body}"
    );
    let included: Vec<&serde_json::Value> = body["entry"]
        .as_array()
        .map(|entries| {
            entries
                .iter()
                .filter(|entry| entry["search"]["mode"] == "include")
                .collect()
        })
        .unwrap_or_default();
    assert_eq!(1, included.len(), "{body}");
    assert_eq!(
        Some("Observation"),
        included[0]["resource"]["resourceType"].as_str()
    );
    Ok(())
}

#[tokio::test]
async fn a_report_code_matches_the_test_name_under_any_event() -> Result<(), Box<dyn Error>> {
    // PCC-44 2:3.44.4.1.2.1.4 patient+category+code: KDS_Laborbericht holds
    // the test name under its repeating event, so the query binds the event
    // inside the result and a report matches when any of its events does.
    let report = diagnostic_report();
    let harness = annex_x().await?;
    let composition = composition_of(&harness, &report);
    mount_rows(&harness.cdr, &[entry_row(&composition, report.archetype)?]).await;
    let (status, body) = get(
        &harness,
        &format!("DiagnosticReport?patient={SUBJECT}&category=LAB&code=http://loinc.org|24331-1"),
    )
    .await?;
    assert_eq!(StatusCode::OK, status, "{body}");
    assert_eq!(1, matches(&body).len(), "{body}");
    let sent = queries(&harness).await;
    let text = sent
        .iter()
        .find_map(|query| query["q"].as_str())
        .ok_or("the search queried the CDR")?;
    assert!(
        text.contains(
            "OBSERVATION[openEHR-EHR-OBSERVATION.laboratory_test_result.v1] CONTAINS EVENT n0[at0002]"
        ),
        "{text}"
    );
    assert!(
        text.contains(
            "n0/data[at0003]/items[at0005,'Labortest-Bezeichnung']/value/defining_code/code_string"
        ),
        "{text}"
    );
    Ok(())
}
