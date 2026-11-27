// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! ML-DSA access tokens (RFC 9964): the facade takes a token an issuer signs
//! with an ML-DSA key its key set publishes as an `AKP` JWK, for each
//! parameter set it lists, refuses one an issuer does not list and one whose
//! `alg` is `none`, and the signer and verifier the facade uses reproduce
//! the JOSE examples of RFC 9964 Appendix A.1.

use std::error::Error;

use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use ferrohealth_smart::jws::algorithm::Algorithm as Verified;
use ferrohealth_smart::jws::compact;
use ferrohealth_smart::jws::jwk::Jwk;
use ferrohealth_smart::jws::key::PrivateKey;
use http::StatusCode;

use super::Issuer;
use super::authority;
use super::bearing;
use super::get;
use super::mint;
use super::ml_dsa_key;
use super::secured;
use crate::facade::harness;
use crate::facade::raw;

/// The read every case sends: an id the map does not know, so a request the
/// guard admits answers the handler's `404`.
const UNKNOWN: &str = "/fhir/Condition/never-written";

/// The ML-DSA parameter sets of RFC 9964 §5.
const PARAMETER_SETS: [Verified; 3] = [Verified::MlDsa44, Verified::MlDsa65, Verified::MlDsa87];

/// Sends the read under `token` and returns the status and the challenge.
async fn read_with(
    app: axum::Router,
    token: &str,
) -> Result<(StatusCode, Option<String>), Box<dyn Error>> {
    let response = raw(app, bearing(get(UNKNOWN), token)).await?;
    Ok((response.status(), super::challenge(&response)))
}

#[tokio::test]
async fn a_token_signed_with_each_ml_dsa_parameter_set_reaches_the_handler()
-> Result<(), Box<dyn Error>> {
    for algorithm in PARAMETER_SETS {
        let key = ml_dsa_key("pq1", algorithm);
        let issuer = Issuer::start(&[&key]).await;
        let harness = harness().await;
        let mut settings = issuer.settings();
        settings.issuers[0].algorithms = vec![Verified::Es256, algorithm];
        let app = secured(&harness, &authority(settings).await);
        let token = mint(&key, &issuer.claims("system/Condition.r"));
        let (status, challenge) = read_with(app, &token).await?;
        assert_eq!(StatusCode::NOT_FOUND, status, "{}", algorithm.name());
        assert_eq!(None, challenge, "{}", algorithm.name());
    }
    Ok(())
}

#[tokio::test]
async fn an_ml_dsa_token_from_an_issuer_that_lists_no_ml_dsa_is_refused()
-> Result<(), Box<dyn Error>> {
    let key = ml_dsa_key("pq1", Verified::MlDsa65);
    let issuer = Issuer::start(&[&key]).await;
    let harness = harness().await;
    let mut settings = issuer.settings();
    settings.issuers[0].algorithms = vec![Verified::Es256, Verified::MlDsa44];
    let app = secured(&harness, &authority(settings).await);
    let token = mint(&key, &issuer.claims("system/Condition.r"));
    let (status, challenge) = read_with(app, &token).await?;
    assert_eq!(StatusCode::UNAUTHORIZED, status);
    assert_eq!(
        Some(String::from(
            "Bearer error=\"invalid_token\", error_description=\"algorithm-refused\""
        )),
        challenge
    );
    Ok(())
}

// NOTE: RFC 8725 §3.1, a verifier refuses an unsecured JWS whatever key set
// the issuer publishes.
#[tokio::test]
async fn an_unsecured_token_is_refused() -> Result<(), Box<dyn Error>> {
    let key = ml_dsa_key("pq1", Verified::MlDsa65);
    let issuer = Issuer::start(&[&key]).await;
    let harness = harness().await;
    let mut settings = issuer.settings();
    settings.issuers[0].algorithms = vec![Verified::MlDsa65];
    let app = secured(&harness, &authority(settings).await);
    let header = URL_SAFE_NO_PAD.encode(r#"{"alg":"none","kid":"pq1","typ":"at+jwt"}"#);
    let claims = URL_SAFE_NO_PAD.encode(issuer.claims("system/Condition.r").to_string());
    let (status, challenge) = read_with(app, &format!("{header}.{claims}.")).await?;
    assert_eq!(StatusCode::UNAUTHORIZED, status);
    assert_eq!(
        Some(String::from(
            "Bearer error=\"invalid_token\", error_description=\"algorithm-refused\""
        )),
        challenge
    );
    Ok(())
}

/// The JOSE examples of RFC 9964 Appendix A.1, one JSON object per parameter
/// set, read from the vendored RFC text.
fn rfc_9964_examples() -> Result<Vec<serde_json::Value>, Box<dyn Error>> {
    let text = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../docs/specs/ietf-oauth/rfc9964.txt"),
    )?;
    let start = text.find("\nA.1.  JOSE\n").ok_or("Appendix A.1")?;
    let end = text.find("\nA.2.  COSE\n").ok_or("Appendix A.2")?;
    let section = text.get(start..end).ok_or("the section")?;
    let mut examples = Vec::new();
    for figure in section.split("Figure ").filter(|part| part.contains('{')) {
        let open = figure.find('{').ok_or("an example object")?;
        let close = figure.rfind('}').ok_or("an example object's end")?;
        // The RFC text wraps each long string over several lines; the wrap is
        // undone inside a string alone.
        let mut joined = String::new();
        let mut quoted = false;
        for character in figure.get(open..=close).ok_or("the object")?.chars() {
            match character {
                '"' => {
                    quoted = !quoted;
                    joined.push(character);
                }
                '\n' if quoted => {}
                other => joined.push(other),
            }
        }
        examples.push(serde_json::from_str(&joined)?);
    }
    Ok(examples)
}

#[test]
fn the_rfc_9964_jose_examples_verify_and_their_seeds_give_their_keys() -> Result<(), Box<dyn Error>>
{
    let examples = rfc_9964_examples()?;
    assert_eq!(3, examples.len(), "one example per parameter set");
    for (example, algorithm) in examples.iter().zip(PARAMETER_SETS) {
        let published = &example["jwk"];
        assert_eq!(Some(algorithm.name()), published["alg"].as_str());
        let mut public = published.clone();
        public
            .as_object_mut()
            .ok_or("the JWK is an object")?
            .remove("priv");
        let jwk: Jwk = serde_json::from_value(public)?;
        let token = example["jws"].as_str().ok_or("the example JWS")?;
        let decoded = compact::decode(token)?;
        assert_eq!(algorithm, decoded.header().alg);
        assert!(
            !decoded.verify(&jwk)?.is_empty(),
            "{}: the example verifies",
            algorithm.name()
        );
        let seed = URL_SAFE_NO_PAD.decode(published["priv"].as_str().ok_or("the seed")?)?;
        let private = PrivateKey::from_ml_dsa_seed(algorithm, &seed)?;
        assert_eq!(
            published["pub"],
            private.public_jwk().to_value()["pub"],
            "{}: the seed gives the published key",
            algorithm.name()
        );
        let mut tampered = String::from(token);
        tampered.push('A');
        assert!(
            compact::decode(&tampered)
                .and_then(|held| held.verify(&jwk).map(<[u8]>::len))
                .is_err(),
            "{}: a changed signature is refused",
            algorithm.name()
        );
    }
    Ok(())
}
