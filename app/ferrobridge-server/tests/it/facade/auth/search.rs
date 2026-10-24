// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Search under a token: the `s` permission admits it, and a `patient/`
//! token's search reaches only its patient's EHR
//! (<https://hl7.org/fhir/smart-app-launch/scopes-and-launch-context.html>).

use std::error::Error;

use ferrobridge_server::cdr::ids::EhrId;
use ferrobridge_server::facade::identity::PersonId;
use ferrobridge_server::facade::identity::store::Store;
use ferrobridge_server::facade::search::Limits;
use http::StatusCode;
use jsonwebtoken::Algorithm;
use wiremock::Mock;
use wiremock::ResponseTemplate;
use wiremock::matchers;

use super::Issuer;
use super::authority;
use super::bearing;
use super::ec_key;
use super::get;
use super::mint;
use super::secured;
use crate::facade::EHR_ID;
use crate::facade::raw;

/// Runs `path` under a token granting `scope`, launched for `patient`, over
/// a facade whose subject `Patient/p-1` is bound to [`EHR_ID`], and returns
/// the status, the body and the AQL requests the CDR received.
async fn search_as(
    scope: &str,
    patient: Option<&str>,
    path: &str,
) -> Result<(StatusCode, serde_json::Value, Vec<serde_json::Value>), Box<dyn Error>> {
    let key = ec_key("k1", Algorithm::ES256);
    let issuer = Issuer::start(&[&key]).await;
    let harness = crate::facade::search_harness(Limits::default()).await;
    harness.store.record_ehr(
        &PersonId::new("http://example.org/fhir", "Patient/p-1")?,
        &EhrId::new(EHR_ID)?,
    )?;
    Mock::given(matchers::method("POST"))
        .and(matchers::path("/query/aql"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "q": "synthetic",
            "rows": []
        })))
        .mount(&harness.cdr)
        .await;
    let authority = authority(issuer.settings()).await;
    let mut claims = issuer.claims(scope);
    if let Some(patient) = patient {
        claims["patient"] = serde_json::json!(patient);
    }
    let token = mint(&key, &claims);
    let response = raw(secured(&harness, &authority), bearing(get(path), &token)).await?;
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), 1024 * 1024).await?;
    let body = serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null);
    let queries = harness
        .cdr
        .received_requests()
        .await
        .unwrap_or_default()
        .iter()
        .filter(|request| request.url.path() == "/query/aql")
        .filter_map(|request| serde_json::from_slice(&request.body).ok())
        .collect();
    Ok((status, body, queries))
}

#[tokio::test]
async fn a_token_without_the_search_permission_is_forbidden() -> Result<(), Box<dyn Error>> {
    let (status, _, queries) = search_as("system/Condition.r", None, "/fhir/Condition").await?;
    assert_eq!(StatusCode::FORBIDDEN, status);
    assert!(queries.is_empty(), "a forbidden search reached the CDR");
    Ok(())
}

#[tokio::test]
async fn a_system_search_token_reaches_every_ehr() -> Result<(), Box<dyn Error>> {
    let (status, body, queries) = search_as("system/Condition.rs", None, "/fhir/Condition").await?;
    assert_eq!(StatusCode::OK, status, "{body}");
    let query = queries.first().ok_or("the search reached the CDR")?;
    assert!(
        !query["q"]
            .as_str()
            .unwrap_or_default()
            .contains("e/ehr_id/value ="),
        "a system token's search is held to one EHR: {query}"
    );
    Ok(())
}

#[tokio::test]
async fn a_patient_token_searches_its_own_patients_ehr_only() -> Result<(), Box<dyn Error>> {
    let (status, body, queries) =
        search_as("patient/Condition.rs", Some("p-1"), "/fhir/Condition").await?;
    assert_eq!(StatusCode::OK, status, "{body}");
    let query = queries.first().ok_or("the search reached the CDR")?;
    assert!(
        query["q"]
            .as_str()
            .unwrap_or_default()
            .contains("e/ehr_id/value = $"),
        "{query}"
    );
    assert!(
        query["query_parameters"]
            .as_object()
            .is_some_and(|bound| bound.values().any(|value| value == EHR_ID)),
        "{query}"
    );

    // Another patient's EHR is outside the compartment even when named.
    let (status, body, queries) = search_as(
        "patient/Condition.rs",
        Some("p-2"),
        "/fhir/Condition?patient=p-1",
    )
    .await?;
    assert_eq!(StatusCode::OK, status, "{body}");
    assert_eq!(Some(0), body["total"].as_u64(), "{body}");
    assert!(
        queries.is_empty(),
        "a patient the map binds no EHR to reached the CDR"
    );
    Ok(())
}
