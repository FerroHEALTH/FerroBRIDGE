// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! SMART launch `Task`s under the guard: the `Task` permission of each
//! interaction, and a `patient/` grant held to the patient its launch context
//! names (SMART App Launch 2.2.0, Scopes and Launch Context).

use std::error::Error;
use std::sync::Arc;

use axum::body::Body;
use ferrobridge_server::smart::Smart;
use http::Request;
use http::StatusCode;
use jsonwebtoken::Algorithm;

use super::Issuer;
use super::SigningKey;
use super::authority;
use super::bearing;
use super::ec_key;
use super::mint;
use super::secured;
use crate::facade::launch_task::Case;
use crate::facade::launch_task::EHR_LAUNCH;
use crate::facade::launch_task::get;
use crate::facade::launch_task::post_task;

/// One guarded case.
struct Guarded {
    /// The issuer.
    issuer: Issuer,
    /// The key every token is signed with.
    key: SigningKey,
    /// The facade serving launch `Task`s.
    case: Case,
    /// The resource server.
    authority: Arc<Smart>,
}

impl Guarded {
    /// Returns a guarded facade serving launch `Task`s.
    async fn new() -> Result<Self, Box<dyn Error>> {
        let key = ec_key("k1", Algorithm::ES256);
        let issuer = Issuer::start(&[&key]).await;
        let authority = authority(issuer.settings()).await;
        Ok(Self {
            issuer,
            key,
            case: Case::new().await?,
            authority,
        })
    }

    /// Returns a token granting `scope`, launched for `patient` when given.
    fn token(&self, scope: &str, patient: Option<&str>) -> String {
        let mut claims = self.issuer.claims(scope);
        if let Some(patient) = patient {
            claims["patient"] = serde_json::json!(patient);
        }
        mint(&self.key, &claims)
    }

    /// Sends `request` under `token` and returns the status and the body.
    async fn send(
        &self,
        request: Request<Body>,
        token: &str,
    ) -> Result<(StatusCode, serde_json::Value), Box<dyn Error>> {
        crate::facade::call(
            secured(&self.case.harness, &self.authority),
            bearing(request, token),
        )
        .await
    }
}

#[tokio::test]
async fn a_patient_grant_reaches_the_tasks_of_its_launch_patient_only() -> Result<(), Box<dyn Error>>
{
    let guarded = Guarded::new().await?;
    let (status, created) = guarded
        .send(
            post_task(EHR_LAUNCH),
            &guarded.token("patient/Task.c", Some("123")),
        )
        .await?;
    assert_eq!(StatusCode::CREATED, status, "{created}");
    let id = created["id"].as_str().ok_or("an id")?.to_owned();
    let (status, _) = guarded
        .send(
            post_task(EHR_LAUNCH),
            &guarded.token("patient/Task.c", Some("456")),
        )
        .await?;
    assert_eq!(StatusCode::FORBIDDEN, status);
    let path = format!("/fhir/Task/{id}");
    let (status, _) = guarded
        .send(get(&path), &guarded.token("patient/Task.r", Some("123")))
        .await?;
    assert_eq!(StatusCode::OK, status);
    let (status, _) = guarded
        .send(get(&path), &guarded.token("patient/Task.r", Some("456")))
        .await?;
    assert_eq!(StatusCode::FORBIDDEN, status);
    let (_, own) = guarded
        .send(
            get("/fhir/Task"),
            &guarded.token("patient/Task.s", Some("123")),
        )
        .await?;
    assert_eq!(Some(1), own["total"].as_u64(), "{own}");
    let (_, other) = guarded
        .send(
            get("/fhir/Task"),
            &guarded.token("patient/Task.s", Some("456")),
        )
        .await?;
    assert_eq!(Some(0), other["total"].as_u64(), "{other}");
    Ok(())
}

#[tokio::test]
async fn each_interaction_needs_its_task_permission() -> Result<(), Box<dyn Error>> {
    let guarded = Guarded::new().await?;
    let (status, _) = guarded
        .send(post_task(EHR_LAUNCH), &guarded.token("user/Task.rs", None))
        .await?;
    assert_eq!(StatusCode::FORBIDDEN, status);
    let (status, _) = guarded
        .send(get("/fhir/Task"), &guarded.token("user/Task.c", None))
        .await?;
    assert_eq!(StatusCode::FORBIDDEN, status);
    Ok(())
}

#[tokio::test]
async fn every_launch_task_route_reaches_its_handler_under_a_full_scope()
-> Result<(), Box<dyn Error>> {
    let guarded = Guarded::new().await?;
    let token = guarded.token("system/*.cruds", None);
    for (method, template, _) in ferrobridge_server::facade::guard::LAUNCH_TASK {
        let path = format!("/fhir{template}").replace("{id}", "unknown-id");
        let request = Request::builder()
            .method(method.clone())
            .uri(&path)
            .header(http::header::CONTENT_TYPE, "application/fhir+json")
            .body(Body::from("{}"))?;
        let (status, body) = guarded.send(request, &token).await?;
        assert_ne!(StatusCode::METHOD_NOT_ALLOWED, status, "{method} {path}");
        assert_ne!(StatusCode::FORBIDDEN, status, "{method} {path}");
        assert!(
            body["resourceType"] == serde_json::json!("OperationOutcome")
                || body["resourceType"] == serde_json::json!("Bundle"),
            "{method} {path} answers from the launch endpoint: {body}"
        );
    }
    Ok(())
}
