// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The facade sends the CDR no attestation. An openEHR `ATTESTATION` ("an
//! explicit signing by one healthcare agent of particular content", RM
//! Common IM) travels as a CONTRIBUTION audit of type `UPDATE_ATTESTATION`
//! (ITS-REST `schemas/common/UpdateAttestation.yaml`); the facade has no path
//! that builds one, so a FHIR `Provenance` carrying a `signature` is refused
//! as an entry no program maps and never reaches the CDR. A future
//! attestation path must refuse every caller that is not a health
//! professional; no specification governs that limit: our own design.

use std::error::Error;

use http::StatusCode;

use super::EHR_ID;
use super::condition;
use super::harness;
use super::post_transaction;
use super::stub::ECHOED;
use super::stub::Echo;
use super::stub::mount_echo;
use super::stub::mount_ehr;

/// Returns a `Provenance` of `target` signed by a synthetic practitioner
/// (<https://hl7.org/fhir/R4/provenance.html>, `Provenance.signature`).
fn signed_provenance(target: &str) -> serde_json::Value {
    serde_json::json!({
        "resourceType": "Provenance",
        "target": [{ "reference": target }],
        "recorded": "2026-01-01T00:00:00Z",
        "agent": [{ "who": { "display": "Synthetic Practitioner" } }],
        "signature": [{
            "type": [{
                "system": "urn:iso-astm:E1762-95:2013",
                "code": "1.2.840.10065.1.12.1.1",
                "display": "Author's Signature"
            }],
            "when": "2026-01-01T00:00:00Z",
            "who": { "display": "Synthetic Practitioner" },
            "sigFormat": "application/jose",
            "data": "c3ludGhldGljLXNpZ25hdHVyZQ=="
        }]
    })
}

#[tokio::test]
async fn a_signed_provenance_in_a_transaction_is_refused_and_never_attests()
-> Result<(), Box<dyn Error>> {
    let harness = harness().await;
    mount_ehr(&harness.cdr).await;
    mount_echo(&harness.cdr, ECHOED, Echo::Sent).await;
    let bundle = serde_json::json!({
        "resourceType": "Bundle",
        "type": "transaction",
        "entry": [
            { "fullUrl": "urn:uuid:0000-condition", "resource": condition(),
              "request": { "method": "POST", "url": "Condition" } },
            { "fullUrl": "urn:uuid:0000-provenance",
              "resource": signed_provenance("urn:uuid:0000-condition"),
              "request": { "method": "POST", "url": "Provenance" } }
        ]
    });

    let (status, body) = post_transaction(&harness, &bundle).await?;

    assert_eq!(
        StatusCode::UNPROCESSABLE_ENTITY,
        status,
        "a signed Provenance is refused as an entry no program maps, never attested: {body}"
    );
    assert_eq!(
        0,
        harness
            .received("POST", &format!("/ehr/{EHR_ID}/contribution"))
            .await,
        "the refused transaction commits nothing"
    );
    let requests = harness.cdr.received_requests().await.unwrap_or_default();
    for request in &requests {
        let sent = String::from_utf8_lossy(&request.body);
        assert!(
            !sent.contains("ATTESTATION") && !sent.contains("attestation"),
            "{} {} carries an attestation",
            request.method,
            request.url.path()
        );
    }
    Ok(())
}

#[tokio::test]
async fn a_signed_provenance_on_its_own_is_refused_and_reaches_no_cdr() -> Result<(), Box<dyn Error>>
{
    let harness = harness().await;
    let request = http::Request::post("/fhir/Provenance")
        .header(http::header::CONTENT_TYPE, "application/fhir+json")
        .body(axum::body::Body::from(
            signed_provenance("Condition/c-1").to_string(),
        ))?;

    let (status, body) = super::call(harness.app(), request).await?;

    assert_eq!(
        StatusCode::NOT_FOUND,
        status,
        "no program serves Provenance: {body}"
    );
    let writes = harness
        .cdr
        .received_requests()
        .await
        .unwrap_or_default()
        .iter()
        .filter(|request| request.method.as_str() != "GET")
        .count();
    assert_eq!(0, writes, "nothing was written to the CDR");
    Ok(())
}
