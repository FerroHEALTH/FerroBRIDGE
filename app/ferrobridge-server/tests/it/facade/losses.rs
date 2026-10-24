// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The losses a write's mapping declared, shown to the client that wrote.
//!
//! A transaction entry carries them in `response.outcome`, "an
//! `OperationOutcome` containing hints and warnings produced as part of
//! processing this entry" (R4 bundle-definitions.html
//! §`Bundle.entry.response.outcome`), and a single write carries them when
//! the client asks `Prefer: return=OperationOutcome` (R4 http.html §Managing
//! Return Content).

use http::Request;
use http::StatusCode;
use http::header;
use std::error::Error;

use axum::body::Body;

use super::call;
use super::condition;
use super::harness;
use super::post_transaction;
use super::stub::ECHOED;
use super::stub::Echo;
use super::stub::mount_create;
use super::stub::mount_echo;
use super::stub::mount_ehr;
use super::transaction_of;

/// The system the operations render a loss's openEHR path under.
const OPENEHR_PATH: &str = fhirconnect::operations::issues::OPENEHR_PATH_SYSTEM;

/// Returns the issues of `outcome` that report a field the engine filled.
fn defaulted(outcome: &serde_json::Value) -> Vec<&serde_json::Value> {
    outcome["issue"]
        .as_array()
        .map(|issues| {
            issues
                .iter()
                .filter(|issue| {
                    issue["severity"] == "information"
                        && issue["code"] == "informational"
                        && issue["details"]["coding"]
                            .as_array()
                            .is_some_and(|codings| {
                                codings
                                    .iter()
                                    .any(|coding| coding["system"] == OPENEHR_PATH)
                            })
                })
                .collect()
        })
        .unwrap_or_default()
}

#[tokio::test]
async fn a_transaction_entry_answers_its_declared_losses_in_its_outcome()
-> Result<(), Box<dyn Error>> {
    let harness = harness().await;
    mount_ehr(&harness.cdr).await;
    mount_echo(&harness.cdr, ECHOED, Echo::Sent).await;
    let (status, body) = post_transaction(
        &harness,
        &transaction_of(&[("urn:uuid:0000-good", condition())]),
    )
    .await?;
    assert_eq!(StatusCode::OK, status, "{body}");
    let outcome = &body["entry"][0]["response"]["outcome"];
    assert_eq!(
        Some("OperationOutcome"),
        outcome["resourceType"].as_str(),
        "{body}"
    );
    assert!(
        !defaulted(outcome).is_empty(),
        "the composition fields the engine filled are named: {outcome}"
    );
    Ok(())
}

#[tokio::test]
async fn a_transaction_sent_again_answers_no_losses_for_what_it_did_not_commit()
-> Result<(), Box<dyn Error>> {
    let harness = harness().await;
    mount_ehr(&harness.cdr).await;
    mount_echo(&harness.cdr, ECHOED, Echo::Sent).await;
    let bundle = transaction_of(&[("urn:uuid:0000-good", condition())]);
    let (_, first) = post_transaction(&harness, &bundle).await?;
    let (status, second) = post_transaction(&harness, &bundle).await?;
    assert_eq!(StatusCode::OK, status, "{second}");
    assert!(
        first["entry"][0]["response"]["outcome"].is_object(),
        "{first}"
    );
    assert!(
        second["entry"][0]["response"]["outcome"].is_null(),
        "a redelivery commits nothing, so it declares nothing: {second}"
    );
    Ok(())
}

#[tokio::test]
async fn a_single_create_asking_for_an_outcome_answers_its_declared_losses()
-> Result<(), Box<dyn Error>> {
    let harness = harness().await;
    mount_ehr(&harness.cdr).await;
    mount_create(&harness.cdr, super::VERSION_ONE, &harness.composition()).await;
    let (status, body) = call(
        harness.app(),
        Request::post("/fhir/Condition")
            .header(header::CONTENT_TYPE, "application/fhir+json")
            .header("Prefer", "return=OperationOutcome")
            .body(Body::from(condition().to_string()))?,
    )
    .await?;
    assert_eq!(StatusCode::CREATED, status, "{body}");
    assert_eq!(
        Some("OperationOutcome"),
        body["resourceType"].as_str(),
        "{body}"
    );
    assert!(
        body["issue"][0]["diagnostics"]
            .as_str()
            .is_some_and(|text| text.contains("committed as version")),
        "{body}"
    );
    assert!(!defaulted(&body).is_empty(), "{body}");
    Ok(())
}

#[tokio::test]
async fn a_single_create_asking_for_the_resource_answers_no_outcome() -> Result<(), Box<dyn Error>>
{
    // R4 http.html §Managing Return Content: the OperationOutcome "should only
    // be returned if explicitly requested".
    let harness = harness().await;
    mount_ehr(&harness.cdr).await;
    mount_create(&harness.cdr, super::VERSION_ONE, &harness.composition()).await;
    let (status, body) = call(
        harness.app(),
        Request::post("/fhir/Condition")
            .header(header::CONTENT_TYPE, "application/fhir+json")
            .header("Prefer", "return=representation")
            .body(Body::from(condition().to_string()))?,
    )
    .await?;
    assert_eq!(StatusCode::CREATED, status, "{body}");
    assert_eq!(Some("Condition"), body["resourceType"].as_str(), "{body}");
    Ok(())
}
