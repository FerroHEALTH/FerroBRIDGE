// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Search against a real CDR, behind the `FERROBRIDGE_E2E` gate.
//!
//! The `wiremock` suite pins the AQL each parameter writes and every refusal;
//! this case proves the projection runs on a CDR: two synthetic `Condition`s
//! commit through the facade, and `GET Condition?patient=…` pages them back
//! across a page boundary in a stable order, mapped through the same program.

use axum::Router;
use axum::body::Body;
use ferrobridge_server::cdr::CdrClient;
use ferrobridge_server::cdr::config::CdrConfig;
use ferrobridge_server::facade::Facade;
use ferrobridge_server::facade::Settings;
use ferrobridge_server::facade::ehr::Policy;
use ferrobridge_server::facade::identity::store::MemoryStore;
use ferrobridge_server::facade::identity::store::Store;
use ferrobridge_server::facade::programs;
use ferrobridge_server::facade::search;
use ferrobridge_server::health::Registry;
use ferrobridge_server::state::AppState;
use ferrobridge_testkit::containers;
use ferrobridge_testkit::fixtures;
use http::Request;
use http::StatusCode;
use http::header;
use std::error::Error as StdError;
use std::sync::Arc;
use tower::ServiceExt as _;

/// The fixture directory the case loads its mapping set and its search
/// declaration from.
const FIXTURES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/it/fixtures");

/// The profile the fixture context claims.
const PROFILE: &str = "http://example.org/fhir/StructureDefinition/ferrobridge-facade-diagnosis";

/// The namespace a literal subject reference is keyed in.
const NAMESPACE: &str = "http://example.org/fhir";

/// The FHIR service base the facade answers under.
const BASE: &str = "http://ferrobridge.invalid/fhir/";

#[tokio::test]
async fn a_patient_search_pages_the_committed_conditions_back_from_a_real_cdr()
-> Result<(), Box<dyn StdError>> {
    if !containers::e2e_enabled() {
        return Ok(());
    }
    let cdr = containers::cdr().await?;
    crate::facade_e2e::upload_template(cdr.base_url()).await?;
    let client = CdrClient::new(&CdrConfig::new(cdr.base_url().parse()?))?;
    let set = programs::read_set(std::path::Path::new(FIXTURES))?;
    let templates = programs::fetch_templates(&set, &client).await?;
    let compiled = programs::compile_set(&set, &templates)?;
    let catalog = search::load(std::path::Path::new(FIXTURES), &compiled)?;
    let store: Arc<dyn Store> = Arc::new(MemoryStore::new());
    let facade = Arc::new(
        Facade::new(
            compiled,
            store,
            client,
            Settings {
                base_url: String::from(BASE.trim_end_matches('/')),
                ehr_policy: Policy::CreateOnFirstWrite,
                subject_namespace: String::from(NAMESPACE),
                system_id: String::from("ferrobridge.e2e"),
                language: String::from("en"),
                territory: String::from("GB"),
            },
        )
        .with_search(catalog, search::Limits::default()),
    );

    let mut created = Vec::new();
    for (sender, code) in [("e2e-search-1", "SYN-001"), ("e2e-search-2", "SYN-002")] {
        let (status, body) = call(app(&facade), post_condition(sender, code)?).await?;
        assert_eq!(StatusCode::CREATED, status, "{body}");
        created.push(
            body["id"]
                .as_str()
                .ok_or("the create carries an id")?
                .to_owned(),
        );
    }

    let (status, first) = get(&facade, "Condition?patient=e2e-search-subject&_count=1").await?;
    assert_eq!(StatusCode::OK, status, "{first}");
    assert_eq!(1, matched(&first).len(), "{first}");
    let next = link(&first, "next").ok_or("two matches over a page of one leave a next page")?;
    let (status, second) = get(&facade, next.strip_prefix(BASE).unwrap_or(next)).await?;
    assert_eq!(StatusCode::OK, status, "{second}");
    assert_eq!(1, matched(&second).len(), "{second}");
    assert_eq!(None, link(&second, "next"));
    let mut found: Vec<String> = [&first, &second]
        .iter()
        .flat_map(|page| matched(page))
        .filter_map(|id| id.map(str::to_owned))
        .collect();
    found.sort();
    created.sort();
    assert_eq!(
        created, found,
        "the pages answer each committed Condition once"
    );

    let (status, all) = get(&facade, "Condition?patient=e2e-search-subject").await?;
    assert_eq!(StatusCode::OK, status, "{all}");
    assert_eq!(Some(2), all["total"].as_u64(), "{all}");

    let (status, none) = get(&facade, "Condition?patient=nobody").await?;
    assert_eq!(StatusCode::OK, status, "{none}");
    assert_eq!(Some(0), none["total"].as_u64());

    let (status, refused) = get(&facade, "Condition?patient=e2e-search-subject&severity=x").await?;
    assert_eq!(StatusCode::BAD_REQUEST, status, "{refused}");
    Ok(())
}

/// Returns a create of the synthetic `Condition` under the sender id
/// `sender`, coded `code`, its subject a literal reference.
fn post_condition(sender: &str, code: &str) -> Result<Request<Body>, Box<dyn StdError>> {
    let mut resource: serde_json::Value = serde_json::from_str(fixtures::R4_CONDITION)?;
    resource["id"] = serde_json::json!(sender);
    resource["meta"] = serde_json::json!({ "profile": [PROFILE] });
    resource["code"]["coding"][0]["code"] = serde_json::json!(code);
    resource["subject"] = serde_json::json!({ "reference": "Patient/e2e-search-subject" });
    Ok(Request::post("/fhir/Condition")
        .header(header::CONTENT_TYPE, "application/fhir+json")
        .body(Body::from(resource.to_string()))?)
}

/// Returns the logical id of every match of a searchset.
fn matched(body: &serde_json::Value) -> Vec<Option<&str>> {
    body["entry"]
        .as_array()
        .map(|entries| {
            entries
                .iter()
                .filter(|entry| entry["search"]["mode"].as_str() == Some("match"))
                .map(|entry| entry["resource"]["id"].as_str())
                .collect()
        })
        .unwrap_or_default()
}

/// Returns the `url` of the link whose relation is `relation`.
fn link<'a>(body: &'a serde_json::Value, relation: &str) -> Option<&'a str> {
    body["link"]
        .as_array()?
        .iter()
        .find(|entry| entry["relation"].as_str() == Some(relation))
        .and_then(|entry| entry["url"].as_str())
}

/// Sends `GET /fhir/{path_and_query}`.
async fn get(
    facade: &Arc<Facade>,
    path_and_query: &str,
) -> Result<(StatusCode, serde_json::Value), Box<dyn StdError>> {
    call(
        app(facade),
        Request::get(format!("/fhir/{path_and_query}")).body(Body::empty())?,
    )
    .await
}

/// Returns the application over `facade`.
fn app(facade: &Arc<Facade>) -> Router {
    let state = AppState::with_health(Registry::default()).with_facade(Arc::clone(facade));
    ferrobridge_server::router(Arc::new(state), &crate::support::settings())
}

/// Sends `request` and reads the status and the JSON body.
async fn call(
    app: Router,
    request: Request<Body>,
) -> Result<(StatusCode, serde_json::Value), Box<dyn StdError>> {
    let response = app.oneshot(request).await?;
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), 1024 * 1024).await?;
    Ok((status, serde_json::from_slice(&bytes)?))
}
