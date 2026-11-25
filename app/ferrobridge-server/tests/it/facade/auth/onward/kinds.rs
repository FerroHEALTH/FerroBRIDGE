// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The users of a SMART issuer under `[cdr.endpoint.credentials.onward]`:
//! every user is exchanged (RFC 8693 §2.1) at that issuer, whatever the
//! type of their `fhirUser`, and a patient's own token is a `patient/` grant
//! with its `patient` launch context
//! (<https://hl7.org/fhir/smart-app-launch/scopes-and-launch-context.html>).
//! The token `FerroSMART` issues for the CDR names no actor and carries the
//! SMART on openEHR scopes.

use std::error::Error;

use wiremock::Mock;
use wiremock::ResponseTemplate;
use wiremock::matchers;

use super::BRIDGE_CLIENT;
use super::CDR_AUDIENCE;
use super::Grant;
use super::TOKEN_EXCHANGE;
use super::World;
use super::assert_every_call_under;
use super::create_and_read;
use crate::facade::auth::condition_of;
use crate::facade::auth::mint;
use crate::facade::auth::now;
use crate::facade::summary;

/// The patient the caller is launched for, and the subject of its record.
const PATIENT: &str = "p-1";

/// The `patient/` grant of the patient's token.
const PATIENT_SCOPE: &str = "patient/Condition.cru";

/// Returns the claims of the patient's own token for `patient` at the
/// facade, granting `scope`.
fn patient_claims(world: &World, patient: &str, scope: &str) -> serde_json::Value {
    let mut claims = world.issuer.claims(scope);
    claims["patient"] = serde_json::json!(patient);
    claims["fhirUser"] = serde_json::json!(format!("Patient/{patient}"));
    claims
}

/// Mounts the `FerroSMART` exchange of a patient's token for `patient`
/// (FerroHEALTH/FerroSMART#785): the patient as `sub`, the openEHR base as
/// `aud`, the bridge as `client_id` with no `act`, the scopes
/// `patient/composition-*.r` and `patient/aql-*.s`, the launch context with
/// `ehrId`, the subject token's `acr` and `auth_time`, for five minutes;
/// returns that token.
async fn mount_ferrosmart_exchange(world: &World, patient: &str) -> String {
    let now = now();
    let issued = mint(
        &world.key,
        &serde_json::json!({
            "iss": world.issuer.uri(),
            "aud": CDR_AUDIENCE,
            "sub": "synthetic-client-sub",
            "client_id": BRIDGE_CLIENT,
            "scope": "patient/composition-*.r patient/aql-*.s",
            "fhirUser": format!("Patient/{patient}"),
            "patient": patient,
            "ehrId": crate::facade::EHR_ID,
            "acr": "http://eidas.europa.eu/LoA/substantial",
            "auth_time": now - 60,
            "iat": now,
            "exp": now + 300,
            "jti": "synthetic-ferrosmart-onward-jti",
        }),
    );
    Mock::given(matchers::method("POST"))
        .and(matchers::path("/token"))
        .and(matchers::body_string_contains(TOKEN_EXCHANGE))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "access_token": issued,
            "issued_token_type": "urn:ietf:params:oauth:token-type:access_token",
            "token_type": "Bearer",
            "expires_in": 300,
        })))
        .mount(world.issuer.server())
        .await;
    issued
}

#[tokio::test]
async fn a_user_of_every_fhir_user_type_is_exchanged_at_the_smart_issuer()
-> Result<(), Box<dyn Error>> {
    for fhir_user in [
        None,
        Some("Practitioner/pr-1"),
        Some("PractitionerRole/pr-1"),
        Some("RelatedPerson/rp-1"),
        Some("Person/pe-1"),
    ] {
        let world = World::start(Grant::Standard, "").await?;
        let issued = mint(&world.key, &world.issued_claims(300));
        world
            .mount_token_endpoint(Grant::Standard, &issued, 300)
            .await;
        world.mount_cdr(&condition_of(PATIENT)).await;
        let mut claims = world.person_claims();
        if let Some(fhir_user) = fhir_user {
            claims["fhirUser"] = serde_json::json!(fhir_user);
        }
        let token = mint(&world.key, &claims);

        create_and_read(&world, &token).await?;

        assert_every_call_under(&world.cdr_calls().await, &issued);
        let exchanges = world.onward_requests().await;
        assert_eq!(1, exchanges.len(), "{fhir_user:?}: one exchange");
        let body = exchanges.first().ok_or("an exchange")?;
        assert!(body.contains(&format!("subject_token={token}")), "{body}");
    }
    Ok(())
}

#[tokio::test]
async fn a_patient_s_own_token_is_exchanged_at_the_smart_issuer() -> Result<(), Box<dyn Error>> {
    let world = World::start(Grant::Standard, "").await?;
    let issued = mount_ferrosmart_exchange(&world, PATIENT).await;
    world.mount_own_token().await;
    world.mount_cdr(&condition_of(PATIENT)).await;
    let token = mint(&world.key, &patient_claims(&world, PATIENT, PATIENT_SCOPE));

    create_and_read(&world, &token).await?;

    assert_every_call_under(&world.cdr_calls().await, &issued);
    let exchanges = world.onward_requests().await;
    assert_eq!(1, exchanges.len(), "one exchange serves both requests");
    let body = exchanges.first().ok_or("an exchange")?;
    assert!(body.contains(&format!("subject_token={token}")), "{body}");
    Ok(())
}

/// Sends `GET path` with `token` to a facade over the patient summary
/// fixtures and the world's CDR, with the synthetic record mounted, and
/// returns the answer; `None` when the profile packages are not fetched.
async fn over_summary(
    world: &World,
    path: &str,
    token: &str,
) -> Result<Option<http::Response<axum::body::Body>>, Box<dyn Error>> {
    let term = wiremock::MockServer::start().await;
    let client =
        ferrobridge_server::cdr::CdrClient::new(&world.cdr_config.clone().with_demographic(true))?;
    let store =
        std::sync::Arc::new(ferrobridge_server::facade::identity::store::MemoryStore::default());
    let Some(facade) = summary::summary_facade(
        &world.harness.cdr,
        client,
        crate::support::translator(&term)?,
        &store,
    )
    .await?
    else {
        return Ok(None);
    };
    summary::mount_terminology(&term).await;
    summary::mount_record(&world.harness.cdr, &[summary::composition()?]).await;
    let state = ferrobridge_server::state::AppState::with_health(
        ferrohealth_health::registry::Registry::default(),
    )
    .with_facade(facade)
    .with_auth(crate::facade::auth::authority(world.trusted.clone()).await);
    let response = crate::facade::raw(
        ferrobridge_server::router(
            std::sync::Arc::new(state),
            &crate::facade::server_settings(),
        ),
        crate::facade::auth::bearing(
            http::Request::get(path).body(axum::body::Body::empty())?,
            token,
        ),
    )
    .await?;
    Ok(Some(response))
}

#[tokio::test]
async fn a_patient_s_summary_is_read_from_the_cdr_under_the_exchanged_token()
-> Result<(), Box<dyn Error>> {
    let world = World::start(Grant::Standard, "").await?;
    world.mount_own_token().await;
    let issued = mount_ferrosmart_exchange(&world, summary::UID).await;
    let token = mint(
        &world.key,
        &patient_claims(&world, summary::UID, "patient/*.rs"),
    );
    let path = format!("/fhir/Patient/{}/$summary", summary::UID);
    let Some(response) = over_summary(&world, &path, &token).await? else {
        return Ok(());
    };
    assert_eq!(http::StatusCode::OK, response.status());

    assert_every_call_under(&world.cdr_calls().await, &issued);
    let exchanges = world.onward_requests().await;
    assert_eq!(1, exchanges.len(), "one exchange serves the summary");
    assert!(
        exchanges
            .first()
            .is_some_and(|body| body.contains(&format!("subject_token={token}"))),
        "{exchanges:?}"
    );
    Ok(())
}

// NOTE: SMART App Launch 2.2.0, Backend Services, and RFC 8693 §2.1: a
// `system/` token names no user, so nothing is exchanged (#2139).
#[tokio::test]
async fn a_backend_services_read_of_the_launch_patient_is_never_exchanged()
-> Result<(), Box<dyn Error>> {
    let world = World::start(Grant::Standard, "").await?;
    world.mount_own_token().await;
    mount_ferrosmart_exchange(&world, summary::UID).await;
    let token = mint(
        &world.key,
        &world.issuer.machine_claims("system/Patient.rs"),
    );
    let path = format!("/fhir/Patient/{}", summary::UID);
    let Some(response) = over_summary(&world, &path, &token).await? else {
        return Ok(());
    };
    assert_eq!(http::StatusCode::OK, response.status(), "the patient reads");

    assert!(
        world.onward_requests().await.is_empty(),
        "a Backend Services token was exchanged"
    );
    assert_every_call_under(&world.cdr_calls().await, &super::OWN_TOKEN);
    Ok(())
}
