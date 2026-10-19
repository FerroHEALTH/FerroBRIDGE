// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The SMART v2 scope per interaction and resource type: `c` for create,
//! `r` for read and vread, `u` for update, `c` or `u` for `$validate`
//! (<https://hl7.org/fhir/smart-app-launch/scopes-and-launch-context.html>).
//! A token without the permission answers `403 insufficient_scope` (RFC 6750
//! §3.1) whose body and challenge name no scope.

use std::error::Error as StdError;
use std::sync::Arc;

use axum::body::Body;
use ferrobridge_server::health::Registry;
use ferrobridge_server::smart::Smart;
use ferrobridge_server::state::AppState;
use ferrobridge_server::state::OperationsLane;
use fhirconnect::operations::programs::ProgramSet;
use http::Request;
use http::StatusCode;
use http::header;
use jsonwebtoken::Algorithm;

use super::Issuer;
use super::SigningKey;
use super::authority;
use super::bearing;
use super::challenge;
use super::ec_key;
use super::get;
use super::mint;
use super::secured;
use crate::facade::Harness;
use crate::facade::VALIDATE;
use crate::facade::VERSION_ONE;
use crate::facade::condition;
use crate::facade::condition_at;
use crate::facade::harness;
use crate::facade::named;
use crate::facade::post_condition;
use crate::facade::raw;
use crate::facade::stub::mount_create;
use crate::facade::stub::mount_ehr;
use crate::facade::stub::mount_read;
use crate::facade::stub::mount_version_two;
use crate::facade::writes;

/// One case: the issuer, its key, the facade and the resource server.
struct Case {
    /// The stub authorization server.
    issuer: Issuer,
    /// The key every token is signed with.
    key: SigningKey,
    /// The facade.
    harness: Harness,
    /// The resource server.
    authority: Arc<Smart>,
}

impl Case {
    /// Returns a case whose CDR answers the create of the fixture Condition.
    async fn new() -> Self {
        Self::with(|_| {}).await
    }

    /// Returns a case whose resource server settings `adjust` changes.
    async fn with(adjust: impl FnOnce(&mut ferrobridge_server::smart::SmartSettings)) -> Self {
        let key = ec_key("k1", Algorithm::ES256);
        let issuer = Issuer::start(&[&key]).await;
        let harness = harness().await;
        mount_ehr(&harness.cdr).await;
        mount_create(&harness.cdr, VERSION_ONE, &harness.composition()).await;
        let mut settings = issuer.settings();
        adjust(&mut settings);
        let authority = authority(settings).await;
        Self {
            issuer,
            key,
            harness,
            authority,
        }
    }

    /// Returns a token granting `scope`.
    fn token(&self, scope: &str) -> String {
        mint(&self.key, &self.issuer.claims(scope))
    }

    /// Sends `request` under a token granting `scope` and returns the
    /// status, the challenge and the body text.
    async fn send(
        &self,
        request: Request<Body>,
        scope: &str,
    ) -> Result<(StatusCode, Option<String>, String), Box<dyn StdError>> {
        let response = raw(
            secured(&self.harness, &self.authority),
            bearing(request, &self.token(scope)),
        )
        .await?;
        let status = response.status();
        let challenge = challenge(&response);
        let bytes = axum::body::to_bytes(response.into_body(), 1024 * 1024).await?;
        Ok((
            status,
            challenge,
            String::from_utf8_lossy(&bytes).into_owned(),
        ))
    }
}

/// Asserts the `403` of a token without the permission.
fn assert_forbidden((status, challenge, body): &(StatusCode, Option<String>, String)) {
    assert_eq!(StatusCode::FORBIDDEN, *status, "{body}");
    assert_eq!(
        Some("Bearer error=\"insufficient_scope\""),
        challenge.as_deref(),
        "RFC 6750 §3.1 insufficient_scope, naming no scope"
    );
    assert!(body.contains("\"forbidden\""), "{body}");
    for named in ["Condition.", "system/", "scope", ".c", ".r"] {
        assert!(!body.contains(named), "the body names {named}: {body}");
    }
}

/// Returns a `PUT` of the fixture Condition at `id`.
fn put_condition(id: &str) -> Request<Body> {
    Request::put(format!("/fhir/Condition/{id}"))
        .header(header::CONTENT_TYPE, "application/fhir+json")
        .body(Body::from(named(condition(), id).to_string()))
        .expect("a legal request")
}

/// Returns a `$validate` of the fixture Condition.
fn validate_condition() -> Request<Body> {
    Request::post(VALIDATE)
        .header(header::CONTENT_TYPE, "application/fhir+json")
        .body(Body::from(condition().to_string()))
        .expect("a legal request")
}

#[tokio::test]
async fn create_needs_c_on_the_type() -> Result<(), Box<dyn StdError>> {
    let case = Case::new().await;
    for scope in ["system/Condition.rus", "system/Observation.c", "openid"] {
        assert_forbidden(&case.send(post_condition(&condition()), scope).await?);
    }
    let (status, _, body) = case
        .send(post_condition(&condition()), "system/Condition.c")
        .await?;
    assert_eq!(StatusCode::CREATED, status, "{body}");
    Ok(())
}

#[tokio::test]
async fn read_and_vread_need_r_on_the_type() -> Result<(), Box<dyn StdError>> {
    let case = Case::new().await;
    for path in [
        "/fhir/Condition/never-written",
        "/fhir/Condition/never-written/_history/1",
    ] {
        assert_forbidden(&case.send(get(path), "system/Condition.cus").await?);
        let (status, _, body) = case.send(get(path), "system/Condition.r").await?;
        assert_eq!(StatusCode::NOT_FOUND, status, "{path}: {body}");
    }
    Ok(())
}

#[tokio::test]
async fn update_needs_u_on_the_type() -> Result<(), Box<dyn StdError>> {
    let case = Case::new().await;
    assert_forbidden(
        &case
            .send(put_condition("never-written"), "system/Condition.crds")
            .await?,
    );
    let (status, _, body) = case
        .send(put_condition("never-written"), "system/Condition.u")
        .await?;
    assert_eq!(StatusCode::NOT_FOUND, status, "{body}");
    Ok(())
}

#[tokio::test]
async fn validate_needs_c_or_u_on_the_type() -> Result<(), Box<dyn StdError>> {
    let case = Case::new().await;
    assert_forbidden(
        &case
            .send(validate_condition(), "system/Condition.rds")
            .await?,
    );
    for scope in ["system/Condition.c", "system/Condition.u"] {
        let (status, _, body) = case.send(validate_condition(), scope).await?;
        assert_eq!(StatusCode::OK, status, "{scope}: {body}");
    }
    Ok(())
}

#[tokio::test]
async fn validate_admits_a_token_that_carries_only_the_operator_scope()
-> Result<(), Box<dyn StdError>> {
    // No specification governs the operator scope: our own design, a dry run
    // commits nothing, so the console's token needs no clinical write scope.
    let case = Case::new().await;
    let (status, _, body) = case
        .send(validate_condition(), "__ferrobridge.operator")
        .await?;
    assert_eq!(StatusCode::OK, status, "{body}");
    assert_forbidden(
        &case
            .send(post_condition(&condition()), "__ferrobridge.operator")
            .await?,
    );
    assert_forbidden(
        &case
            .send(
                get("/fhir/Condition/never-written"),
                "__ferrobridge.operator",
            )
            .await?,
    );
    Ok(())
}

#[tokio::test]
async fn a_renamed_operator_scope_is_the_one_validate_admits() -> Result<(), Box<dyn StdError>> {
    let case =
        Case::with(|settings| settings.operator_scope = String::from("__bridge.operator")).await;
    let (status, _, body) = case.send(validate_condition(), "__bridge.operator").await?;
    assert_eq!(StatusCode::OK, status, "{body}");
    assert_forbidden(
        &case
            .send(validate_condition(), "__ferrobridge.operator")
            .await?,
    );
    Ok(())
}

#[tokio::test]
async fn a_fhirconnect_operation_admits_a_token_with_only_the_operator_scope()
-> Result<(), Box<dyn StdError>> {
    let case = Case::new().await;
    let state = AppState::with_health(Registry::default())
        .serving_operations(OperationsLane::new(
            ProgramSet::new(),
            "Device/ferrobridge-test",
        ))
        .with_auth(Arc::clone(&case.authority));
    let app = ferrobridge_server::router(Arc::new(state), &crate::facade::server_settings());
    for path in ["/fhir/$toopenehr", "/fhir/$tofhir"] {
        let request = Request::post(path)
            .header(header::CONTENT_TYPE, "application/fhir+json")
            .body(Body::from(condition().to_string()))?;
        let response = raw(
            app.clone(),
            bearing(request, &case.token("__ferrobridge.operator")),
        )
        .await?;
        assert_ne!(StatusCode::UNAUTHORIZED, response.status(), "{path}");
        assert_ne!(StatusCode::FORBIDDEN, response.status(), "{path}");
    }
    Ok(())
}

#[tokio::test]
async fn the_wildcard_user_and_v1_scopes_grant_as_their_letters_say()
-> Result<(), Box<dyn StdError>> {
    let case = Case::new().await;
    let read = get("/fhir/Condition/never-written");
    let (status, _, _) = case.send(read, "system/*.cruds").await?;
    assert_eq!(StatusCode::NOT_FOUND, status);
    let (status, _, _) = case
        .send(get("/fhir/Condition/never-written"), "user/Condition.read")
        .await?;
    assert_eq!(StatusCode::NOT_FOUND, status, "v1 read is rs");
    assert_forbidden(
        &case
            .send(post_condition(&condition()), "user/Condition.read")
            .await?,
    );
    let (status, _, body) = case
        .send(post_condition(&condition()), "user/Condition.write")
        .await?;
    assert_eq!(StatusCode::CREATED, status, "v1 write is cud: {body}");
    Ok(())
}

#[tokio::test]
async fn a_scopeless_token_gets_system_rights_only_when_its_subject_is_listed()
-> Result<(), Box<dyn StdError>> {
    let unlisted = Case::new().await;
    assert_forbidden(&unlisted.send(post_condition(&condition()), "").await?);
    let listed = Case::with(|settings| {
        settings.issuers[0]
            .scopeless_subjects
            .insert(String::from("synthetic-client-sub"));
    })
    .await;
    let (status, _, body) = listed.send(post_condition(&condition()), "").await?;
    assert_eq!(StatusCode::CREATED, status, "{body}");
    Ok(())
}

#[tokio::test]
async fn a_fhirconnect_operation_needs_a_token() -> Result<(), Box<dyn StdError>> {
    let case = Case::new().await;
    let state = AppState::with_health(Registry::default())
        .with_facade(Arc::clone(&case.harness.facade))
        .serving_operations(OperationsLane::new(
            ProgramSet::new(),
            "Device/ferrobridge-test",
        ))
        .with_auth(Arc::clone(&case.authority));
    let app = ferrobridge_server::router(Arc::new(state), &crate::facade::server_settings());
    let request = Request::post("/fhir/$toopenehr")
        .header(header::CONTENT_TYPE, "application/fhir+json")
        .body(Body::from(condition().to_string()))?;
    let response = raw(app, request).await?;
    assert_eq!(StatusCode::UNAUTHORIZED, response.status());
    Ok(())
}

/// Returns a case whose CDR also answers the read and the update a revision
/// sends, with the fixture Condition created at `meta.versionId` 1.
async fn created_at_one() -> Result<Case, Box<dyn StdError>> {
    let case = Case::new().await;
    mount_read(&case.harness.cdr, VERSION_ONE, &case.harness.composition()).await;
    mount_version_two(&case.harness).await;
    let (status, _, body) = case
        .send(post_condition(&condition_at("1")), "system/Condition.c")
        .await?;
    assert_eq!(StatusCode::CREATED, status, "{body}");
    Ok(case)
}

#[tokio::test]
async fn a_create_that_revises_a_known_resource_needs_u_too() -> Result<(), Box<dyn StdError>> {
    let case = created_at_one().await?;
    assert_forbidden(
        &case
            .send(post_condition(&condition_at("2")), "system/Condition.c")
            .await?,
    );
    assert_eq!(
        (1, 0, 0),
        writes(&case.harness).await,
        "the refused revision committed nothing"
    );
    let (status, _, body) = case
        .send(post_condition(&condition_at("2")), "system/Condition.cu")
        .await?;
    assert_eq!(StatusCode::OK, status, "{body}");
    assert_eq!((1, 1, 0), writes(&case.harness).await, "one update PUT");
    Ok(())
}

#[tokio::test]
async fn a_create_that_replays_needs_only_c() -> Result<(), Box<dyn StdError>> {
    let case = created_at_one().await?;
    let (status, _, body) = case
        .send(post_condition(&condition_at("1")), "system/Condition.c")
        .await?;
    assert_eq!(StatusCode::OK, status, "a replay answers 200 OK: {body}");
    assert_eq!(
        (1, 0, 0),
        writes(&case.harness).await,
        "nothing more written"
    );
    Ok(())
}
