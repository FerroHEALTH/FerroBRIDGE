// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! A start archetype held inside a `SECTION`: each diagnosis the section
//! holds searches, reads and is bound as a resource of its own, under the
//! path the engine's `$tofhir` names it by and with the id `$tofhir` answers
//! for it (FHIRconnect v1.0.0 `engine/id-management.adoc`, §Deterministic
//! IDs).

use ferrobridge_server::cdr::ids::EhrId;
use ferrobridge_server::facade::identity::FhirResourceId;
use ferrobridge_server::facade::identity::PersonId;
use ferrobridge_server::facade::identity::store::Store;
use ferrobridge_server::facade::search::Limits;
use http::StatusCode;
use std::error::Error;

use super::super::EHR_ID;
use super::super::Harness;
use super::super::SECTIONED_FIXTURES;
use super::super::VERSION_ONE;
use super::super::condition;
use super::super::stub::mount_read;
use super::entries::tofhir_ids;
use super::get;
use super::matches;
use super::mount_rows;
use super::node_row;

/// The context the sectioned fixture maps.
const CONTEXT: &str = "ferrobridge_sectioned.context";

/// The section the sectioned template holds its diagnoses in.
const SECTION: &str = "openEHR-EHR-SECTION.adhoc.v1";

/// Returns a harness over the sectioned fixture, its subject bound.
async fn sectioned() -> Harness {
    let harness = super::super::harness_built(
        SECTIONED_FIXTURES,
        super::super::settings(),
        Some(Limits::default()),
    )
    .await;
    let person =
        PersonId::new("http://example.org/fhir", super::SUBJECT).expect("a legal person key");
    harness
        .store
        .record_ehr(&person, &EhrId::new(EHR_ID).expect("a legal ehr_id"))
        .expect("the subject records");
    harness
}

/// Returns the composition one synthetic `Condition` coded `code` maps to.
fn coded(harness: &Harness, code: &str) -> serde_json::Value {
    let mut resource = condition();
    resource["code"]["coding"][0]["code"] = serde_json::json!(code);
    resource["meta"]["profile"] = serde_json::json!([
        "http://example.org/fhir/StructureDefinition/ferrobridge-sectioned-diagnosis"
    ]);
    harness.composition_under(CONTEXT, &resource)
}

/// Returns the items of the one section of `composition`.
fn items(composition: &serde_json::Value) -> &Vec<serde_json::Value> {
    composition["content"][0]["items"]
        .as_array()
        .expect("the composition holds its section's items")
}

/// Returns a composition whose section holds two diagnoses, `SYN-001` then
/// `SYN-002`, with each diagnosis.
fn two_in_a_section(
    harness: &Harness,
) -> (serde_json::Value, serde_json::Value, serde_json::Value) {
    let mut composition = coded(harness, "SYN-001");
    assert_eq!(
        Some(SECTION),
        composition["content"][0]["archetype_node_id"].as_str(),
        "the diagnosis sits inside the section: {composition}"
    );
    let first = items(&composition)[0].clone();
    let second = items(&coded(harness, "SYN-002"))[0].clone();
    composition["content"][0]["items"]
        .as_array_mut()
        .expect("the section holds items")
        .push(second.clone());
    (composition, first, second)
}

#[tokio::test]
async fn each_diagnosis_inside_a_section_searches_reads_and_binds_as_its_own_resource()
-> Result<(), Box<dyn Error>> {
    let harness = sectioned().await;
    let (composition, first, second) = two_in_a_section(&harness);
    mount_rows(
        &harness.cdr,
        &[
            node_row(VERSION_ONE, &composition, &first),
            node_row(VERSION_ONE, &composition, &second),
        ],
    )
    .await;
    let (status, body) = get(&harness, "Condition?code=SYN-001,SYN-002").await?;
    assert_eq!(StatusCode::OK, status, "{body}");
    let found = matches(&body);
    let codes: Vec<Option<&str>> = found
        .iter()
        .map(|entry| entry["resource"]["code"]["coding"][0]["code"].as_str())
        .collect();
    assert_eq!(vec![Some("SYN-001"), Some("SYN-002")], codes, "{body}");
    let ids: Vec<String> = found
        .iter()
        .filter_map(|entry| entry["resource"]["id"].as_str().map(str::to_owned))
        .collect();
    assert_eq!(2, ids.len(), "{body}");
    assert_ne!(ids.first(), ids.get(1), "two diagnoses answered one id");

    let mut paths = Vec::new();
    for id in &ids {
        let binding = harness
            .store
            .binding_of("Condition", &FhirResourceId::new(id)?)?
            .ok_or("the diagnosis is bound")?;
        // NOTE: openEHR BASE Release 1.2.0, Paths and Locators: the path parses
        // back and names the diagnosis inside the section.
        let parsed: openehr_rm::v1_2::paths::RmPath = binding.entry_path.parse()?;
        assert_eq!(2, parsed.segments.len(), "{}", binding.entry_path);
        paths.push(binding.entry_path);
    }
    assert_eq!(
        vec![
            format!("/content[{SECTION}]/items[openEHR-EHR-EVALUATION.problem_diagnosis.v1]"),
            format!("/content[{SECTION}]/items[2]"),
        ],
        paths
    );

    mount_read(&harness.cdr, VERSION_ONE, &composition).await;
    for (id, code) in ids.iter().zip(["SYN-001", "SYN-002"]) {
        let (status, read) = get(&harness, &format!("Condition/{id}")).await?;
        assert_eq!(StatusCode::OK, status, "{read}");
        assert_eq!(Some(id.as_str()), read["id"].as_str());
        assert_eq!(
            Some(code),
            read["code"]["coding"][0]["code"].as_str(),
            "{read}"
        );
    }

    let mut held = composition.clone();
    held["uid"] = serde_json::json!({ "_type": "OBJECT_VERSION_ID", "value": VERSION_ONE });
    let answered = tofhir_ids(
        SECTIONED_FIXTURES,
        ferrobridge_testkit::fixtures::DIAGNOSE_SECTIONED_OPT,
        &held,
    )
    .await?;
    assert_eq!(ids, answered);
    Ok(())
}
