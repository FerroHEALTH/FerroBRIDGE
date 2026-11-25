// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The runtime settings registers on the operator surface and across
//! replicas: a read answers the version in force, a write names the version
//! it replaces, and a write on one replica is in force on another after the
//! notification. The cases that need the store run only when
//! `FERROBRIDGE_E2E=1` admits the harness. No specification governs the
//! register mechanics: our own design.

use std::collections::BTreeMap;
use std::error::Error;
use std::sync::Arc;
use std::time::Duration;

use axum::Router;
use axum::body::Body;
use ferrobridge_server::check::Report;
use ferrobridge_server::operator::Operator;
use ferrobridge_server::operator::snapshot::Snapshot;
use ferrobridge_server::settings::{Register, Registers, Written};
use ferrobridge_server::state::AppState;
use ferrohealth_health::registry::Registry;
use http::{Request, StatusCode, header};
use jsonwebtoken::Algorithm;
use tower::ServiceExt as _;

use super::{OPERATOR, bytes};
use crate::facade::auth::{Issuer, SigningKey, authority, bearing, ec_key, mint};

/// The longest a case waits on a notification.
const WAIT: Duration = Duration::from_secs(10);

/// A category map that maps one template to a national category.
fn categories() -> serde_json::Value {
    serde_json::json!({
        "national": [],
        "templates": {"ferrobridge.diagnose.v1": "none"},
        "archetypes": {},
    })
}

/// The operator API with its registers in `store`, when given.
struct Surface {
    /// The stub authorization server.
    issuer: Issuer,
    /// The signing key.
    key: SigningKey,
    /// The router under test.
    app: Router,
}

impl Surface {
    /// Builds the operator API, keeping its registers in `store`.
    async fn new(store: Option<&ferrobridge_server::store::Store>) -> Result<Self, Box<dyn Error>> {
        let key = ec_key("k1", Algorithm::ES256);
        let issuer = Issuer::start(&[&key]).await;
        let operator = Operator::new(
            Vec::new(),
            Snapshot::new(Report::default(), BTreeMap::new()),
        );
        let state = AppState::with_health(Registry::default())
            .with_auth(authority(issuer.settings()).await)
            .with_operator(Arc::new(operator));
        let state = match store {
            Some(store) => state.with_registers(store),
            None => state,
        };
        let app = ferrobridge_server::router(Arc::new(state), &crate::support::settings());
        Ok(Self { issuer, key, app })
    }

    /// Sends `request` under an operator token.
    async fn send(&self, request: Request<Body>) -> Result<http::Response<Body>, Box<dyn Error>> {
        let token = mint(&self.key, &self.issuer.claims(OPERATOR));
        Ok(self.app.clone().oneshot(bearing(request, &token)).await?)
    }

    /// Writes `document` to `audit-categories` naming `if_match`.
    async fn put(
        &self,
        if_match: Option<&str>,
        document: &serde_json::Value,
    ) -> Result<http::Response<Body>, Box<dyn Error>> {
        let mut request = Request::put("/operator/registers/audit-categories")
            .header(header::CONTENT_TYPE, "application/json");
        if let Some(tag) = if_match {
            request = request.header(header::IF_MATCH, tag);
        }
        self.send(request.body(Body::from(document.to_string()))?)
            .await
    }
}

/// Returns the `ETag` of `response`.
fn etag(response: &http::Response<Body>) -> Option<&str> {
    response
        .headers()
        .get(header::ETAG)
        .and_then(|value| value.to_str().ok())
}

#[tokio::test]
async fn without_a_state_store_the_registers_answer_409() -> Result<(), Box<dyn Error>> {
    let surface = Surface::new(None).await?;
    let response = surface
        .send(Request::get("/operator/registers/audit-categories").body(Body::empty())?)
        .await?;
    assert_eq!(StatusCode::CONFLICT, response.status());
    Ok(())
}

#[tokio::test]
async fn a_write_names_the_version_it_replaces_and_a_stale_one_answers_412()
-> Result<(), Box<dyn Error>> {
    let Some((_postgres, store)) = crate::store::store().await? else {
        return Ok(());
    };
    let surface = Surface::new(Some(&store)).await?;
    let read = surface
        .send(Request::get("/operator/registers/audit-categories").body(Body::empty())?)
        .await?;
    assert_eq!(StatusCode::OK, read.status());
    assert_eq!(Some("\"0\""), etag(&read));
    let default: serde_json::Value = serde_json::from_slice(&bytes(read).await?)?;
    assert_eq!(0, default["version"]);
    assert_eq!(
        Register::AuditCategories.default_document(),
        default["document"]
    );

    let written = surface.put(Some("\"0\""), &categories()).await?;
    assert_eq!(StatusCode::OK, written.status());
    assert_eq!(Some("\"1\""), etag(&written));
    let answered: serde_json::Value = serde_json::from_slice(&bytes(written).await?)?;
    assert_eq!("synthetic-client-sub", answered["committer"]);
    assert_eq!(categories(), answered["document"]);

    let stale = surface.put(Some("\"0\""), &categories()).await?;
    assert_eq!(StatusCode::PRECONDITION_FAILED, stale.status());
    let unconditional = surface.put(None, &categories()).await?;
    assert_eq!(StatusCode::PRECONDITION_REQUIRED, unconditional.status());
    let refused = surface
        .put(Some("\"1\""), &serde_json::json!({"shipped": true}))
        .await?;
    assert_eq!(StatusCode::BAD_REQUEST, refused.status());
    let unknown = surface
        .send(Request::get("/operator/registers/access-delay").body(Body::empty())?)
        .await?;
    assert_eq!(StatusCode::NOT_FOUND, unknown.status());

    let history = surface
        .send(Request::get("/operator/registers/audit-categories/history").body(Body::empty())?)
        .await?;
    let versions: serde_json::Value = serde_json::from_slice(&bytes(history).await?)?;
    assert_eq!(1, versions.as_array().map_or(0, Vec::len), "{versions}");
    Ok(())
}

#[tokio::test]
async fn a_write_on_one_replica_is_in_force_on_another_after_the_notification()
-> Result<(), Box<dyn Error>> {
    let Some((postgres, store)) = crate::store::store().await? else {
        return Ok(());
    };
    let other = crate::store::open(postgres.url()).await?;
    let writer = Registers::new(&store);
    let reader = Arc::new(Registers::new(&other));
    let listened = reader.listened();
    other.listeners(&[Arc::clone(&listened)]).spawn();
    assert!(
        listened.listening_within(WAIT).await,
        "the listener connects"
    );
    assert_eq!(0, reader.current(Register::AuditCategories).await?.number);
    let seen = listened.notifications();
    let Written::Committed(version) = writer
        .write(
            Register::AuditCategories,
            0,
            &categories(),
            "synthetic-client-sub",
        )
        .await?
    else {
        return Err("the first write commits".into());
    };
    assert!(
        listened.notified_after(seen, WAIT).await,
        "the notification arrives"
    );
    let current = reader.current(Register::AuditCategories).await?;
    assert_eq!(version.number, current.number);
    assert_eq!(categories(), current.document);
    Ok(())
}

#[tokio::test]
async fn a_replica_whose_listener_is_down_reads_the_register_on_every_request()
-> Result<(), Box<dyn Error>> {
    let Some((postgres, store)) = crate::store::store().await? else {
        return Ok(());
    };
    let other = crate::store::open(postgres.url()).await?;
    let writer = Registers::new(&store);
    let reader = Registers::new(&other);
    assert!(!reader.listened().is_listening());
    assert_eq!(0, reader.current(Register::AuditCategories).await?.number);
    writer
        .write(
            Register::AuditCategories,
            0,
            &categories(),
            "synthetic-client-sub",
        )
        .await?;
    assert_eq!(
        1,
        reader.current(Register::AuditCategories).await?.number,
        "no notification is needed while the listener is down"
    );
    Ok(())
}
