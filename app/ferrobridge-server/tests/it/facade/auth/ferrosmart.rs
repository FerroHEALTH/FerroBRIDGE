// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The facade behind `FerroSMART`: one `persons` issuer under the `ferrosmart`
//! preset of `ferrohealth-smart`, read from the configuration as an operator
//! writes it.
//!
//! The preset reads plain RFC 9068 access tokens with `typ` `at+jwt` alone
//! (RFC 9068 §2.1) and `scope` among the required claims. A SMART app the
//! table lists in `delegates` acts for the signed-in person, and a client
//! credentials token, whose `sub` is its `client_id` (RFC 9068 §2.2), is a
//! machine's. No specification governs the deployment shape: our own design.

use std::collections::BTreeMap;
use std::error::Error;

use ferrobridge_server::config::Config;
use ferrobridge_server::config::Process;
use ferrobridge_server::smart::SmartSettings;
use http::StatusCode;
use jsonwebtoken::Algorithm;
use jsonwebtoken::Header;

use super::AUDIENCE;
use super::Issuer;
use super::authority;
use super::bearing;
use super::ec_key;
use super::get;
use super::mint;
use super::mint_with;
use super::secured;
use crate::facade::harness;
use crate::facade::raw;

/// The read every case sends: an id the map does not know, so a request the
/// guard admits answers the handler's `404`.
const UNKNOWN: &str = "/fhir/Condition/never-written";

/// The SMART app the deployment admits for its people.
const APP: &str = "growth-chart";

/// Returns the resource-server settings of a bridge behind the `FerroSMART` at
/// `issuer`, as `[auth]` reads them.
fn behind_ferrosmart(issuer: &str) -> Result<SmartSettings, Box<dyn Error>> {
    let text = format!(
        concat!(
            "[mappings]\ndirectory = \"/srv/mappings\"\ntemplates = \"/srv/templates\"\n\n",
            "[auth]\nenabled = true\n\n",
            "[[auth.issuer]]\nissuer = \"{issuer}\"\nkind = \"persons\"\nprofile = \"ferrosmart\"\n",
            "audience = [\"{audience}\"]\ndelegates = [{{ client = \"{app}\" }}]\n",
            "jwks_refresh_interval = \"0s\"\n",
        ),
        issuer = issuer,
        audience = AUDIENCE,
        app = APP,
    );
    Config::from_sources(Some(&text), &BTreeMap::new())?
        .resolve(Process::Server)?
        .auth
        .ok_or_else(|| "auth is on".into())
}

/// Returns the status the guarded read answers under `token`.
async fn status_under(issuer: &Issuer, token: &str) -> Result<StatusCode, Box<dyn Error>> {
    let harness = harness().await;
    let app = secured(
        &harness,
        &authority(behind_ferrosmart(&issuer.uri())?).await,
    );
    Ok(raw(app, bearing(get(UNKNOWN), token)).await?.status())
}

#[tokio::test]
async fn a_person_through_a_listed_app_is_admitted() -> Result<(), Box<dyn Error>> {
    let key = ec_key("k1", Algorithm::ES256);
    let issuer = Issuer::start(&[&key]).await;
    let mut claims = issuer.claims("user/Condition.rs");
    claims["client_id"] = serde_json::json!(APP);
    claims["sub"] = serde_json::json!("pairwise-person-1");
    claims["fhirUser"] = serde_json::json!("Practitioner/p-1");
    assert_eq!(
        StatusCode::NOT_FOUND,
        status_under(&issuer, &mint(&key, &claims)).await?
    );
    Ok(())
}

#[tokio::test]
async fn a_backend_services_client_is_admitted_as_a_machine() -> Result<(), Box<dyn Error>> {
    let key = ec_key("k1", Algorithm::ES256);
    let issuer = Issuer::start(&[&key]).await;
    let mut claims = issuer.claims("system/Condition.rs");
    claims["client_id"] = serde_json::json!("backend-service");
    claims["sub"] = serde_json::json!("backend-service");
    assert_eq!(
        StatusCode::NOT_FOUND,
        status_under(&issuer, &mint(&key, &claims)).await?
    );
    Ok(())
}

#[tokio::test]
async fn a_person_through_an_unlisted_app_is_refused() -> Result<(), Box<dyn Error>> {
    let key = ec_key("k1", Algorithm::ES256);
    let issuer = Issuer::start(&[&key]).await;
    let mut claims = issuer.claims("user/Condition.rs");
    claims["client_id"] = serde_json::json!("unlisted-app");
    claims["sub"] = serde_json::json!("pairwise-person-1");
    assert_eq!(
        StatusCode::UNAUTHORIZED,
        status_under(&issuer, &mint(&key, &claims)).await?
    );
    Ok(())
}

#[tokio::test]
async fn a_token_without_a_scope_is_refused() -> Result<(), Box<dyn Error>> {
    let key = ec_key("k1", Algorithm::ES256);
    let issuer = Issuer::start(&[&key]).await;
    let mut claims = issuer.claims("user/Condition.rs");
    claims["client_id"] = serde_json::json!(APP);
    claims
        .as_object_mut()
        .ok_or("claims are an object")?
        .remove("scope");
    assert_eq!(
        StatusCode::UNAUTHORIZED,
        status_under(&issuer, &mint(&key, &claims)).await?
    );
    Ok(())
}

#[tokio::test]
async fn the_long_form_type_is_refused() -> Result<(), Box<dyn Error>> {
    // RFC 9068 §2.1: FerroSMART writes `at+jwt`, so the preset takes no other `typ`.
    let key = ec_key("k1", Algorithm::ES256);
    let issuer = Issuer::start(&[&key]).await;
    let mut claims = issuer.claims("user/Condition.rs");
    claims["client_id"] = serde_json::json!(APP);
    let mut header = Header::new(Algorithm::ES256);
    header.kid = Some(String::from("k1"));
    header.typ = Some(String::from("application/at+jwt"));
    assert_eq!(
        StatusCode::UNAUTHORIZED,
        status_under(&issuer, &mint_with(&key, &header, &claims)).await?
    );
    Ok(())
}
