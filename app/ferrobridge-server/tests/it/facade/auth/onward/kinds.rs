// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! A `persons` issuer under `[cdr.credentials.onward]` and a `patients`
//! issuer under `[[cdr.onward]]`, side by side: each caller is exchanged
//! (RFC 8693 §2.1) at the issuer of its kind and never at the other. The
//! patient's token is a `patient/` grant with its `patient` launch context
//! (<https://hl7.org/fhir/smart-app-launch/scopes-and-launch-context.html>).

use std::error::Error;

use ferrohealth_smart::kind::IssuerKind;
use jsonwebtoken::Algorithm;
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
use super::received;
use crate::facade::auth::Issuer;
use crate::facade::auth::SigningKey;
use crate::facade::auth::condition_of;
use crate::facade::auth::ec_key;
use crate::facade::auth::mint;
use crate::facade::auth::now;

/// The patient the caller is launched for, and the subject of its record.
const PATIENT: &str = "p-1";

/// The `patient/` grant of the patient's token and of the onward one.
const PATIENT_SCOPE: &str = "patient/Condition.cru";

/// A world whose CDR lane carries persons and patients, each at its own
/// issuer, with the patients' issuer and its key.
struct Sides {
    /// The world of the `persons` issuer, trusting both issuers.
    world: World,
    /// The deployment's `patients` issuer, a realm of its own.
    patients: Issuer,
    /// The key the `patients` issuer signs with.
    key: SigningKey,
}

impl Sides {
    /// Starts the two issuers and the facade that trusts both.
    async fn start() -> Result<Self, Box<dyn Error>> {
        let key = ec_key("p1", Algorithm::ES256);
        let patients = Issuer::start(&[&key]).await;
        let table = format!(
            concat!(
                "\n[[cdr.onward]]\n",
                "issuer = \"{issuer}\"\n",
                "kind = \"patients\"\n",
                "token_endpoint = \"{issuer}/token\"\n",
                "audiences = [\"{audience}\"]\n",
                "indicators = [\"{audience}\"]\n",
                "[cdr.onward.keys]\n",
                "max_onward_lifetime = \"1h\"\n",
            ),
            issuer = patients.uri(),
            audience = CDR_AUDIENCE,
        );
        let mut world = World::start(Grant::Standard, &table).await?;
        let mut trusted = patients.issuer_settings();
        trusted.kind = IssuerKind::Patients;
        world.trusted.issuers.push(trusted);
        Ok(Self {
            world,
            patients,
            key,
        })
    }

    /// Returns the patient's token at the facade.
    fn patient_token(&self) -> String {
        let mut claims = self.patients.claims(PATIENT_SCOPE);
        claims["patient"] = serde_json::json!(PATIENT);
        mint(&self.key, &claims)
    }

    /// Mounts the `patients` issuer's exchange, answering with a token for
    /// the CDR that names the same patient, and returns that token.
    async fn mount_patient_exchange(&self) -> String {
        let now = now();
        let issued = mint(
            &self.key,
            &serde_json::json!({
                "iss": self.patients.uri(),
                "aud": CDR_AUDIENCE,
                "sub": "synthetic-client-sub",
                "client_id": BRIDGE_CLIENT,
                "scope": PATIENT_SCOPE,
                "patient": PATIENT,
                "iat": now,
                "exp": now + 300,
                "jti": "synthetic-patient-onward-jti",
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
            .mount(self.patients.server())
            .await;
        issued
    }

    /// Returns the form body of every request the `patients` issuer's token
    /// endpoint received.
    async fn patient_exchanges(&self) -> Vec<String> {
        received(self.patients.server(), "/token")
            .await
            .iter()
            .map(|request| String::from_utf8_lossy(&request.body).into_owned())
            .collect()
    }
}

#[tokio::test]
async fn a_patient_is_exchanged_at_the_patients_issuer_alone() -> Result<(), Box<dyn Error>> {
    let sides = Sides::start().await?;
    let issued = sides.mount_patient_exchange().await;
    sides.world.mount_own_token().await;
    sides.world.mount_cdr(&condition_of(PATIENT)).await;
    let token = sides.patient_token();

    create_and_read(&sides.world, &token).await?;

    assert_every_call_under(&sides.world.cdr_calls().await, &issued);
    let exchanges = sides.patient_exchanges().await;
    assert_eq!(1, exchanges.len(), "one exchange serves both requests");
    let body = exchanges.first().ok_or("an exchange")?;
    assert!(body.contains(&format!("subject_token={token}")), "{body}");
    assert!(
        sides.world.onward_requests().await.is_empty(),
        "the persons issuer never sees a patient"
    );
    Ok(())
}

#[tokio::test]
async fn a_person_beside_a_patients_issuer_is_exchanged_at_the_persons_issuer()
-> Result<(), Box<dyn Error>> {
    let sides = Sides::start().await?;
    let world = &sides.world;
    let issued = mint(&world.key, &world.issued_claims(Grant::Standard, 300));
    world
        .mount_token_endpoint(Grant::Standard, &issued, 300)
        .await;
    world.mount_cdr(&condition_of(PATIENT)).await;
    let token = mint(&world.key, &world.person_claims());

    create_and_read(world, &token).await?;

    assert_every_call_under(&world.cdr_calls().await, &issued);
    assert_eq!(1, world.onward_requests().await.len(), "one exchange");
    assert!(
        sides.patient_exchanges().await.is_empty(),
        "the patients issuer never sees a person"
    );
    Ok(())
}
