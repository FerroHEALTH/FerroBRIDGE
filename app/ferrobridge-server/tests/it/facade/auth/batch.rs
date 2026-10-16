// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! A batch is authorised entry by entry, and an entry the token does not
//! cover answers `403` inside the `batch-response` while the others proceed
//! (<https://hl7.org/fhir/R4/http.html#transaction>, §3.1.0.11: each entry is
//! treated as an individual interaction).

use std::error::Error as StdError;

use axum::body::Body;
use http::Request;
use http::StatusCode;
use http::header;
use jsonwebtoken::Algorithm;

use super::Issuer;
use super::authority;
use super::bearing;
use super::condition_of;
use super::ec_key;
use super::mint;
use super::secured;
use crate::facade::VERSION_ONE;
use crate::facade::batch::batch_of;
use crate::facade::batch::entry;
use crate::facade::batch::mount_create_once;
use crate::facade::batch::statuses;
use crate::facade::first_issue;
use crate::facade::harness;
use crate::facade::post_condition;
use crate::facade::raw;
use crate::facade::stub::mount_create;
use crate::facade::stub::mount_ehr;
use crate::facade::stub::mount_read;
use crate::facade::writes;

/// Sends `bundle` to `app` under `token` and returns the status and the body.
async fn send(
    app: axum::Router,
    bundle: &serde_json::Value,
    token: &str,
) -> Result<(StatusCode, serde_json::Value), Box<dyn StdError>> {
    let request = Request::post("/fhir")
        .header(header::CONTENT_TYPE, "application/fhir+json")
        .body(Body::from(bundle.to_string()))?;
    let response = raw(app, bearing(request, token)).await?;
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), 1024 * 1024).await?;
    Ok((status, serde_json::from_slice(&bytes)?))
}

#[tokio::test]
async fn an_entry_outside_the_token_scope_is_refused_inside_and_the_others_proceed()
-> Result<(), Box<dyn StdError>> {
    let key = ec_key("k1", Algorithm::ES256);
    let issuer = Issuer::start(&[&key]).await;
    let harness = harness().await;
    let authority = authority(issuer.settings()).await;
    mount_ehr(&harness.cdr).await;
    let resource = condition_of("p-1");
    mount_create_once(
        &harness.cdr,
        VERSION_ONE,
        &harness.composition_of(&resource),
    )
    .await;
    let writer = mint(&key, &issuer.claims("system/Condition.c"));
    let response = raw(
        secured(&harness, &authority),
        bearing(post_condition(&resource), &writer),
    )
    .await?;
    assert_eq!(StatusCode::CREATED, response.status());
    let bytes = axum::body::to_bytes(response.into_body(), 1024 * 1024).await?;
    let created: serde_json::Value = serde_json::from_slice(&bytes)?;
    let id = created["id"].as_str().ok_or("an id")?.to_owned();
    mount_read(
        &harness.cdr,
        VERSION_ONE,
        &harness.composition_of(&resource),
    )
    .await;

    let reader = mint(&key, &issuer.claims("system/Condition.r"));
    let (status, body) = send(
        secured(&harness, &authority),
        &batch_of(&[
            entry("POST", "Condition", Some(condition_of("p-2"))),
            entry("GET", &format!("Condition/{id}"), None),
            entry("PUT", &format!("Condition/{id}"), Some(resource)),
        ]),
        &reader,
    )
    .await?;
    assert_eq!(StatusCode::OK, status, "{body}");
    assert_eq!(
        vec!["403 Forbidden", "200 OK", "403 Forbidden"],
        statuses(&body),
        "{body}"
    );
    assert_eq!(
        Some("forbidden"),
        first_issue(&body["entry"][0]["response"]["outcome"])["code"].as_str()
    );
    assert_eq!(
        Some("Condition"),
        body["entry"][1]["resource"]["resourceType"].as_str()
    );
    assert_eq!(
        (1, 0, 0),
        writes(&harness).await,
        "the refused entries write nothing"
    );
    Ok(())
}

#[tokio::test]
async fn an_entry_outside_the_patient_compartment_is_refused_inside()
-> Result<(), Box<dyn StdError>> {
    // SMART App Launch 2.x scopes-and-launch-context: a `patient/` grant
    // reaches the compartment of the patient in the launch context only.
    let key = ec_key("k1", Algorithm::ES256);
    let issuer = Issuer::start(&[&key]).await;
    let harness = harness().await;
    let authority = authority(issuer.settings()).await;
    mount_ehr(&harness.cdr).await;
    let own = condition_of("p-1");
    mount_create(&harness.cdr, VERSION_ONE, &harness.composition_of(&own)).await;
    let mut claims = issuer.claims("patient/Condition.c");
    claims["patient"] = serde_json::json!("p-1");
    let token = mint(&key, &claims);
    let mut other = condition_of("p-2");
    other["id"] = serde_json::json!("ferrobridge-synthetic-condition-p2");

    let (status, body) = send(
        secured(&harness, &authority),
        &batch_of(&[
            entry("POST", "Condition", Some(other)),
            entry("POST", "Condition", Some(own)),
        ]),
        &token,
    )
    .await?;
    assert_eq!(StatusCode::OK, status, "{body}");
    assert_eq!(
        vec!["403 Forbidden", "201 Created"],
        statuses(&body),
        "{body}"
    );
    assert_eq!(
        (1, 0, 0),
        writes(&harness).await,
        "only the own patient's entry commits"
    );
    Ok(())
}

#[tokio::test]
async fn a_batch_without_a_token_is_refused_whole() -> Result<(), Box<dyn StdError>> {
    let key = ec_key("k1", Algorithm::ES256);
    let issuer = Issuer::start(&[&key]).await;
    let harness = harness().await;
    let authority = authority(issuer.settings()).await;
    let request = Request::post("/fhir")
        .header(header::CONTENT_TYPE, "application/fhir+json")
        .body(Body::from(
            batch_of(&[entry("GET", "Condition/c-1", None)]).to_string(),
        ))?;
    let response = raw(secured(&harness, &authority), request).await?;
    assert_eq!(StatusCode::UNAUTHORIZED, response.status());
    Ok(())
}
