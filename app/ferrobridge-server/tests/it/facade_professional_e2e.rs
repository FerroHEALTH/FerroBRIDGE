// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! A `Practitioner` over the reference CDR's own professional `PERSON`,
//! behind the `FERROBRIDGE_E2E` gate: the id-coded ADL 2 archetype the reference CDR
//! loads for its professionals is uploaded on the `adl2` template route
//! (ITS-REST 1.1.0 Definition API), which is the only route it is served on,
//! and a `Practitioner` creates, reads, updates and reads back its first
//! version through a party context whose paths name the archetype's id-codes
//! (ADL 2.4, §4.2.5 Node Identifier and Coding Systems).

use std::error::Error;
use std::sync::Arc;

use axum::body::Body;
use ferrobridge_server::cdr::CdrClient;
use ferrobridge_server::cdr::config::CdrConfig;
use ferrobridge_server::facade::Facade;
use ferrobridge_server::facade::Settings;
use ferrobridge_server::facade::ehr::Policy;
use ferrobridge_server::facade::identity::store::MemoryStore;
use ferrobridge_server::facade::identity::store::Store;
use ferrobridge_server::facade::programs;
use ferrobridge_testkit::containers;
use ferrobridge_testkit::fixtures;
use http::Request;
use http::StatusCode;
use http::header;

use crate::facade_demographic_e2e::with_body;
use crate::facade_e2e::app;
use crate::facade_e2e::call;

/// The professional mapping set.
const FIXTURES: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/it/fixtures_professional"
);

/// Uploads the professional `PERSON` on the CDR's `adl2` template route.
async fn upload_professional(base_url: &str) -> Result<(), Box<dyn Error>> {
    let response = reqwest::Client::new()
        .post(format!("{base_url}/definition/template/adl2"))
        .header(header::CONTENT_TYPE, "text/plain")
        .body(fixtures::FERROEHR_PROFESSIONAL_PERSON_ADL2)
        .send()
        .await?;
    assert_eq!(
        StatusCode::CREATED,
        response.status(),
        "the CDR refused its professional PERSON: {}",
        response.text().await?
    );
    Ok(())
}

/// Returns a synthetic `Practitioner` named `name`.
fn practitioner(name: &str) -> serde_json::Value {
    serde_json::json!({
        "resourceType": "Practitioner",
        "identifier": [{"system": "http://example.org/fhir/sid/synthetic", "value": "SYN-PRO-0001"}],
        "name": [{"text": name}]
    })
}

#[tokio::test]
async fn a_practitioner_round_trips_over_the_reference_cdrs_professional_person()
-> Result<(), Box<dyn Error>> {
    if !containers::e2e_enabled() {
        return Ok(());
    }
    let cdr = containers::cdr().await?;
    upload_professional(cdr.base_url()).await?;
    let client = CdrClient::new(&CdrConfig::new(cdr.base_url().parse()?).with_demographic(true))?;
    let set = programs::read_set(std::path::Path::new(FIXTURES))?;
    let templates = programs::fetch_templates(&set, &client).await?;
    let compiled = programs::compile_set(&set, &templates)?;
    programs::check_parties(&compiled, true)?;
    let store: Arc<dyn Store> = Arc::new(MemoryStore::default());
    let facade = Arc::new(Facade::new(
        compiled,
        store,
        client,
        Settings {
            base_url: String::from("http://ferrobridge.invalid/fhir"),
            ehr_policy: Policy::Existing,
            subject: crate::support::subject_link("http://example.org/fhir"),
            system_id: String::from("ferrobridge.e2e"),
            language: String::from("en"),
            territory: String::from("GB"),
            ehr_status_system: None,
            custodian: crate::facade::custodian(),
        },
    ));

    let created = call(
        app(&facade),
        with_body(
            http::Method::POST,
            "/fhir/Practitioner",
            &practitioner("Synthetic Professional"),
            None,
        )?,
    )
    .await?;
    assert_eq!(StatusCode::CREATED, created.0, "{}", created.1);
    let id = created.1["id"].as_str().ok_or("an id")?.to_owned();

    let read = call(
        app(&facade),
        Request::get(format!("/fhir/Practitioner/{id}")).body(Body::empty())?,
    )
    .await?;
    assert_eq!(StatusCode::OK, read.0, "{}", read.1);
    assert_eq!(
        "Synthetic Professional", read.1["name"][0]["text"],
        "{}",
        read.1
    );
    assert_eq!(
        "SYN-PRO-0001", read.1["identifier"][0]["value"],
        "{}",
        read.1
    );

    let mut revised = practitioner("Synthetic Professional Revised");
    revised["id"] = serde_json::Value::from(id.as_str());
    let updated = call(
        app(&facade),
        with_body(
            http::Method::PUT,
            &format!("/fhir/Practitioner/{id}"),
            &revised,
            Some("W/\"1\""),
        )?,
    )
    .await?;
    assert_eq!(StatusCode::OK, updated.0, "{}", updated.1);

    let first = call(
        app(&facade),
        Request::get(format!("/fhir/Practitioner/{id}/_history/1")).body(Body::empty())?,
    )
    .await?;
    assert_eq!(StatusCode::OK, first.0, "{}", first.1);
    assert_eq!(
        "Synthetic Professional", first.1["name"][0]["text"],
        "{}",
        first.1
    );
    Ok(())
}
