// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The party resources through the facade against the reference CDR's
//! demographic store, behind the `FERROBRIDGE_E2E` gate (ITS-REST 1.1.0
//! Demographic API, DEVELOPMENT).
//!
//! The testkit's synthetic party templates are uploaded, and `Patient`,
//! `Practitioner`, `Organization` and `PractitionerRole` each create, read,
//! update, vread, list their history and delete through the facade. The
//! `PractitionerRole`'s practitioner is a `Practitioner` created first, which
//! becomes the `ROLE`'s mandatory `performer` (RM Demographic IM, the `ROLE`
//! class). No specification governs a party context: our own design.

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

use crate::facade_e2e::app;
use crate::facade_e2e::call;
use crate::facade_e2e::upload_published;

/// The demographic fixture mapping set.
const FIXTURES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/it/fixtures_demographic");

/// Sends a request with a FHIR JSON body.
pub(crate) fn with_body(
    method: http::Method,
    uri: &str,
    body: &serde_json::Value,
    if_match: Option<&str>,
) -> Result<Request<Body>, Box<dyn Error>> {
    let mut request = Request::builder()
        .method(method)
        .uri(uri)
        .header(header::CONTENT_TYPE, "application/fhir+json");
    if let Some(tag) = if_match {
        request = request.header(header::IF_MATCH, tag);
    }
    Ok(request.body(Body::from(body.to_string()))?)
}

/// Returns `resource` with an identifier and `name` set as `type` names it.
fn resource(resource_type: &str, name: &str) -> serde_json::Value {
    let identifier = serde_json::json!([{"system": "http://example.org/fhir/sid/synthetic", "value": "SYN-0001"}]);
    match resource_type {
        "Organization" => serde_json::json!({
            "resourceType": resource_type, "identifier": identifier, "name": name
        }),
        _ => serde_json::json!({
            "resourceType": resource_type, "identifier": identifier, "name": [{"text": name}]
        }),
    }
}

/// Returns a `PractitionerRole` coded `name` whose practitioner is the
/// `Practitioner` the facade serves under `performer`.
fn practitioner_role(name: &str, performer: &str) -> serde_json::Value {
    serde_json::json!({
        "resourceType": "PractitionerRole",
        "identifier": [{"system": "http://example.org/fhir/sid/synthetic", "value": "SYN-0001"}],
        "code": [{"text": name}],
        "practitioner": {"reference": format!("Practitioner/{performer}")}
    })
}

/// Returns the name `body` carries, as `type` spells it.
fn name_of<'b>(resource_type: &str, body: &'b serde_json::Value) -> &'b serde_json::Value {
    match resource_type {
        "Organization" => &body["name"],
        "PractitionerRole" => &body["code"][0]["text"],
        _ => &body["name"][0]["text"],
    }
}

/// Runs the whole lifecycle of one resource type through the facade.
async fn lifecycle(
    facade: &Arc<Facade>,
    resource_type: &str,
    make: &dyn Fn(&str) -> serde_json::Value,
) -> Result<(), Box<dyn Error>> {
    let first_body = make("Synthetic First");
    let base = format!("/fhir/{resource_type}");
    let created = call(
        app(facade),
        with_body(http::Method::POST, &base, &first_body, None)?,
    )
    .await?;
    assert_eq!(
        StatusCode::CREATED,
        created.0,
        "{resource_type}: {}",
        created.1
    );
    let id = created.1["id"]
        .as_str()
        .ok_or("the created resource carries an id")?
        .to_owned();
    assert_eq!(created.1["meta"]["versionId"], "1", "{}", created.1);

    let read = call(
        app(facade),
        Request::get(format!("{base}/{id}")).body(Body::empty())?,
    )
    .await?;
    assert_eq!(StatusCode::OK, read.0, "{resource_type}: {}", read.1);
    assert_eq!(
        name_of(resource_type, &read.1),
        "Synthetic First",
        "{}",
        read.1
    );
    if let Some(practitioner) = first_body.get("practitioner") {
        assert_eq!(&read.1["practitioner"], practitioner, "{}", read.1);
    }

    let mut revised = make("Synthetic Second");
    revised["id"] = serde_json::Value::from(id.as_str());
    let updated = call(
        app(facade),
        with_body(
            http::Method::PUT,
            &format!("{base}/{id}"),
            &revised,
            Some("W/\"1\""),
        )?,
    )
    .await?;
    assert_eq!(StatusCode::OK, updated.0, "{resource_type}: {}", updated.1);
    assert_eq!(updated.1["meta"]["versionId"], "2", "{}", updated.1);

    let first = call(
        app(facade),
        Request::get(format!("{base}/{id}/_history/1")).body(Body::empty())?,
    )
    .await?;
    assert_eq!(StatusCode::OK, first.0, "{resource_type}: {}", first.1);
    assert_eq!(
        name_of(resource_type, &first.1),
        "Synthetic First",
        "{}",
        first.1
    );

    let history = call(
        app(facade),
        Request::get(format!("{base}/{id}/_history")).body(Body::empty())?,
    )
    .await?;
    assert_eq!(StatusCode::OK, history.0, "{resource_type}: {}", history.1);
    assert_eq!(history.1["total"], 2, "{}", history.1);

    let deleted = call(
        app(facade),
        Request::delete(format!("{base}/{id}")).body(Body::empty())?,
    )
    .await?;
    assert_eq!(
        StatusCode::NO_CONTENT,
        deleted.0,
        "{resource_type}: {}",
        deleted.1
    );
    let after = call(
        app(facade),
        Request::get(format!("{base}/{id}")).body(Body::empty())?,
    )
    .await?;
    assert!(
        after.0 == StatusCode::GONE || after.0 == StatusCode::NOT_FOUND,
        "{resource_type} after its delete: {} {}",
        after.0,
        after.1
    );
    Ok(())
}

#[tokio::test]
async fn the_party_resources_live_in_the_reference_cdrs_demographic_store()
-> Result<(), Box<dyn Error>> {
    if !containers::e2e_enabled() {
        return Ok(());
    }
    let cdr = containers::cdr().await?;
    for opt in [
        fixtures::PERSON_OPT,
        fixtures::ORGANISATION_OPT,
        fixtures::ROLE_OPT,
    ] {
        upload_published(cdr.base_url(), opt).await?;
    }
    let client = CdrClient::new(&CdrConfig::new(cdr.base_url().parse()?).with_demographic(true))?;
    let set = programs::read_set(std::path::Path::new(FIXTURES))?;
    let templates = programs::fetch_templates(&set, &client).await?;
    let compiled = programs::compile_set(&set, &templates)?;
    programs::check_parties(&compiled, true)?;
    let store: Arc<dyn Store> = Arc::new(MemoryStore::new());
    let facade = Arc::new(Facade::new(
        compiled,
        store,
        client.clone(),
        Settings {
            base_url: String::from("http://ferrobridge.invalid/fhir"),
            ehr_policy: Policy::Existing,
            subject: crate::support::subject_link("http://example.org/fhir"),
            system_id: String::from("ferrobridge.e2e"),
            language: String::from("en"),
            territory: String::from("GB"),
            ehr_status_system: None,
        },
    ));
    for resource_type in ["Patient", "Practitioner", "Organization"] {
        lifecycle(&facade, resource_type, &|name| {
            resource(resource_type, name)
        })
        .await?;
    }

    let performer = call(
        app(&facade),
        with_body(
            http::Method::POST,
            "/fhir/Practitioner",
            &resource("Practitioner", "Synthetic Performer"),
            None,
        )?,
    )
    .await?;
    assert_eq!(StatusCode::CREATED, performer.0, "{}", performer.1);
    let performer = performer.1["id"].as_str().ok_or("an id")?.to_owned();
    lifecycle(&facade, "PractitionerRole", &|name| {
        practitioner_role(name, &performer)
    })
    .await?;
    Ok(())
}
