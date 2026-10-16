// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! A `patient/` token reaches only the compartment of the patient in its
//! `patient` launch context
//! (<https://hl7.org/fhir/smart-app-launch/scopes-and-launch-context.html>),
//! and a transaction is authorised entry by entry and fails whole
//! (<https://hl7.org/fhir/R4/http.html#transaction>).

use std::error::Error as StdError;
use std::sync::Arc;

use axum::body::Body;
use ferrobridge_server::auth::Authority;
use http::Request;
use http::StatusCode;
use http::header;
use jsonwebtoken::Algorithm;

use super::Issuer;
use super::SigningKey;
use super::authority;
use super::bearing;
use super::condition_of;
use super::ec_key;
use super::get;
use super::mint;
use super::secured;
use crate::facade::EHR_ID;
use crate::facade::Harness;
use crate::facade::VERSION_ONE;
use crate::facade::condition;
use crate::facade::harness;
use crate::facade::post_condition;
use crate::facade::raw;
use crate::facade::stub::mount_create;
use crate::facade::stub::mount_ehr;
use crate::facade::stub::mount_read;
use crate::facade::transaction_of;
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
    authority: Arc<Authority>,
}

impl Case {
    /// Returns a case whose CDR answers the EHR lookup.
    async fn new() -> Self {
        let key = ec_key("k1", Algorithm::ES256);
        let issuer = Issuer::start(&[&key]).await;
        let harness = harness().await;
        mount_ehr(&harness.cdr).await;
        let authority = authority(issuer.settings()).await;
        Self {
            issuer,
            key,
            harness,
            authority,
        }
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
    ) -> Result<(StatusCode, serde_json::Value), Box<dyn StdError>> {
        let response = raw(
            secured(&self.harness, &self.authority),
            bearing(request, token),
        )
        .await?;
        let status = response.status();
        let bytes = axum::body::to_bytes(response.into_body(), 1024 * 1024).await?;
        let body = if bytes.is_empty() {
            serde_json::Value::Null
        } else {
            serde_json::from_slice(&bytes)?
        };
        Ok((status, body))
    }

    /// Creates the Condition of `patient` under a system token and returns
    /// its id.
    async fn create_for(&self, patient: &str) -> Result<String, Box<dyn StdError>> {
        let resource = condition_of(patient);
        mount_create(
            &self.harness.cdr,
            VERSION_ONE,
            &self.harness.composition_of(&resource),
        )
        .await;
        let (status, body) = self
            .send(
                post_condition(&resource),
                &self.token("system/Condition.c", None),
            )
            .await?;
        assert_eq!(StatusCode::CREATED, status, "{body}");
        Ok(body["id"].as_str().ok_or("an id")?.to_owned())
    }
}

/// Returns a transaction Bundle posting `resources`.
fn transaction(resources: &[(&str, serde_json::Value)]) -> Request<Body> {
    Request::post("/fhir")
        .header(header::CONTENT_TYPE, "application/fhir+json")
        .body(Body::from(transaction_of(resources).to_string()))
        .expect("a legal request")
}

#[tokio::test]
async fn a_patient_token_creates_for_its_own_patient_only() -> Result<(), Box<dyn StdError>> {
    let case = Case::new().await;
    let own = condition_of("p-1");
    mount_create(
        &case.harness.cdr,
        VERSION_ONE,
        &case.harness.composition_of(&own),
    )
    .await;
    let token = case.token("patient/Condition.c", Some("p-1"));
    let (status, body) = case
        .send(post_condition(&condition_of("p-2")), &token)
        .await?;
    assert_eq!(StatusCode::FORBIDDEN, status, "{body}");
    assert_eq!((0, 0, 0), writes(&case.harness).await, "nothing committed");
    let (status, body) = case.send(post_condition(&condition()), &token).await?;
    assert_eq!(
        StatusCode::FORBIDDEN,
        status,
        "a subject by identifier is not shown to be the patient: {body}"
    );
    let (status, body) = case.send(post_condition(&own), &token).await?;
    assert_eq!(StatusCode::CREATED, status, "{body}");
    Ok(())
}

#[tokio::test]
async fn a_patient_scope_without_a_patient_claim_reaches_nothing() -> Result<(), Box<dyn StdError>>
{
    let case = Case::new().await;
    let token = case.token("patient/Condition.c", None);
    let (status, body) = case
        .send(post_condition(&condition_of("p-1")), &token)
        .await?;
    assert_eq!(StatusCode::FORBIDDEN, status, "{body}");
    Ok(())
}

#[tokio::test]
async fn a_patient_token_reads_its_own_patient_only() -> Result<(), Box<dyn StdError>> {
    let case = Case::new().await;
    let id = case.create_for("p-1").await?;
    mount_read(
        &case.harness.cdr,
        VERSION_ONE,
        &case.harness.composition_of(&condition_of("p-1")),
    )
    .await;
    let path = format!("/fhir/Condition/{id}");
    let other = case.token("patient/Condition.rs", Some("p-2"));
    let (status, body) = case.send(get(&path), &other).await?;
    assert_eq!(StatusCode::FORBIDDEN, status, "{body}");
    assert_eq!(
        0,
        case.harness
            .received("GET", &format!("/ehr/{EHR_ID}/composition"))
            .await,
        "the refused read reached no composition"
    );
    let own = case.token("patient/Condition.rs", Some("p-1"));
    let (status, body) = case.send(get(&path), &own).await?;
    assert_eq!(StatusCode::OK, status, "{body}");
    let (status, body) = case
        .send(get(&format!("{path}/_history/1")), &other)
        .await?;
    assert_eq!(StatusCode::FORBIDDEN, status, "{body}");
    Ok(())
}

#[tokio::test]
async fn a_transaction_with_one_entry_outside_the_compartment_fails_whole()
-> Result<(), Box<dyn StdError>> {
    let case = Case::new().await;
    let token = case.token("patient/Condition.c", Some("p-1"));
    let request = transaction(&[
        ("urn:uuid:entry-own", condition_of("p-1")),
        ("urn:uuid:entry-other", condition_of("p-2")),
    ]);
    let (status, body) = case.send(request, &token).await?;
    assert_eq!(StatusCode::FORBIDDEN, status, "{body}");
    assert!(
        !body.to_string().contains("urn:uuid:entry-other"),
        "the refusal names no entry: {body}"
    );
    assert_eq!((0, 0, 0), writes(&case.harness).await, "nothing committed");
    Ok(())
}

#[tokio::test]
async fn a_transaction_entry_of_a_type_the_token_does_not_cover_fails_whole()
-> Result<(), Box<dyn StdError>> {
    let case = Case::new().await;
    let token = case.token("system/Observation.c", None);
    let request = transaction(&[("urn:uuid:entry-1", condition())]);
    let (status, body) = case.send(request, &token).await?;
    assert_eq!(StatusCode::FORBIDDEN, status, "{body}");
    assert_eq!((0, 0, 0), writes(&case.harness).await, "nothing committed");
    Ok(())
}

#[tokio::test]
async fn a_covered_transaction_passes_to_the_mapping() -> Result<(), Box<dyn StdError>> {
    let case = Case::new().await;
    let token = case.token("system/Condition.c", None);
    let mut unmapped = condition();
    unmapped["meta"] = serde_json::json!({ "profile": ["http://example.org/other"] });
    let request = transaction(&[("urn:uuid:entry-1", unmapped)]);
    let (status, body) = case.send(request, &token).await?;
    assert_eq!(
        StatusCode::UNPROCESSABLE_ENTITY,
        status,
        "the guard admitted it and the mapping refused it: {body}"
    );
    Ok(())
}
