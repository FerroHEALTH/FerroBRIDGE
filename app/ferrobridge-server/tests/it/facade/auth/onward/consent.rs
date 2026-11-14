// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The access directive exchange under the onward lane: a person's
//! `Consent` reaches the CDR's `EHR_ACCESS` read and the CONTRIBUTION under
//! the token exchanged for that person, so the CDR decides for the caller,
//! and only the controller's policy is read under the bridge's own token.

use std::error::Error;
use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use ferrobridge_server::cdr::CdrClient;
use ferrobridge_server::consent::exchange::Settings as ExchangeSettings;
use ferrobridge_server::facade::Facade;
use ferrobridge_server::facade::identity::store::MemoryStore;
use ferrobridge_server::facade::programs::Programs;
use ferrobridge_server::smart::Smart;
use ferrobridge_server::state::AppState;
use ferrohealth_health::registry::Registry;
use http::Request;
use http::StatusCode;
use http::header;
use openehr_rm::draft::access_control::terminology::AccessDefault;
use wiremock::Mock;
use wiremock::ResponseTemplate;
use wiremock::matchers;

use super::super::authority;
use super::super::bearing;
use super::super::mint;
use super::Grant;
use super::OWN_TOKEN;
use super::World;
use crate::facade::EHR_ID;
use crate::facade::handle;
use crate::facade::raw;
use crate::facade::settings;
use crate::facade::stub::mount_ehr;

/// The CDR's extensions root, under the stub, as the reference CDR serves it.
const EXTENSIONS: &str = "/ferroehr/rest/extensions/v1";

/// An `EHR_ACCESS` with no settings.
const EMPTY_ACCESS: &str = r#"{"_type": "EHR_ACCESS", "name": {"_type": "DV_TEXT", "value": "EHR access"}, "archetype_node_id": "openEHR-EHR-EHR_ACCESS.access_directives.v1.0.0"}"#;

/// Returns the shipped router over a facade serving `Consent` through the
/// CDR lane of `world`, with `authority` guarding it.
fn app(world: &World, authority: &Arc<Smart>) -> Result<Router, Box<dyn Error>> {
    let store = Arc::new(MemoryStore::default());
    let facade = Facade::new(
        Programs::default(),
        handle(&store),
        CdrClient::new(&world.cdr_config)?,
        settings(),
    )
    .with_consent(Some(ExchangeSettings {
        extensions_root: url::Url::parse(&format!("{}{EXTENSIONS}", world.harness.cdr.uri()))?,
        default_access: AccessDefault::Restricted,
    }));
    let state = AppState::with_health(Registry::default())
        .with_facade(Arc::new(facade))
        .with_auth(Arc::clone(authority));
    Ok(ferrobridge_server::router(
        Arc::new(state),
        &crate::facade::server_settings(),
    ))
}

/// Mounts the CDR answers of one import into an EHR with no directives.
async fn mount_cdr(world: &World) {
    let cdr = &world.harness.cdr;
    mount_ehr(cdr).await;
    Mock::given(matchers::method("GET"))
        .and(matchers::path(format!(
            "{EXTENSIONS}/ehr/{EHR_ID}/ehr_access"
        )))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header(
                    "ETag",
                    "\"0a1b2c3d-0000-4000-8000-00000000000b::ferrobridge.test::1\"",
                )
                .set_body_raw(EMPTY_ACCESS, "application/json"),
        )
        .mount(cdr)
        .await;
    Mock::given(matchers::method("GET"))
        .and(matchers::path(format!("{EXTENSIONS}/admin/access_policy")))
        .respond_with(ResponseTemplate::new(404))
        .mount(cdr)
        .await;
    Mock::given(matchers::method("POST"))
        .and(matchers::path(format!("/ehr/{EHR_ID}/contribution")))
        .respond_with(
            ResponseTemplate::new(201)
                .insert_header("ETag", "\"6c7e5d4b-0000-4000-8000-000000000006\""),
        )
        .mount(cdr)
        .await;
}

/// Returns the `Authorization` the CDR received on `method` `path`.
async fn credential_of(world: &World, method: &str, path: &str) -> Option<String> {
    world
        .harness
        .cdr
        .received_requests()
        .await
        .unwrap_or_default()
        .iter()
        .find(|request| request.method.as_str() == method && request.url.path() == path)
        .and_then(|request| request.headers.get(header::AUTHORIZATION))
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned)
}

#[tokio::test]
async fn a_persons_consent_reaches_the_ehr_access_under_the_exchanged_token()
-> Result<(), Box<dyn Error>> {
    let world = World::start(Grant::Standard, "").await?;
    let issued = mint(&world.key, &world.issued_claims(Grant::Standard, 300));
    world
        .mount_token_endpoint(Grant::Standard, &issued, 300)
        .await;
    mount_cdr(&world).await;
    let mut claims = world.person_claims();
    claims["scope"] = serde_json::json!("user/Consent.crs");
    let token = mint(&world.key, &claims);
    let authority = authority(world.trusted.clone()).await;
    let response = raw(
        app(&world, &authority)?,
        bearing(
            Request::post("/fhir/Consent")
                .header(header::CONTENT_TYPE, "application/fhir+json")
                .body(Body::from(crate::consent::fhir::vendored(
                    ferrobridge_server::consent::fhir::Release::R4,
                    "consent-example-notTime.json",
                )))?,
            &token,
        ),
    )
    .await?;
    assert_eq!(StatusCode::CREATED, response.status());
    let exchanged = Some(format!("Bearer {issued}"));
    assert_eq!(
        exchanged,
        credential_of(
            &world,
            "GET",
            &format!("{EXTENSIONS}/ehr/{EHR_ID}/ehr_access")
        )
        .await,
        "the directives are read for the caller"
    );
    assert_eq!(
        exchanged,
        credential_of(&world, "POST", &format!("/ehr/{EHR_ID}/contribution")).await,
        "the directives are written for the caller"
    );
    assert_eq!(
        Some(format!("Bearer {}", OWN_TOKEN.as_str())),
        credential_of(&world, "GET", &format!("{EXTENSIONS}/admin/access_policy")).await,
        "only the controller's policy is read under the bridge's own token"
    );
    Ok(())
}
