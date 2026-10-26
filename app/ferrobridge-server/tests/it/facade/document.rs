// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! `Composition/$document`: the document Bundle of one mapped Composition,
//! the Composition first and every resource its outbound run creates after
//! it (<https://hl7.org/fhir/R4/composition-operation-document.html>), and
//! the refusals of the parameters this server does not support.

use axum::body::Body;
use http::Request;
use http::StatusCode;
use http::header;
use std::error::Error;

use super::BASE_URL;
use super::Harness;
use super::VERSION_ONE;
use super::call;
use super::harness_over;
use super::settings;
use super::stub::mount_create;
use super::stub::mount_ehr;
use super::stub::mount_read;
use super::stub::mount_version_read;

/// The fixture directory holding the Composition context.
const DOCUMENT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/it/document");

/// The profile the Composition context maps.
const PROFILE: &str = "http://example.org/fhir/StructureDefinition/ferrobridge-document";

/// Returns the synthetic Composition whose section names one contained
/// Condition.
fn composition() -> serde_json::Value {
    serde_json::json!({
        "resourceType": "Composition",
        "meta": {"profile": [PROFILE]},
        "status": "final",
        "type": {"text": "Synthetic report"},
        "subject": {"identifier": {
            "system": "http://example.org/fhir/sid/ferrobridge-subject",
            "value": "synthetic-subject-0001"
        }},
        "date": "2026-09-15T09:00:00Z",
        "author": [{"display": "Synthetic Author"}],
        "title": "Synthetic report",
        "contained": [{
            "resourceType": "Condition",
            "id": "problem",
            "subject": {"identifier": {
                "system": "http://example.org/fhir/sid/ferrobridge-subject",
                "value": "synthetic-subject-0001"
            }},
            "code": {"text": "Synthetic diagnosis"}
        }],
        "section": [{"entry": [{"reference": "#problem"}]}]
    })
}

/// Returns the canonical composition the synthetic Composition maps to.
fn canonical(harness: &Harness) -> serde_json::Value {
    let loaded = harness
        .facade
        .programs()
        .by_context("ferrobridge_document.context")
        .cloned()
        .expect("the fixture context compiled");
    let document: fhir_types::codec::Value =
        serde_json::from_value(composition()).expect("the Composition reads as a document");
    ferrobridge_server::facade::engine::inbound(
        loaded.program(),
        loaded.index(),
        &document,
        "2026-09-15T09:00:00Z",
        &settings(),
        &fhirconnect::engine::terminology::NoTerminology,
        &fhirconnect::engine::demographics::IssuerDemographics::new(BASE_URL)
            .expect("the test base issues identities"),
    )
    .expect("the Composition maps")
    .into_value()
    .into_value()
}

/// Creates the synthetic Composition over the stub CDR and returns its id.
async fn created(harness: &Harness) -> Result<String, Box<dyn Error>> {
    let stored = canonical(harness);
    mount_ehr(&harness.cdr).await;
    mount_create(&harness.cdr, VERSION_ONE, &stored).await;
    mount_read(&harness.cdr, VERSION_ONE, &stored).await;
    mount_version_read(&harness.cdr, VERSION_ONE, &stored).await;
    let (status, body) = call(
        harness.app(),
        Request::post("/fhir/Composition")
            .header(header::CONTENT_TYPE, "application/fhir+json")
            .body(Body::from(composition().to_string()))?,
    )
    .await?;
    assert_eq!(StatusCode::CREATED, status, "{body}");
    Ok(body["id"]
        .as_str()
        .ok_or("the created Composition has an id")?
        .to_owned())
}

/// Answers `GET` on `path` against the harness.
async fn get(
    harness: &Harness,
    path: &str,
) -> Result<(StatusCode, serde_json::Value), Box<dyn Error>> {
    call(harness.app(), Request::get(path).body(Body::empty())?).await
}

/// Asserts `bundle` is the document of the Composition `id`.
fn assert_document(bundle: &serde_json::Value, id: &str) {
    assert_eq!(Some("Bundle"), bundle["resourceType"].as_str(), "{bundle}");
    assert_eq!(Some("document"), bundle["type"].as_str(), "{bundle}");
    assert!(bundle["timestamp"].is_string(), "{bundle}");
    assert_eq!(
        Some("urn:ietf:rfc:3986"),
        bundle["identifier"]["system"].as_str(),
        "{bundle}"
    );
    let entries = bundle["entry"]
        .as_array()
        .expect("the document has entries");
    assert_eq!(
        2,
        entries.len(),
        "the Composition and its Condition: {bundle}"
    );
    let composition = &entries[0];
    assert_eq!(
        Some("Composition"),
        composition["resource"]["resourceType"].as_str(),
        "{bundle}"
    );
    assert_eq!(Some(id), composition["resource"]["id"].as_str(), "{bundle}");
    assert_eq!(
        Some(format!("{BASE_URL}/Composition/{id}").as_str()),
        composition["fullUrl"].as_str(),
        "{bundle}"
    );
    let condition = &entries[1];
    assert_eq!(
        Some("Condition"),
        condition["resource"]["resourceType"].as_str(),
        "{bundle}"
    );
    assert_eq!(
        Some("Synthetic diagnosis"),
        condition["resource"]["code"]["text"].as_str(),
        "{bundle}"
    );
    let condition_id = condition["resource"]["id"]
        .as_str()
        .expect("the Condition has an id");
    assert_eq!(
        Some(format!("{BASE_URL}/Condition/{condition_id}").as_str()),
        condition["fullUrl"].as_str(),
        "{bundle}"
    );
    assert_eq!(
        Some(format!("Condition/{condition_id}").as_str()),
        composition["resource"]["section"][0]["entry"][0]["reference"].as_str(),
        "the section names the Condition the document carries: {bundle}"
    );
}

#[tokio::test]
async fn the_document_is_the_composition_and_every_resource_it_creates()
-> Result<(), Box<dyn Error>> {
    let harness = harness_over(DOCUMENT, settings()).await;
    let id = created(&harness).await?;
    let (status, bundle) = get(&harness, &format!("/fhir/Composition/{id}/$document")).await?;
    assert_eq!(StatusCode::OK, status, "{bundle}");
    assert_document(&bundle, &id);
    Ok(())
}

#[tokio::test]
async fn the_type_level_form_names_the_composition_by_id() -> Result<(), Box<dyn Error>> {
    let harness = harness_over(DOCUMENT, settings()).await;
    let id = created(&harness).await?;
    let (status, bundle) = get(&harness, &format!("/fhir/Composition/$document?id={id}")).await?;
    assert_eq!(StatusCode::OK, status, "{bundle}");
    assert_document(&bundle, &id);
    let url = format!("{BASE_URL}/Composition/{id}");
    let (status, bundle) = get(
        &harness,
        &format!(
            "/fhir/Composition/$document?id={}",
            url::form_urlencoded::byte_serialize(url.as_bytes()).collect::<String>()
        ),
    )
    .await?;
    assert_eq!(StatusCode::OK, status, "{bundle}");
    assert_document(&bundle, &id);
    Ok(())
}

#[tokio::test]
async fn one_composition_version_names_one_document() -> Result<(), Box<dyn Error>> {
    let harness = harness_over(DOCUMENT, settings()).await;
    let id = created(&harness).await?;
    let path = format!("/fhir/Composition/{id}/$document?persist=false");
    let (_, first) = get(&harness, &path).await?;
    let (status, second) = get(&harness, &path).await?;
    assert_eq!(StatusCode::OK, status, "{second}");
    assert_eq!(first["identifier"], second["identifier"]);
    Ok(())
}

#[tokio::test]
async fn the_parameters_this_server_does_not_support_are_refused() -> Result<(), Box<dyn Error>> {
    let harness = harness_over(DOCUMENT, settings()).await;
    let id = created(&harness).await?;
    for (query, code) in [
        ("persist=true", "not-supported"),
        ("persist=maybe", "invalid"),
        (
            "graph=http://example.org/GraphDefinition/g",
            "not-supported",
        ),
        ("id=other", "invalid"),
        ("count=1", "not-supported"),
    ] {
        let (status, outcome) = get(
            &harness,
            &format!("/fhir/Composition/{id}/$document?{query}"),
        )
        .await?;
        assert_eq!(StatusCode::BAD_REQUEST, status, "{query}: {outcome}");
        assert_eq!(
            Some("OperationOutcome"),
            outcome["resourceType"].as_str(),
            "{query}: {outcome}"
        );
        assert_eq!(
            Some(code),
            outcome["issue"][0]["code"].as_str(),
            "{query}: {outcome}"
        );
    }
    Ok(())
}

#[tokio::test]
async fn the_type_level_form_without_an_id_or_on_another_server_is_refused()
-> Result<(), Box<dyn Error>> {
    let harness = harness_over(DOCUMENT, settings()).await;
    let (status, outcome) = get(&harness, "/fhir/Composition/$document").await?;
    assert_eq!(StatusCode::BAD_REQUEST, status, "{outcome}");
    assert_eq!(Some("required"), outcome["issue"][0]["code"].as_str());
    let (status, outcome) = get(
        &harness,
        "/fhir/Composition/$document?id=https%3A%2F%2Felsewhere.example%2Ffhir%2FComposition%2Fc-1",
    )
    .await?;
    assert_eq!(StatusCode::BAD_REQUEST, status, "{outcome}");
    assert_eq!(Some("not-supported"), outcome["issue"][0]["code"].as_str());
    Ok(())
}

#[tokio::test]
async fn document_is_an_operation_of_composition_alone() -> Result<(), Box<dyn Error>> {
    let harness = harness_over(DOCUMENT, settings()).await;
    let (status, outcome) = get(&harness, "/fhir/Condition/c-1/$document").await?;
    assert_eq!(StatusCode::NOT_FOUND, status, "{outcome}");
    assert_eq!(Some("not-supported"), outcome["issue"][0]["code"].as_str());
    Ok(())
}

#[tokio::test]
async fn an_unknown_composition_is_not_found() -> Result<(), Box<dyn Error>> {
    let harness = harness_over(DOCUMENT, settings()).await;
    let (status, outcome) = get(&harness, "/fhir/Composition/unknown/$document").await?;
    assert_eq!(StatusCode::NOT_FOUND, status, "{outcome}");
    Ok(())
}

#[tokio::test]
async fn the_capability_statement_declares_document_on_composition() -> Result<(), Box<dyn Error>> {
    let harness = harness_over(DOCUMENT, settings()).await;
    let (status, statement) = get(&harness, "/fhir/metadata").await?;
    assert_eq!(StatusCode::OK, status, "{statement}");
    let resources = statement["rest"][0]["resource"]
        .as_array()
        .expect("the statement names resources");
    let composition = resources
        .iter()
        .find(|resource| resource["type"] == "Composition")
        .expect("the statement names Composition");
    let operations: Vec<(&str, &str)> = composition["operation"]
        .as_array()
        .expect("Composition names operations")
        .iter()
        .filter_map(|operation| {
            Some((
                operation["name"].as_str()?,
                operation["definition"].as_str()?,
            ))
        })
        .collect();
    assert!(
        operations.contains(&(
            "document",
            "http://hl7.org/fhir/OperationDefinition/Composition-document"
        )),
        "{operations:?}"
    );
    let condition = resources
        .iter()
        .find(|resource| resource["type"] == "Condition")
        .expect("the statement names the created Condition");
    assert!(
        condition["operation"]
            .as_array()
            .is_none_or(|operations| operations
                .iter()
                .all(|operation| operation["name"] != "document")),
        "no other type declares document: {condition}"
    );
    Ok(())
}
