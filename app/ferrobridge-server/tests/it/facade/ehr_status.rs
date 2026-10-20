// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The subject a run offers, written into the EHR's `EHR_STATUS` before the
//! composition: `ehr_status_get_at_time`, then `ehr_status_update` with
//! `If-Match` on the version read (`ehr-codegen.openapi.yaml`), and every
//! refusal before anything is committed.

use std::error::Error as StdError;

use ferrobridge_server::facade::Settings;
use http::StatusCode;
use http::header;
use wiremock::Mock;
use wiremock::MockServer;
use wiremock::ResponseTemplate;
use wiremock::matchers;

use super::EHR_ID;
use super::VERSION_ONE;
use super::call;
use super::condition;
use super::harness_over;
use super::post_condition;
use super::post_transaction;
use super::raw;
use super::settings;
use super::stub::mount_create;
use super::stub::mount_ehr;
use super::transaction_of;

/// The mapping set whose model offers every `Condition.identifier`.
const FIXTURES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/it/subject_ref");

/// The system of the synthetic Condition's identifier.
const SYSTEM: &str = "http://example.org/fhir/sid/ferrobridge-condition";

/// The value of the synthetic Condition's identifier.
const VALUE: &str = "synthetic-condition-0001";

/// The `EHR_STATUS` version the stub CDR holds.
const STATUS_VERSION: &str = "0a1b2c3d-0000-4000-8000-00000000000a::ferrobridge.test::1";

/// The `EHR_STATUS` path of the case's EHR.
fn status_path() -> String {
    format!("/ehr/{EHR_ID}/ehr_status")
}

/// Returns the facade settings with the identifier system configured.
fn configured() -> Settings {
    Settings {
        ehr_status_system: Some(String::from(SYSTEM)),
        ..settings()
    }
}

/// Returns an `EHR_STATUS` whose subject holds `external_ref`.
fn status_body(external_ref: Option<serde_json::Value>) -> serde_json::Value {
    let mut subject = serde_json::json!({ "_type": "PARTY_SELF" });
    if let (Some(object), Some(reference)) = (subject.as_object_mut(), external_ref) {
        object.insert(String::from("external_ref"), reference);
    }
    serde_json::json!({
        "_type": "EHR_STATUS",
        "archetype_node_id": "openEHR-EHR-EHR_STATUS.generic.v1",
        "name": { "_type": "DV_TEXT", "value": "EHR Status" },
        "uid": { "_type": "OBJECT_VERSION_ID", "value": STATUS_VERSION },
        "archetype_details": {
            "_type": "ARCHETYPED",
            "archetype_id": { "_type": "ARCHETYPE_ID", "value": "openEHR-EHR-EHR_STATUS.generic.v1" },
            "rm_version": "1.1.0"
        },
        "subject": subject,
        "is_queryable": true,
        "is_modifiable": true
    })
}

/// Returns the `PARTY_REF` the synthetic identifier makes.
fn reference(value: &str) -> serde_json::Value {
    serde_json::json!({
        "_type": "PARTY_REF",
        "namespace": SYSTEM,
        "type": "PERSON",
        "id": { "_type": "GENERIC_ID", "value": value, "scheme": SYSTEM }
    })
}

/// Mounts the `EHR_STATUS` read answering `200` with `body`.
async fn mount_status(cdr: &MockServer, body: &serde_json::Value) {
    Mock::given(matchers::method("GET"))
        .and(matchers::path(status_path()))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("ETag", format!("\"{STATUS_VERSION}\""))
                .insert_header("Content-Type", "application/json")
                .set_body_string(body.to_string()),
        )
        .mount(cdr)
        .await;
}

/// Mounts the `EHR_STATUS` update answering `response`.
async fn mount_update(cdr: &MockServer, response: ResponseTemplate) {
    Mock::given(matchers::method("PUT"))
        .and(matchers::path(status_path()))
        .respond_with(response)
        .mount(cdr)
        .await;
}

#[tokio::test]
async fn an_anonymous_ehr_status_takes_the_subject_before_the_composition()
-> Result<(), Box<dyn StdError>> {
    let harness = harness_over(FIXTURES, configured()).await;
    mount_ehr(&harness.cdr).await;
    mount_status(&harness.cdr, &status_body(None)).await;
    mount_update(
        &harness.cdr,
        ResponseTemplate::new(204).insert_header(
            "ETag",
            "\"0a1b2c3d-0000-4000-8000-00000000000a::ferrobridge.test::2\"",
        ),
    )
    .await;
    mount_create(&harness.cdr, VERSION_ONE, &harness.composition()).await;
    let response = raw(harness.app(), post_condition(&condition())).await?;
    assert_eq!(StatusCode::CREATED, response.status());
    let sent = harness
        .sent("PUT", &status_path())
        .await
        .ok_or("the EHR_STATUS is written")?;
    assert_eq!(
        sent.pointer("/subject/external_ref"),
        Some(&reference(VALUE)),
        "{sent}"
    );
    let requests = harness.cdr.received_requests().await.unwrap_or_default();
    let put = requests
        .iter()
        .position(|request| request.method.as_str() == "PUT")
        .ok_or("a PUT was sent")?;
    let post = requests
        .iter()
        .position(|request| request.method.as_str() == "POST")
        .ok_or("a POST was sent")?;
    assert!(
        put < post,
        "the EHR_STATUS is written before the composition"
    );
    let if_match = requests
        .get(put)
        .and_then(|request| request.headers.get(header::IF_MATCH))
        .and_then(|value| value.to_str().ok())
        .ok_or("the update carries If-Match")?;
    assert_eq!(format!("\"{STATUS_VERSION}\""), if_match);
    assert!(
        requests
            .get(put)
            .is_some_and(|request| request.headers.contains_key("openehr-audit-details")),
        "the update states its committer"
    );
    Ok(())
}

#[tokio::test]
async fn an_ehr_status_that_names_the_subject_takes_no_write() -> Result<(), Box<dyn StdError>> {
    let harness = harness_over(FIXTURES, configured()).await;
    mount_ehr(&harness.cdr).await;
    mount_status(&harness.cdr, &status_body(Some(reference(VALUE)))).await;
    mount_create(&harness.cdr, VERSION_ONE, &harness.composition()).await;
    let response = raw(harness.app(), post_condition(&condition())).await?;
    assert_eq!(StatusCode::CREATED, response.status());
    assert_eq!(0, harness.received("PUT", &status_path()).await);
    Ok(())
}

#[tokio::test]
async fn an_ehr_status_that_names_another_subject_is_refused_with_nothing_committed()
-> Result<(), Box<dyn StdError>> {
    let harness = harness_over(FIXTURES, configured()).await;
    mount_ehr(&harness.cdr).await;
    mount_status(
        &harness.cdr,
        &status_body(Some(reference("synthetic-other-subject"))),
    )
    .await;
    let (status, body) = call(harness.app(), post_condition(&condition())).await?;
    assert_eq!(StatusCode::CONFLICT, status, "{body}");
    assert_eq!(0, harness.received("PUT", &status_path()).await);
    assert_eq!(
        0,
        harness
            .received("POST", &format!("/ehr/{EHR_ID}/composition"))
            .await
    );
    Ok(())
}

#[tokio::test]
async fn a_version_conflict_answers_four_hundred_and_twelve_with_the_latest_version()
-> Result<(), Box<dyn StdError>> {
    let harness = harness_over(FIXTURES, configured()).await;
    mount_ehr(&harness.cdr).await;
    mount_status(&harness.cdr, &status_body(None)).await;
    mount_update(
        &harness.cdr,
        ResponseTemplate::new(412)
            .insert_header(
                "ETag",
                "\"0a1b2c3d-0000-4000-8000-00000000000a::ferrobridge.test::3\"",
            )
            .set_body_string(r#"{"message":"synthetic version conflict"}"#),
    )
    .await;
    let response = raw(harness.app(), post_condition(&condition())).await?;
    assert_eq!(StatusCode::PRECONDITION_FAILED, response.status());
    assert!(
        response.headers().contains_key(header::ETAG),
        "the 412 names the latest version"
    );
    assert_eq!(
        0,
        harness
            .received("POST", &format!("/ehr/{EHR_ID}/composition"))
            .await,
        "nothing is committed after a refused EHR_STATUS write"
    );
    Ok(())
}

#[tokio::test]
async fn a_refused_update_keeps_the_upstream_status_in_the_diagnostics()
-> Result<(), Box<dyn StdError>> {
    let harness = harness_over(FIXTURES, configured()).await;
    mount_ehr(&harness.cdr).await;
    mount_status(&harness.cdr, &status_body(None)).await;
    mount_update(
        &harness.cdr,
        ResponseTemplate::new(400).set_body_string(r#"{"message":"synthetic refusal"}"#),
    )
    .await;
    let (status, body) = call(harness.app(), post_condition(&condition())).await?;
    assert!(!status.is_success(), "{status} {body}");
    assert!(body.to_string().contains("400"), "{body}");
    assert_eq!(
        0,
        harness
            .received("POST", &format!("/ehr/{EHR_ID}/composition"))
            .await
    );
    Ok(())
}

#[tokio::test]
async fn an_unknown_ehr_status_is_refused_as_not_found() -> Result<(), Box<dyn StdError>> {
    let harness = harness_over(FIXTURES, configured()).await;
    mount_ehr(&harness.cdr).await;
    Mock::given(matchers::method("GET"))
        .and(matchers::path(status_path()))
        .respond_with(
            ResponseTemplate::new(404).set_body_string(r#"{"message":"synthetic unknown"}"#),
        )
        .mount(&harness.cdr)
        .await;
    let (status, body) = call(harness.app(), post_condition(&condition())).await?;
    assert_eq!(StatusCode::NOT_FOUND, status, "{body}");
    Ok(())
}

#[tokio::test]
async fn an_offered_subject_with_no_system_configured_is_refused() -> Result<(), Box<dyn StdError>>
{
    let harness = harness_over(FIXTURES, settings()).await;
    mount_ehr(&harness.cdr).await;
    let (status, body) = call(harness.app(), post_condition(&condition())).await?;
    assert_eq!(StatusCode::UNPROCESSABLE_ENTITY, status, "{body}");
    assert!(
        body.to_string()
            .contains("facade.ehr_status_identifier_system"),
        "{body}"
    );
    assert_eq!(0, harness.received("GET", &status_path()).await);
    Ok(())
}

#[tokio::test]
async fn an_identifier_of_another_system_writes_nothing() -> Result<(), Box<dyn StdError>> {
    let harness = harness_over(
        FIXTURES,
        Settings {
            ehr_status_system: Some(String::from("urn:oid:2.999.7")),
            ..settings()
        },
    )
    .await;
    mount_ehr(&harness.cdr).await;
    mount_create(&harness.cdr, VERSION_ONE, &harness.composition()).await;
    let response = raw(harness.app(), post_condition(&condition())).await?;
    assert_eq!(StatusCode::CREATED, response.status());
    assert_eq!(0, harness.received("GET", &status_path()).await);
    Ok(())
}

#[tokio::test]
async fn a_transaction_writes_the_subject_before_its_contribution() -> Result<(), Box<dyn StdError>>
{
    let harness = harness_over(FIXTURES, configured()).await;
    mount_ehr(&harness.cdr).await;
    mount_status(
        &harness.cdr,
        &status_body(Some(reference("synthetic-other-subject"))),
    )
    .await;
    let bundle = transaction_of(&[("urn:uuid:7a1f3b0e-0000-4000-8000-000000000001", condition())]);
    let (status, body) = post_transaction(&harness, &bundle).await?;
    assert_eq!(StatusCode::CONFLICT, status, "{body}");
    assert_eq!(
        0,
        harness
            .received("POST", &format!("/ehr/{EHR_ID}/contribution"))
            .await,
        "no contribution is committed after a refused EHR_STATUS write"
    );
    Ok(())
}
