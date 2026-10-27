// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Saving FHIRconnect files in a deployment that provisions no FHIRconnect
//! tree: the console's files are the whole set, checked and compiled on
//! every save. No specification governs the operator surface: our own
//! design.

use std::collections::BTreeMap;
use std::error::Error;
use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use ferrobridge_server::check::Report;
use ferrobridge_server::operator::Operator;
use ferrobridge_server::operator::reload::Reloader;
use ferrobridge_server::operator::snapshot::Snapshot;
use ferrobridge_server::state::AppState;
use ferrohealth_health::registry::Registry;
use http::{Request, StatusCode, header};
use jsonwebtoken::Algorithm;
use tower::ServiceExt as _;
use wiremock::{Mock, MockServer, ResponseTemplate, matchers};

use super::{BROKEN, CONTEXT, MODEL, REASON, server_settings};
use crate::facade::auth::{Issuer, SigningKey, authority, bearing, ec_key, mint};
use crate::operator::{OPERATOR, bytes, digest};

/// A console-only deployment: no provisioned tree, templates from the CDR.
struct Unprovisioned {
    /// The console tree, kept alive for the case.
    tree: tempfile::TempDir,
    /// The stub CDR, kept alive for the case.
    _cdr: MockServer,
    /// The stub authorization server.
    issuer: Issuer,
    /// The signing key.
    key: SigningKey,
    /// The router under test.
    app: Router,
}

impl Unprovisioned {
    /// Builds the operator API over an empty console directory.
    async fn new() -> Result<Self, Box<dyn Error>> {
        let tree = tempfile::tempdir()?;
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
        let reloader = Reloader {
            cdr: Some(crate::facade::client(&cdr)),
            ..Reloader::default()
        };
        let operator = Operator::new(
            Vec::new(),
            Snapshot::new(Report::default(), BTreeMap::new()),
        )
        .with_saving(tree.path().join("console"), reloader);
        let key = ec_key("k1", Algorithm::ES256);
        let issuer = Issuer::start(&[&key]).await;
        let state = AppState::with_health(Registry::default())
            .with_auth(authority(issuer.settings()).await)
            .with_operator(Arc::new(operator));
        let app = ferrobridge_server::router(Arc::new(state), &server_settings());
        Ok(Self {
            tree,
            _cdr: cdr,
            issuer,
            key,
            app,
        })
    }

    /// Sends `request` under an operator token.
    async fn send(&self, request: Request<Body>) -> Result<http::Response<Body>, Box<dyn Error>> {
        let token = mint(&self.key, &self.issuer.claims(OPERATOR));
        Ok(self.app.clone().oneshot(bearing(request, &token)).await?)
    }

    /// Saves `body` at the console path `relative`, `precondition` naming
    /// the file's state.
    async fn save(
        &self,
        relative: &str,
        precondition: (header::HeaderName, String),
        body: &str,
    ) -> Result<http::Response<Body>, Box<dyn Error>> {
        self.send(
            Request::put(format!("/operator/mappings/fhirconnect/{relative}"))
                .header(precondition.0, precondition.1)
                .header("ferrobridge-reason", REASON)
                .body(Body::from(body.to_owned()))?,
        )
        .await
    }
}

#[tokio::test]
async fn console_files_are_compiled_and_a_broken_draft_is_refused_without_a_provisioned_tree()
-> Result<(), Box<dyn Error>> {
    let case = Unprovisioned::new().await?;
    let created = (header::IF_NONE_MATCH, String::from("*"));
    for (relative, body) in [
        ("ferrobridge_console.yml", MODEL),
        ("ferrobridge_console.context.yml", CONTEXT),
    ] {
        let response = case.save(relative, created.clone(), body).await?;
        assert_eq!(StatusCode::OK, response.status(), "{relative}");
    }
    let response = case
        .send(Request::get("/operator/state").body(Body::empty())?)
        .await?;
    let state: serde_json::Value = serde_json::from_slice(&bytes(response).await?)?;
    assert!(
        state["contexts"]
            .as_array()
            .ok_or("a contexts array")?
            .iter()
            .any(|context| context["context"] == "ferrobridge_console.context"),
        "{state}"
    );

    let current = (
        header::IF_MATCH,
        format!("\"{}\"", digest(MODEL.as_bytes())),
    );
    let response = case
        .save("ferrobridge_console.yml", current, BROKEN)
        .await?;
    assert_eq!(StatusCode::UNPROCESSABLE_ENTITY, response.status());
    let report: serde_json::Value = serde_json::from_slice(&bytes(response).await?)?;
    assert!(
        report["diagnostics"]
            .as_array()
            .ok_or("a diagnostics array")?
            .iter()
            .any(
                |diagnostic| diagnostic["file"] == "fhirconnect/ferrobridge_console.yml"
                    && diagnostic["code"] == "fc-unknown-template-node"
            ),
        "{report}"
    );
    assert_eq!(
        MODEL.as_bytes(),
        std::fs::read(
            case.tree
                .path()
                .join("console/fhirconnect/ferrobridge_console.yml")
        )?
        .as_slice(),
        "the saved file keeps its bytes"
    );
    Ok(())
}
