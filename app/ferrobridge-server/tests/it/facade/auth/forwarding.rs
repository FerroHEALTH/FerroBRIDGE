// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The caller's access token stays at the facade: every CDR call carries the
//! bridge's own credential, because an access token names the resource server
//! it is for (RFC 9068 §4, <https://www.rfc-editor.org/rfc/rfc9068#section-4>)
//! and SMART App Launch 2.x scopes grant access at the server that receives
//! the token
//! (<https://hl7.org/fhir/smart-app-launch/scopes-and-launch-context.html>).

use std::error::Error as StdError;
use std::sync::Arc;

use axum::body::Body;
use ferrobridge_server::cdr::CdrClient;
use ferrobridge_server::cdr::config::CdrConfig;
use ferrobridge_server::facade::Facade;
use ferrobridge_server::facade::identity::store::MemoryStore;
use ferrobridge_server::facade::programs;
use ferrobridge_server::facade::search::Limits;
use http::Request;
use http::StatusCode;
use http::header;
use jsonwebtoken::Algorithm;
use wiremock::Mock;
use wiremock::MockServer;
use wiremock::ResponseTemplate;
use wiremock::matchers;

use super::Issuer;
use super::authority;
use super::bearing;
use super::condition_of;
use super::ec_key;
use super::get;
use super::mint;
use super::secured;
use crate::facade::FIXTURES;
use crate::facade::Harness;
use crate::facade::VERSION_ONE;
use crate::facade::batch::batch_of;
use crate::facade::batch::entry;
use crate::facade::batch::mount_create_once;
use crate::facade::batch::statuses;
use crate::facade::client;
use crate::facade::handle;
use crate::facade::post_condition;
use crate::facade::raw;
use crate::facade::settings;
use crate::facade::stub::mount_ehr;
use crate::facade::stub::mount_read;

/// The basic authentication password the bridge is configured to reach the
/// CDR with, as user `bridge`.
const BRIDGE_CREDENTIAL: &str = "synthetic-bridge-credential";

/// The `Authorization` value [`BRIDGE_CREDENTIAL`] travels as: RFC 7617 §2,
/// base64 of `bridge:synthetic-bridge-credential`.
const BRIDGE_AUTHORIZATION: &str = "Basic YnJpZGdlOnN5bnRoZXRpYy1icmlkZ2UtY3JlZGVudGlhbA==";

/// Returns a harness whose facade reaches its CDR with [`BRIDGE_CREDENTIAL`]
/// and answers search as the fixture declarations state.
async fn credentialed() -> Result<Harness, Box<dyn StdError>> {
    let cdr = MockServer::start().await;
    Mock::given(matchers::method("GET"))
        .and(matchers::path(
            "/definition/template/adl1.4/ferrobridge.diagnose.v1",
        ))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("Content-Type", "application/xml")
                .set_body_string(ferrobridge_testkit::fixtures::DIAGNOSE_OPT),
        )
        .mount(&cdr)
        .await;
    let set = programs::read_set(std::path::Path::new(FIXTURES))?;
    let templates = programs::fetch_templates(&set, &client(&cdr)).await?;
    let compiled = programs::compile_set(&set, &templates)?;
    let bridge = CdrClient::new(
        &CdrConfig::new(format!("{}/", cdr.uri()).parse()?).with_basic("bridge", BRIDGE_CREDENTIAL),
    )?;
    let store = Arc::new(MemoryStore::new());
    let built = Facade::new(compiled, handle(&store), bridge, settings());
    let catalog = ferrobridge_server::facade::search::load(
        std::path::Path::new(FIXTURES),
        &built.programs(),
    )?;
    let facade = Arc::new(built.with_search(catalog, Limits::default()));
    Ok(Harness { cdr, store, facade })
}

/// Returns the `Authorization` value of every request the CDR received after
/// the template fetch, with the method and path it came with.
async fn cdr_credentials(harness: &Harness) -> Vec<(String, Option<String>)> {
    harness
        .cdr
        .received_requests()
        .await
        .unwrap_or_default()
        .iter()
        .filter(|request| !request.url.path().starts_with("/definition/"))
        .map(|request| {
            (
                format!("{} {}", request.method, request.url.path()),
                request
                    .headers
                    .get(header::AUTHORIZATION)
                    .and_then(|value| value.to_str().ok())
                    .map(str::to_owned),
            )
        })
        .collect()
}

/// Asserts that every CDR call carries the bridge's credential and none the
/// caller's token, and that `expected` calls reached the CDR.
fn assert_bridge_credential_only(
    calls: &[(String, Option<String>)],
    token: &str,
    expected: &[&str],
) {
    for wanted in expected {
        assert!(
            calls.iter().any(|(call, _)| call.starts_with(wanted)),
            "no {wanted} reached the CDR: {calls:?}"
        );
    }
    for (call, credential) in calls {
        assert_eq!(
            Some(BRIDGE_AUTHORIZATION),
            credential.as_deref(),
            "{call} carries the bridge's credential"
        );
        assert!(
            !credential.as_deref().unwrap_or_default().contains(token),
            "{call} carries the caller's token"
        );
    }
}

/// Mounts the AQL endpoint answering an empty result set.
async fn mount_empty_query(cdr: &MockServer) {
    Mock::given(matchers::method("POST"))
        .and(matchers::path("/query/aql"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "q": "synthetic",
            "rows": []
        })))
        .mount(cdr)
        .await;
}

#[tokio::test]
async fn create_read_and_search_reach_the_cdr_with_the_bridge_credential_alone()
-> Result<(), Box<dyn StdError>> {
    let key = ec_key("k1", Algorithm::ES256);
    let issuer = Issuer::start(&[&key]).await;
    let harness = credentialed().await?;
    let authority = authority(issuer.settings()).await;
    let resource = condition_of("p-1");
    mount_ehr(&harness.cdr).await;
    mount_create_once(
        &harness.cdr,
        VERSION_ONE,
        &harness.composition_of(&resource),
    )
    .await;
    mount_read(
        &harness.cdr,
        VERSION_ONE,
        &harness.composition_of(&resource),
    )
    .await;
    mount_empty_query(&harness.cdr).await;
    let token = mint(&key, &issuer.claims("system/Condition.crs"));

    let created = raw(
        secured(&harness, &authority),
        bearing(post_condition(&resource), &token),
    )
    .await?;
    assert_eq!(StatusCode::CREATED, created.status());
    let bytes = axum::body::to_bytes(created.into_body(), 1024 * 1024).await?;
    let id = serde_json::from_slice::<serde_json::Value>(&bytes)?["id"]
        .as_str()
        .ok_or("an id")?
        .to_owned();
    let read = raw(
        secured(&harness, &authority),
        bearing(get(&format!("/fhir/Condition/{id}")), &token),
    )
    .await?;
    assert_eq!(StatusCode::OK, read.status());
    let search = raw(
        secured(&harness, &authority),
        bearing(get("/fhir/Condition"), &token),
    )
    .await?;
    assert_eq!(StatusCode::OK, search.status());

    assert_bridge_credential_only(
        &cdr_credentials(&harness).await,
        &token,
        &["GET /ehr", "POST /ehr/", "GET /ehr/", "POST /query/aql"],
    );
    Ok(())
}

#[tokio::test]
async fn each_batch_entry_reaches_the_cdr_with_the_bridge_credential_alone()
-> Result<(), Box<dyn StdError>> {
    let key = ec_key("k1", Algorithm::ES256);
    let issuer = Issuer::start(&[&key]).await;
    let harness = credentialed().await?;
    let authority = authority(issuer.settings()).await;
    let resource = condition_of("p-1");
    mount_ehr(&harness.cdr).await;
    mount_create_once(
        &harness.cdr,
        VERSION_ONE,
        &harness.composition_of(&resource),
    )
    .await;
    mount_empty_query(&harness.cdr).await;
    let token = mint(&key, &issuer.claims("system/Condition.crs"));
    let bundle = batch_of(&[
        entry("POST", "Condition", Some(resource)),
        entry("GET", "Condition", None),
    ]);
    let request = Request::post("/fhir")
        .header(header::CONTENT_TYPE, "application/fhir+json")
        .body(Body::from(bundle.to_string()))?;

    let response = raw(secured(&harness, &authority), bearing(request, &token)).await?;
    assert_eq!(StatusCode::OK, response.status());
    let bytes = axum::body::to_bytes(response.into_body(), 1024 * 1024).await?;
    let body: serde_json::Value = serde_json::from_slice(&bytes)?;
    assert_eq!(vec!["201 Created", "200 OK"], statuses(&body), "{body}");

    assert_bridge_credential_only(
        &cdr_credentials(&harness).await,
        &token,
        &["GET /ehr", "POST /ehr/", "POST /query/aql"],
    );
    Ok(())
}
