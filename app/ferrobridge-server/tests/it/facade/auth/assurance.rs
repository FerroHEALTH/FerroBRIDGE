// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The eIDAS assurance level of every facade access token: the issuer maps
//! the `acr` claim (OpenID Connect Core 1.0 §2) to a level, and a token below
//! `[assurance] fhir`, or carrying no mapped `acr`, answers `401` with the
//! RFC 9470 §3 step-up challenge and an `OperationOutcome`.

use std::collections::BTreeMap;
use std::error::Error;
use std::sync::Arc;

use axum::body::Body;
use ferrobridge_server::assurance::Minimums;
use ferrobridge_server::smart::Smart;
use ferrohealth_smart::assurance::AssuranceLevel;
use ferrohealth_smart::assurance::AssuranceSection;
use http::Request;
use http::StatusCode;
use jsonwebtoken::Algorithm;

use super::Issuer;
use super::SigningKey;
use super::authority;
use super::bearing;
use super::challenge;
use super::ec_key;
use super::get;
use super::mint;
use super::secured;
use crate::facade::Harness;
use crate::facade::harness;
use crate::facade::raw;
use crate::facade::stub::mount_ehr;

/// The route every case reads: a Condition the CDR never wrote.
const READ: &str = "/fhir/Condition/never-written";

/// The scope every token grants.
const SCOPE: &str = "system/Condition.r";

/// One case: an issuer mapping three `acr` values, and the facade held to
/// a minimum.
struct Case {
    /// The stub authorization server.
    issuer: Issuer,
    /// The key every token is signed with.
    key: SigningKey,
    /// The facade.
    harness: Harness,
    /// The resource server.
    authority: Arc<Smart>,
}

impl Case {
    /// Returns a case whose facade requires `fhir` and whose issuer requires
    /// `issuer_minimum` of its own.
    async fn new(fhir: Option<AssuranceLevel>, issuer_minimum: Option<AssuranceLevel>) -> Self {
        let key = ec_key("k1", Algorithm::ES256);
        let issuer = Issuer::start(&[&key]).await;
        let harness = harness().await;
        mount_ehr(&harness.cdr).await;
        let mut settings = issuer.settings();
        if let Some(first) = settings.issuers.first_mut() {
            first.assurance = Some(AssuranceSection {
                levels: BTreeMap::from([
                    (String::from("urn:acr:low"), AssuranceLevel::Low),
                    (
                        String::from("urn:acr:substantial"),
                        AssuranceLevel::Substantial,
                    ),
                    (String::from("urn:acr:high"), AssuranceLevel::High),
                ]),
                minimum: issuer_minimum,
                ..AssuranceSection::default()
            });
        }
        settings.assurance = Minimums { fhir, shl: None };
        let authority = authority(settings).await;
        Self {
            issuer,
            key,
            harness,
            authority,
        }
    }

    /// Reads [`READ`] under a token whose `acr` is `acr`, none when `None`,
    /// and returns the status, the challenge and the body text.
    async fn read(
        &self,
        acr: Option<&str>,
    ) -> Result<(StatusCode, Option<String>, String), Box<dyn Error>> {
        let mut claims = self.issuer.claims(SCOPE);
        if let Some(acr) = acr {
            claims["acr"] = serde_json::json!(acr);
        }
        let response = raw(
            secured(&self.harness, &self.authority),
            bearing(get(READ), &mint(&self.key, &claims)),
        )
        .await?;
        let status = response.status();
        let challenge = challenge(&response);
        let bytes = axum::body::to_bytes(response.into_body(), 1024 * 1024).await?;
        Ok((
            status,
            challenge,
            String::from_utf8_lossy(&bytes).into_owned(),
        ))
    }
}

/// Asserts the step-up refusal naming `accepted` as `acr_values`.
fn assert_step_up(
    (status, challenge, body): &(StatusCode, Option<String>, String),
    accepted: &str,
) {
    assert_eq!(StatusCode::UNAUTHORIZED, *status, "{body}");
    assert_eq!(
        Some(format!(
            "Bearer error=\"insufficient_user_authentication\", error_description=\"assurance-below-minimum\", acr_values=\"{accepted}\""
        ))
        .as_deref(),
        challenge.as_deref(),
        "RFC 9470 §3: the error code and the accepted acr values"
    );
    let outcome: serde_json::Value = serde_json::from_str(body).expect("a JSON body");
    assert_eq!("OperationOutcome", outcome["resourceType"], "{body}");
    assert_eq!("login", outcome["issue"][0]["code"], "{body}");
    assert!(
        !body.contains("urn:acr"),
        "the body names a level value: {body}"
    );
}

#[tokio::test]
async fn a_token_at_or_above_the_minimum_is_admitted() -> Result<(), Box<dyn Error>> {
    let case = Case::new(Some(AssuranceLevel::Substantial), None).await;
    for acr in ["urn:acr:substantial", "urn:acr:high"] {
        let (status, _, body) = case.read(Some(acr)).await?;
        assert_eq!(StatusCode::NOT_FOUND, status, "{acr}: {body}");
    }
    Ok(())
}

#[tokio::test]
async fn a_token_below_the_minimum_gets_the_step_up_challenge() -> Result<(), Box<dyn Error>> {
    let case = Case::new(Some(AssuranceLevel::Substantial), None).await;
    assert_step_up(
        &case.read(Some("urn:acr:low")).await?,
        "urn:acr:high urn:acr:substantial",
    );
    Ok(())
}

#[tokio::test]
async fn a_token_without_a_mapped_acr_is_refused_when_a_minimum_is_set()
-> Result<(), Box<dyn Error>> {
    let case = Case::new(Some(AssuranceLevel::High), None).await;
    assert_step_up(&case.read(None).await?, "urn:acr:high");
    assert_step_up(&case.read(Some("urn:acr:other")).await?, "urn:acr:high");
    Ok(())
}

#[tokio::test]
async fn without_a_minimum_no_token_is_refused_for_its_level() -> Result<(), Box<dyn Error>> {
    let case = Case::new(None, None).await;
    for acr in [None, Some("urn:acr:low")] {
        let (status, _, body) = case.read(acr).await?;
        assert_eq!(StatusCode::NOT_FOUND, status, "{acr:?}: {body}");
    }
    Ok(())
}

#[tokio::test]
async fn the_issuer_minimum_holds_beside_the_route_class() -> Result<(), Box<dyn Error>> {
    let case = Case::new(Some(AssuranceLevel::Low), Some(AssuranceLevel::Substantial)).await;
    assert_step_up(
        &case.read(Some("urn:acr:low")).await?,
        "urn:acr:high urn:acr:substantial",
    );
    let (status, _, body) = case.read(Some("urn:acr:substantial")).await?;
    assert_eq!(StatusCode::NOT_FOUND, status, "{body}");
    Ok(())
}

#[tokio::test]
async fn the_capability_statement_stays_open_under_a_minimum() -> Result<(), Box<dyn Error>> {
    let case = Case::new(Some(AssuranceLevel::High), None).await;
    let response = raw(
        secured(&case.harness, &case.authority),
        Request::get("/fhir/metadata").body(Body::empty())?,
    )
    .await?;
    assert_eq!(StatusCode::OK, response.status());
    Ok(())
}
