// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! SMART App State under the guard (SMART App Launch 2.2.0, Persisting App
//! State, §Security and Access controls and §Discovery): the `Basic`
//! permission decided against the state, a granular `?code=` scope for one
//! state code, a `patient/` grant held to its launch patient, and the
//! discovery document listing `smart-app-state` only when the endpoint is on.

use std::error::Error;

use axum::body::Body;
use http::Request;
use http::StatusCode;
use jsonwebtoken::Algorithm;

use super::Issuer;
use super::SigningKey;
use super::authority;
use super::bearing;
use super::discovery::read_open;
use super::ec_key;
use super::mint;
use super::secured;
use crate::facade::app_state::Case;
use crate::facade::app_state::at_base;
use crate::facade::app_state::get;
use crate::facade::app_state::keys;
use crate::facade::app_state::matching;
use crate::facade::app_state::preferences;
use crate::facade::app_state::with_body;
use crate::facade::harness;

/// The granular scope of the page's patient-facing mobile app.
const MOBILE: &str = "patient/Basic.cuds?code=https://myapp.example.org|encrypted-phr-access-keys";

/// One guarded case.
struct Guarded {
    /// The issuer.
    issuer: Issuer,
    /// The key every token is signed with.
    key: SigningKey,
    /// The facade serving App State.
    case: Case,
    /// The resource server.
    authority: std::sync::Arc<ferrobridge_server::smart::Smart>,
}

impl Guarded {
    /// Returns a guarded facade serving App State.
    async fn new() -> Result<Option<Self>, Box<dyn Error>> {
        let Some(case) = Case::new().await? else {
            return Ok(None);
        };
        let key = ec_key("k1", Algorithm::ES256);
        let issuer = Issuer::start(&[&key]).await;
        let authority = authority(issuer.settings()).await;
        Ok(Some(Self {
            issuer,
            key,
            case,
            authority,
        }))
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
async fn a_granular_code_scope_reaches_its_own_state_code_only() -> Result<(), Box<dyn Error>> {
    let Some(guarded) = Guarded::new().await? else {
        return Ok(());
    };
    let token = guarded.token(MOBILE, Some("123"));
    let (status, created) = guarded
        .send(with_body("POST", "/fhir/Basic", &keys("a")), &token)
        .await?;
    assert_eq!(StatusCode::CREATED, status, "{created}");
    let mut preferences_of_the_patient = preferences();
    preferences_of_the_patient["subject"]["reference"] = serde_json::json!(at_base("Patient/123"));
    let (status, _) = guarded
        .send(
            with_body("POST", "/fhir/Basic", &preferences_of_the_patient),
            &token,
        )
        .await?;
    assert_eq!(StatusCode::FORBIDDEN, status);
    let (status, bundle) = guarded
        .send(
            get(&format!(
                "/fhir/Basic?code=https://myapp.example.org%7Cencrypted-phr-access-keys&subject={}",
                at_base("Patient/123")
            )),
            &token,
        )
        .await?;
    assert_eq!(StatusCode::OK, status, "{bundle}");
    assert_eq!(Some(1), bundle["total"].as_u64());
    Ok(())
}

#[tokio::test]
async fn a_patient_grant_reaches_its_launch_patient_and_no_global_state()
-> Result<(), Box<dyn Error>> {
    let Some(guarded) = Guarded::new().await? else {
        return Ok(());
    };
    let token = guarded.token("patient/Basic.cuds", Some("123"));
    let mut other = keys("a");
    other["subject"]["reference"] = serde_json::json!(at_base("Patient/456"));
    let (status, _) = guarded
        .send(with_body("POST", "/fhir/Basic", &other), &token)
        .await?;
    assert_eq!(StatusCode::FORBIDDEN, status);
    let mut global = keys("a");
    global.as_object_mut().ok_or("an object")?.remove("subject");
    let (status, _) = guarded
        .send(with_body("POST", "/fhir/Basic", &global), &token)
        .await?;
    assert_eq!(StatusCode::FORBIDDEN, status);
    let (status, _) = guarded
        .send(
            get("/fhir/Basic?code=https://myapp.example.org%7Cencrypted-phr-access-keys&subject:missing=true"),
            &token,
        )
        .await?;
    assert_eq!(StatusCode::FORBIDDEN, status);
    Ok(())
}

#[tokio::test]
async fn a_search_without_a_subject_answers_only_what_the_token_reaches()
-> Result<(), Box<dyn Error>> {
    let Some(guarded) = Guarded::new().await? else {
        return Ok(());
    };
    let admin = guarded.token("user/Basic.cuds", None);
    for patient in ["123", "456"] {
        let mut state = keys("a");
        state["subject"]["reference"] = serde_json::json!(at_base(&format!("Patient/{patient}")));
        let (status, _) = guarded
            .send(with_body("POST", "/fhir/Basic", &state), &admin)
            .await?;
        assert_eq!(StatusCode::CREATED, status);
    }
    let query = "/fhir/Basic?code=https://myapp.example.org%7Cencrypted-phr-access-keys";
    let (_, every) = guarded.send(get(query), &admin).await?;
    assert_eq!(Some(2), every["total"].as_u64(), "{every}");
    let (_, own) = guarded
        .send(get(query), &guarded.token("patient/Basic.s", Some("123")))
        .await?;
    assert_eq!(Some(1), own["total"].as_u64(), "{own}");
    let (status, _) = guarded
        .send(
            get(query),
            &guarded.token("patient/Observation.rs", Some("123")),
        )
        .await?;
    assert_eq!(StatusCode::FORBIDDEN, status);
    Ok(())
}

#[tokio::test]
async fn a_delete_needs_the_delete_permission_on_the_stored_state() -> Result<(), Box<dyn Error>> {
    let Some(guarded) = Guarded::new().await? else {
        return Ok(());
    };
    let (_, created) = guarded
        .send(
            with_body("POST", "/fhir/Basic", &keys("a")),
            &guarded.token("user/Basic.c", None),
        )
        .await?;
    let id = created["id"].as_str().ok_or("an id")?.to_owned();
    let delete = || {
        matching(
            Request::delete(format!("/fhir/Basic/{id}"))
                .body(Body::empty())
                .expect("a legal request"),
            "W/\"1\"",
        )
    };
    let (status, _) = guarded
        .send(delete(), &guarded.token("user/Basic.cus", None))
        .await?;
    assert_eq!(StatusCode::FORBIDDEN, status);
    let (status, _) = guarded
        .send(delete(), &guarded.token("user/Basic.d", None))
        .await?;
    assert_eq!(StatusCode::NO_CONTENT, status);
    Ok(())
}

#[tokio::test]
async fn an_app_state_request_without_a_token_is_refused() -> Result<(), Box<dyn Error>> {
    let Some(guarded) = Guarded::new().await? else {
        return Ok(());
    };
    let response = crate::facade::raw(
        secured(&guarded.case.harness, &guarded.authority),
        with_body("POST", "/fhir/Basic", &keys("a")),
    )
    .await?;
    assert_eq!(StatusCode::UNAUTHORIZED, response.status());
    Ok(())
}

#[tokio::test]
async fn discovery_lists_smart_app_state_only_when_the_endpoint_is_on() -> Result<(), Box<dyn Error>>
{
    let Some(guarded) = Guarded::new().await? else {
        return Ok(());
    };
    let path = "/fhir/.well-known/smart-configuration";
    let (_, _, on) = read_open(secured(&guarded.case.harness, &guarded.authority), path).await?;
    let listed = |document: &serde_json::Value| {
        document["capabilities"]
            .as_array()
            .is_some_and(|all| all.contains(&serde_json::json!("smart-app-state")))
    };
    assert!(listed(&on), "{on}");
    let plain = harness().await;
    let (_, _, off) = read_open(secured(&plain, &guarded.authority), path).await?;
    assert!(!listed(&off), "{off}");
    Ok(())
}

#[tokio::test]
async fn every_app_state_route_reaches_its_handler_under_a_full_scope() -> Result<(), Box<dyn Error>>
{
    let Some(guarded) = Guarded::new().await? else {
        return Ok(());
    };
    let token = guarded.token("system/*.cruds", None);
    for (method, template, _) in ferrobridge_server::facade::guard::APP_STATE {
        let path = format!("/fhir{template}").replace("{id}", "unknown-id");
        let request = Request::builder()
            .method(method.clone())
            .uri(&path)
            .header(http::header::CONTENT_TYPE, "application/fhir+json")
            .body(Body::from("{}"))?;
        let (status, body) = guarded.send(request, &token).await?;
        assert_ne!(StatusCode::METHOD_NOT_ALLOWED, status, "{method} {path}");
        assert_ne!(StatusCode::FORBIDDEN, status, "{method} {path}");
        assert_eq!(
            Some("OperationOutcome"),
            body["resourceType"].as_str(),
            "{method} {path} answers from the App State endpoint: {body}"
        );
    }
    Ok(())
}
