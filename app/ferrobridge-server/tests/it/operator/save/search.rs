// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Saving a search declaration through the operator API: checked against
//! the context it names, written under the console tree and swapped in with
//! the facade's programs. No specification governs the declaration file or
//! the operator surface: our own design.

use std::error::Error;

use axum::body::Body;
use http::{Request, StatusCode, header};
use tower::ServiceExt as _;
use wiremock::{Mock, ResponseTemplate, matchers};

use super::{CONTEXT, Case, MODEL, REASON};
use crate::facade::auth::{bearing, mint};
use crate::operator::bytes;

/// A declaration of the `code` token of the console's `Observation` context.
const SEARCH: &str = r"context: ferrobridge_console.context
parameters:
  code:
    type: token
    node: { class: EVALUATION, archetype: openEHR-EHR-EVALUATION.problem_diagnosis.v1 }
    path: data[at0001]/items[at0002]/value
";

/// A declaration naming a context no loaded mapping compiles.
const UNBOUND: &str = r"context: ferrobridge_absent.context
parameters:
  code:
    type: token
    node: { class: EVALUATION, archetype: openEHR-EHR-EVALUATION.problem_diagnosis.v1 }
    path: data[at0001]/items[at0002]/value
";

impl Case {
    /// Saves the console's model and context, so `Observation` is mapped.
    async fn map_observation(&self) -> Result<(), Box<dyn Error>> {
        for (relative, body) in [
            ("ferrobridge_console.yml", MODEL),
            ("ferrobridge_console.context.yml", CONTEXT),
        ] {
            let status = self.create(relative, body).await?.status();
            assert_eq!(StatusCode::OK, status, "{relative} saves");
        }
        Ok(())
    }

    /// Returns the search parameter names the `CapabilityStatement` gives
    /// `Observation`.
    async fn observation_parameters(&self) -> Result<Vec<String>, Box<dyn Error>> {
        let response = self
            .app
            .clone()
            .oneshot(Request::get("/fhir/metadata").body(Body::empty())?)
            .await?;
        let statement: serde_json::Value = serde_json::from_slice(&bytes(response).await?)?;
        Ok(statement["rest"][0]["resource"]
            .as_array()
            .ok_or("a resource array")?
            .iter()
            .filter(|entry| entry["type"] == "Observation")
            .flat_map(|entry| entry["searchParam"].as_array().cloned().unwrap_or_default())
            .filter_map(|parameter| parameter["name"].as_str().map(str::to_owned))
            .collect())
    }

    /// Searches `Observation` by `code` under a system token.
    async fn search_observation(&self) -> Result<http::Response<Body>, Box<dyn Error>> {
        let token = mint(&self.key, &self.issuer.claims("system/Observation.rs"));
        let request = Request::get("/fhir/Observation?code=http://snomed.info/sct%7C2024001")
            .body(Body::empty())?;
        Ok(self.app.clone().oneshot(bearing(request, &token)).await?)
    }
}

#[tokio::test]
async fn a_console_declaration_checks_against_its_context_and_serves_search()
-> Result<(), Box<dyn Error>> {
    let case = Case::new().await?;
    Mock::given(matchers::method("POST"))
        .and(matchers::path("/query/aql"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "q": "synthetic",
            "columns": [
                { "name": "ehr_id", "path": "/ehr_id/value" },
                { "name": "version_uid", "path": "/uid/value" },
                { "name": "composition", "path": "" }
            ],
            "rows": []
        })))
        .mount(&case.cdr)
        .await;
    case.map_observation().await?;
    assert_eq!(case.observation_parameters().await?, Vec::<String>::new());
    let before = case.search_observation().await?.status();
    assert_ne!(StatusCode::OK, before, "Observation declares no search yet");

    let response = case
        .create("ferrobridge_console.search.yml", SEARCH)
        .await?;
    assert_eq!(StatusCode::OK, response.status());
    let report: serde_json::Value = serde_json::from_slice(&bytes(response).await?)?;
    assert_eq!(serde_json::Value::Bool(true), report["clean"], "{report}");
    assert_eq!(
        SEARCH.as_bytes(),
        std::fs::read(case.console("ferrobridge_console.search.yml"))?.as_slice()
    );
    let state = case.state().await?;
    let context = state["contexts"]
        .as_array()
        .ok_or("a contexts array")?
        .iter()
        .find(|context| context["context"] == "ferrobridge_console.context")
        .ok_or("the console context is compiled")?;
    assert_eq!(
        "fhirconnect/ferrobridge_console.search.yml",
        context["search"]
    );
    let parameters = case.observation_parameters().await?;
    assert!(parameters.contains(&String::from("code")), "{parameters:?}");
    let response = case.search_observation().await?;
    assert_eq!(StatusCode::OK, response.status());
    let aql = case
        .cdr
        .received_requests()
        .await
        .unwrap_or_default()
        .into_iter()
        .filter(|request| request.url.path() == "/query/aql")
        .count();
    assert!(aql > 0, "the search reaches the CDR");
    assert!(case.staging_left().is_empty(), "{:?}", case.staging_left());
    Ok(())
}

#[tokio::test]
async fn a_declaration_that_does_not_bind_is_refused_and_changes_nothing()
-> Result<(), Box<dyn Error>> {
    let case = Case::new().await?;
    case.map_observation().await?;
    let response = case
        .create("ferrobridge_console.search.yml", UNBOUND)
        .await?;
    assert_eq!(StatusCode::UNPROCESSABLE_ENTITY, response.status());
    let report: serde_json::Value = serde_json::from_slice(&bytes(response).await?)?;
    assert_eq!(serde_json::Value::Bool(false), report["clean"]);
    let diagnostic = report["diagnostics"]
        .as_array()
        .ok_or("a diagnostics array")?
        .iter()
        .find(|diagnostic| diagnostic["code"] == "fb-search-declaration")
        .ok_or("the declaration is reported")?;
    assert_eq!(
        "fhirconnect/ferrobridge_console.search.yml",
        diagnostic["file"]
    );
    assert!(!case.console("ferrobridge_console.search.yml").exists());
    assert!(case.staging_left().is_empty(), "{:?}", case.staging_left());
    assert_eq!(case.observation_parameters().await?, Vec::<String>::new());
    let types = case.declared_types().await?;
    assert!(types.contains(&String::from("Observation")), "{types:?}");
    Ok(())
}

#[tokio::test]
async fn a_draft_declaration_check_answers_its_diagnostics_and_writes_nothing()
-> Result<(), Box<dyn Error>> {
    let case = Case::new().await?;
    case.map_observation().await?;
    let check = |body: &str| {
        Request::post("/operator/mappings/check?path=fhirconnect/ferrobridge_console.search.yml")
            .body(Body::from(body.to_owned()))
    };
    let response = case.send(check(UNBOUND)?).await?;
    assert_eq!(StatusCode::OK, response.status());
    let report: serde_json::Value = serde_json::from_slice(&bytes(response).await?)?;
    assert_eq!(serde_json::Value::Bool(false), report["clean"], "{report}");
    let response = case.send(check(SEARCH)?).await?;
    let report: serde_json::Value = serde_json::from_slice(&bytes(response).await?)?;
    assert_eq!(serde_json::Value::Bool(true), report["clean"], "{report}");
    assert!(!case.console("ferrobridge_console.search.yml").exists());
    assert!(case.staging_left().is_empty(), "{:?}", case.staging_left());
    Ok(())
}

#[tokio::test]
async fn a_save_over_a_provisioned_declaration_is_refused() -> Result<(), Box<dyn Error>> {
    let case = Case::new().await?;
    let response = case
        .send(
            Request::put("/operator/mappings/fhirconnect/ferrobridge_facade.search.yml")
                .header(header::IF_MATCH, "*")
                .header("ferrobridge-reason", REASON)
                .body(Body::from(SEARCH))?,
        )
        .await?;
    assert_eq!(StatusCode::CONFLICT, response.status());
    assert!(!case.console("ferrobridge_facade.search.yml").exists());
    Ok(())
}
