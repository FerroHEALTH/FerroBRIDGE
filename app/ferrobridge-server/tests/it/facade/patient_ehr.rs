// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! A `Patient` written in a party context is given the EHR its ITI-83
//! pseudonym keys (`PIXm` 3.1.0 §2:3.83.4.1.2.2), found with
//! `GET /ehr?subject_id=` or created with `POST /ehr` (ITS-REST 1.1.0 EHR,
//! `ehr_get_by_subject` and `ehr_create`), when the facade creates EHRs, and
//! the identity map records the party against that pseudonym. No
//! specification governs provisioning an EHR from a FHIR `Patient` create:
//! our own design.

use std::error::Error;

use ferrobridge_server::cdr::CdrClient;
use ferrobridge_server::cdr::config::CdrConfig;
use ferrobridge_server::facade::Settings;
use ferrobridge_server::facade::ehr;
use ferrobridge_server::facade::ehr::Policy;
use ferrobridge_server::facade::identity::PersonId;
use ferrobridge_server::facade::identity::store::MemoryStore;
use http::StatusCode;
use wiremock::Mock;
use wiremock::MockServer;
use wiremock::ResponseTemplate;
use wiremock::matchers;

use super::EHR_ID;
use super::app_over;
use super::call;
use super::demographic::UID;
use super::demographic::harness_with;
use super::demographic::patient;
use super::demographic::version;
use super::demographic::with_body;
use super::ehr_body;
use super::settings;
use super::stub::mount_no_ehr;
use crate::support::PSEUDONYM;

/// The pseudonym domain every EHR subject is keyed in.
const NAMESPACE: &str = "http://example.org/fhir";

/// Returns facade settings with `policy`.
fn linked(policy: Policy) -> Settings {
    Settings {
        ehr_policy: policy,
        ..settings()
    }
}

/// Mounts `POST /demographic/{class}` answering `204` with the first version
/// of the synthetic party.
async fn mount_party_create(cdr: &MockServer, class: &str) {
    Mock::given(matchers::method("POST"))
        .and(matchers::path(format!("/demographic/{class}")))
        .respond_with(
            ResponseTemplate::new(204).insert_header("ETag", format!("\"{}\"", version(1))),
        )
        .mount(cdr)
        .await;
}

/// Mounts `POST /ehr` answering `201` with [`EHR_ID`].
async fn mount_ehr_create(cdr: &MockServer) {
    Mock::given(matchers::method("POST"))
        .and(matchers::path("/ehr"))
        .respond_with(ResponseTemplate::new(201).insert_header("ETag", format!("\"{EHR_ID}\"")))
        .mount(cdr)
        .await;
}

/// Returns the requests the CDR received on `/ehr` itself, as the method
/// and the query or body they carried.
async fn ehr_calls(cdr: &MockServer) -> Vec<(String, String, serde_json::Value)> {
    cdr.received_requests()
        .await
        .unwrap_or_default()
        .iter()
        .filter(|request| request.url.path() == "/ehr")
        .map(|request| {
            (
                request.method.to_string(),
                request.url.query().unwrap_or_default().to_owned(),
                serde_json::from_slice(&request.body).unwrap_or(serde_json::Value::Null),
            )
        })
        .collect()
}

#[tokio::test]
async fn a_created_patient_gets_the_ehr_its_pseudonym_keys() -> Result<(), Box<dyn Error>> {
    let (cdr, facade) = harness_with(linked(Policy::CreateOnFirstWrite)).await?;
    mount_party_create(&cdr, "person").await;
    mount_no_ehr(&cdr).await;
    mount_ehr_create(&cdr).await;

    let (status, body) = call(
        app_over(&facade),
        with_body(http::Method::POST, "/fhir/Patient", &patient())?,
    )
    .await?;
    assert_eq!(StatusCode::CREATED, status, "{body}");
    assert_eq!(body["id"], UID);
    let calls = ehr_calls(&cdr).await;
    let lookup = calls
        .iter()
        .find(|(method, _, _)| method == "GET")
        .ok_or("the EHR was looked up")?;
    assert!(
        lookup.1.contains(&format!("subject_id={PSEUDONYM}")),
        "{lookup:?}"
    );
    assert!(
        lookup
            .1
            .contains("subject_namespace=http%3A%2F%2Fexample.org%2Ffhir"),
        "{lookup:?}"
    );
    let (_, _, status_sent) = calls
        .iter()
        .find(|(method, _, _)| method == "POST")
        .ok_or("the EHR was created")?;
    let reference = &status_sent["subject"]["external_ref"];
    assert_eq!(reference["namespace"], NAMESPACE, "{status_sent}");
    assert_eq!(reference["type"], "PERSON", "{status_sent}");
    assert_eq!(reference["id"]["value"], PSEUDONYM, "{status_sent}");
    assert_eq!(reference["id"]["scheme"], NAMESPACE, "{status_sent}");
    Ok(())
}

#[tokio::test]
async fn an_ehr_the_cdr_holds_for_the_party_is_found_not_created() -> Result<(), Box<dyn Error>> {
    let (cdr, facade) = harness_with(linked(Policy::CreateOnFirstWrite)).await?;
    mount_party_create(&cdr, "person").await;
    Mock::given(matchers::method("GET"))
        .and(matchers::path("/ehr"))
        .and(matchers::query_param("subject_id", PSEUDONYM))
        .and(matchers::query_param("subject_namespace", NAMESPACE))
        .respond_with(ResponseTemplate::new(200).set_body_string(ehr_body()))
        .mount(&cdr)
        .await;

    let (status, body) = call(
        app_over(&facade),
        with_body(http::Method::POST, "/fhir/Patient", &patient())?,
    )
    .await?;
    assert_eq!(StatusCode::CREATED, status, "{body}");
    let calls = ehr_calls(&cdr).await;
    assert_eq!(1, calls.len(), "{calls:?}");
    assert!(
        calls.iter().all(|(method, _, _)| method == "GET"),
        "{calls:?}"
    );
    Ok(())
}

#[tokio::test]
async fn under_the_existing_policy_a_patient_create_sends_no_ehr_call() -> Result<(), Box<dyn Error>>
{
    let (cdr, facade) = harness_with(linked(Policy::Existing)).await?;
    mount_party_create(&cdr, "person").await;

    let (status, body) = call(
        app_over(&facade),
        with_body(http::Method::POST, "/fhir/Patient", &patient())?,
    )
    .await?;
    assert_eq!(StatusCode::CREATED, status, "{body}");
    assert_eq!(
        ehr_calls(&cdr).await,
        Vec::<(String, String, serde_json::Value)>::new()
    );
    Ok(())
}

#[tokio::test]
async fn a_created_patient_party_is_recorded_against_its_pseudonym() -> Result<(), Box<dyn Error>> {
    let (cdr, facade) = harness_with(linked(Policy::Existing)).await?;
    mount_party_create(&cdr, "person").await;

    let (status, body) = call(
        app_over(&facade),
        with_body(http::Method::POST, "/fhir/Patient", &patient())?,
    )
    .await?;
    assert_eq!(StatusCode::CREATED, status, "{body}");
    let person = PersonId::new(NAMESPACE, PSEUDONYM)?;
    assert_eq!(
        Some(person.clone()),
        facade.store().person_of_party(UID).await?
    );
    assert_eq!(
        Some(String::from(UID)),
        facade.store().party_of(&person).await?
    );
    let asked = cdr
        .received_requests()
        .await
        .unwrap_or_default()
        .into_iter()
        .filter(|request| request.url.path() == crate::support::PIX_PATH)
        .count();
    assert_eq!(1, asked, "the pseudonym is asked once, before the commit");
    Ok(())
}

#[tokio::test]
async fn a_patient_with_no_accepted_identifier_is_refused_before_the_party_commit()
-> Result<(), Box<dyn Error>> {
    let (cdr, facade) = harness_with(linked(Policy::CreateOnFirstWrite)).await?;
    mount_party_create(&cdr, "person").await;
    let unaccepted = serde_json::json!({
        "resourceType": "Patient",
        "identifier": [{"system": "http://example.org/other", "value": "synthetic-2"}],
        "name": [{"text": "Synthetic Person"}]
    });

    let (status, body) = call(
        app_over(&facade),
        with_body(http::Method::POST, "/fhir/Patient", &unaccepted)?,
    )
    .await?;
    assert_eq!(StatusCode::UNPROCESSABLE_ENTITY, status, "{body}");
    let committed = cdr
        .received_requests()
        .await
        .unwrap_or_default()
        .into_iter()
        .filter(|request| request.url.path().starts_with("/demographic/person"))
        .count();
    assert_eq!(0, committed, "nothing reaches the CDR");
    Ok(())
}

#[tokio::test]
async fn a_practitioner_create_sends_no_ehr_call() -> Result<(), Box<dyn Error>> {
    let (cdr, facade) = harness_with(linked(Policy::CreateOnFirstWrite)).await?;
    mount_party_create(&cdr, "person").await;
    let practitioner = serde_json::json!({
        "resourceType": "Practitioner",
        "name": [{"text": "Synthetic Person"}]
    });

    let (status, body) = call(
        app_over(&facade),
        with_body(http::Method::POST, "/fhir/Practitioner", &practitioner)?,
    )
    .await?;
    assert_eq!(StatusCode::CREATED, status, "{body}");
    assert_eq!(
        ehr_calls(&cdr).await,
        Vec::<(String, String, serde_json::Value)>::new()
    );
    Ok(())
}

#[tokio::test]
async fn a_refused_ehr_creation_is_a_refusal_that_names_the_stored_patient()
-> Result<(), Box<dyn Error>> {
    let (cdr, facade) = harness_with(linked(Policy::CreateOnFirstWrite)).await?;
    mount_party_create(&cdr, "person").await;
    mount_no_ehr(&cdr).await;
    Mock::given(matchers::method("POST"))
        .and(matchers::path("/ehr"))
        .respond_with(
            ResponseTemplate::new(409)
                .insert_header("Content-Type", "application/json")
                .set_body_json(serde_json::json!({"message": "synthetic conflict"})),
        )
        .mount(&cdr)
        .await;

    let (status, body) = call(
        app_over(&facade),
        with_body(http::Method::POST, "/fhir/Patient", &patient())?,
    )
    .await?;
    assert_eq!(StatusCode::UNPROCESSABLE_ENTITY, status, "{body}");
    assert_eq!(body["resourceType"], "OperationOutcome", "{body}");
    let issues = body["issue"].as_array().ok_or("an OperationOutcome")?;
    assert!(
        issues.iter().any(|issue| issue["diagnostics"]
            .as_str()
            .unwrap_or_default()
            .contains("409")),
        "{body}"
    );
    let stored = issues.last().ok_or("a warning")?;
    assert_eq!(stored["severity"], "warning", "{body}");
    assert!(
        stored["diagnostics"]
            .as_str()
            .unwrap_or_default()
            .contains(&format!("Patient/{UID}")),
        "{body}"
    );
    Ok(())
}

#[tokio::test]
async fn two_concurrent_creates_for_one_subject_converge_on_one_ehr() -> Result<(), Box<dyn Error>>
{
    let cdr = MockServer::start().await;
    Mock::given(matchers::method("GET"))
        .and(matchers::path("/ehr"))
        .respond_with(ferrobridge_testkit::stubs::its_rest::not_found(
            "no EHR for the subject",
        ))
        .up_to_n_times(2)
        .mount(&cdr)
        .await;
    Mock::given(matchers::method("GET"))
        .and(matchers::path("/ehr"))
        .and(matchers::query_param("subject_id", PSEUDONYM))
        .and(matchers::query_param("subject_namespace", NAMESPACE))
        .respond_with(ResponseTemplate::new(200).set_body_string(ehr_body()))
        .mount(&cdr)
        .await;
    Mock::given(matchers::method("POST"))
        .and(matchers::path("/ehr"))
        .respond_with(ResponseTemplate::new(201).insert_header("ETag", format!("\"{EHR_ID}\"")))
        .up_to_n_times(1)
        .mount(&cdr)
        .await;
    Mock::given(matchers::method("POST"))
        .and(matchers::path("/ehr"))
        .respond_with(
            ResponseTemplate::new(409)
                .insert_header("Content-Type", "application/json")
                .set_body_json(serde_json::json!({"message": "an EHR with this subject exists"})),
        )
        .mount(&cdr)
        .await;
    let client = CdrClient::new(&CdrConfig::new(cdr.uri().parse()?))?;
    let person = PersonId::new(NAMESPACE, PSEUDONYM)?;
    let (first_store, second_store) = (MemoryStore::default(), MemoryStore::default());

    let (first, second) = tokio::join!(
        ehr::resolve(&client, &first_store, &person, Policy::CreateOnFirstWrite),
        ehr::resolve(&client, &second_store, &person, Policy::CreateOnFirstWrite),
    );
    assert_eq!(EHR_ID, first?.as_str());
    assert_eq!(EHR_ID, second?.as_str());
    let creates = ehr_calls(&cdr)
        .await
        .into_iter()
        .filter(|(method, _, _)| method == "POST")
        .count();
    assert_eq!(2, creates, "both writers tried to create");
    Ok(())
}
