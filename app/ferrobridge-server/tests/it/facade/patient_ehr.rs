// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! A `Patient` created in a party context is given the EHR the subject link
//! keys by the new party's id, found with `GET /ehr?subject_id=` or created
//! with `POST /ehr` (ITS-REST 1.1.0 EHR, `ehr_get_by_subject` and
//! `ehr_create`), when the facade creates EHRs. No specification governs
//! provisioning an EHR from a FHIR `Patient` create: our own design.

use std::error::Error as StdError;

use ferrobridge_server::facade::Settings;
use ferrobridge_server::facade::ehr::Policy;
use ferrobridge_server::facade::subject::IdForm;
use ferrobridge_server::facade::subject::SubjectLink;
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

/// The namespace the CDR's owner links each EHR to its party under.
const NAMESPACE: &str = "fhir_facade";

/// Returns facade settings with `policy` and a subject link that keys a
/// patient by its logical id in [`NAMESPACE`].
fn linked(policy: Policy) -> Result<Settings, Box<dyn StdError>> {
    Ok(Settings {
        ehr_policy: policy,
        subject: SubjectLink::new(NAMESPACE, IdForm::LogicalId, "PERSON")?,
        ..settings()
    })
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
async fn a_created_patient_gets_the_ehr_its_party_id_keys() -> Result<(), Box<dyn StdError>> {
    let (cdr, facade) = harness_with(linked(Policy::CreateOnFirstWrite)?).await?;
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
        lookup.1.contains(&format!("subject_id={UID}")),
        "{lookup:?}"
    );
    assert!(
        lookup.1.contains(&format!("subject_namespace={NAMESPACE}")),
        "{lookup:?}"
    );
    let (_, _, status_sent) = calls
        .iter()
        .find(|(method, _, _)| method == "POST")
        .ok_or("the EHR was created")?;
    let reference = &status_sent["subject"]["external_ref"];
    assert_eq!(reference["namespace"], NAMESPACE, "{status_sent}");
    assert_eq!(reference["type"], "PERSON", "{status_sent}");
    assert_eq!(reference["id"]["value"], UID, "{status_sent}");
    assert_eq!(reference["id"]["scheme"], NAMESPACE, "{status_sent}");
    Ok(())
}

#[tokio::test]
async fn an_ehr_the_cdr_holds_for_the_party_is_found_not_created() -> Result<(), Box<dyn StdError>>
{
    let (cdr, facade) = harness_with(linked(Policy::CreateOnFirstWrite)?).await?;
    mount_party_create(&cdr, "person").await;
    Mock::given(matchers::method("GET"))
        .and(matchers::path("/ehr"))
        .and(matchers::query_param("subject_id", UID))
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
async fn under_the_existing_policy_a_patient_create_sends_no_ehr_call()
-> Result<(), Box<dyn StdError>> {
    let (cdr, facade) = harness_with(linked(Policy::Existing)?).await?;
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
async fn a_practitioner_create_sends_no_ehr_call() -> Result<(), Box<dyn StdError>> {
    let (cdr, facade) = harness_with(linked(Policy::CreateOnFirstWrite)?).await?;
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
-> Result<(), Box<dyn StdError>> {
    let (cdr, facade) = harness_with(linked(Policy::CreateOnFirstWrite)?).await?;
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
