// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The `EHR_STATUS` contract over the generated client: one case per status
//! `docs/specs/its-rest/computable/OAS/ehr-codegen.openapi.yaml` documents for
//! `ehr_status_get_at_time` and `ehr_status_update`, plus the `If-Match` and
//! the commit headers the update carries.

use std::error::Error;

use ferrobridge_server::cdr::Prefer;
use ferrobridge_server::cdr::ids::EhrId;
use ferrobridge_server::facade::ehr::status_of;
use ferrobridge_server::facade::identity::PersonId;
use ferrobridge_server::facade::subject::IdForm;
use ferrobridge_server::facade::subject::SubjectLink;
use openehr_base::v1_3::base_types::identification::object_version_id::ObjectVersionId;
use openehr_its::rest::generated::ehr::client::EhrStatusGetAtTimeOutcome;
use openehr_its::rest::generated::ehr::client::EhrStatusUpdateOutcome;
use openehr_rm::v1_2::ehr::ehr_status::EhrStatus;
use wiremock::Mock;
use wiremock::MockServer;
use wiremock::ResponseTemplate;
use wiremock::matchers::method;
use wiremock::matchers::path;

use super::support;

/// The synthetic EHR.
const EHR: &str = "7d44b88c-4199-4bad-97dc-d78268e01398";

/// The `EHR_STATUS` version the stub holds.
const VERSION_1: &str = "0a1b2c3d-0000-4000-8000-00000000000a::ferrobridge.test::1";

/// The version a write produces.
const VERSION_2: &str = "0a1b2c3d-0000-4000-8000-00000000000a::ferrobridge.test::2";

/// The path of the synthetic EHR's `EHR_STATUS`.
fn status_path() -> String {
    format!("/v1/ehr/{EHR}/ehr_status")
}

/// Returns a synthetic `EHR_STATUS`.
fn status() -> Result<EhrStatus, Box<dyn Error>> {
    let link = SubjectLink::new("http://example.org/fhir", IdForm::Reference, "PERSON")?;
    let person = PersonId::new("urn:oid:2.999.1", "synthetic-pseudonym-1")?;
    Ok(status_of(&person, &link))
}

/// Returns the canonical JSON of [`status`].
fn status_json() -> Result<String, Box<dyn Error>> {
    Ok(serde_json::to_string(
        &openehr_its::json::to_canonical_value(&status()?),
    )?)
}

/// Mounts the update answering `response`.
async fn mount_update(server: &MockServer, response: ResponseTemplate) {
    Mock::given(method("PUT"))
        .and(path(status_path()))
        .respond_with(response)
        .mount(server)
        .await;
}

/// Sends the update with `VERSION_1` as the preceding version.
async fn update(server: &MockServer) -> Result<EhrStatusUpdateOutcome, Box<dyn Error>> {
    Ok(support::client(server)?
        .update_ehr_status(
            &EhrId::new(EHR)?,
            &ObjectVersionId::new(VERSION_1)?,
            &status()?,
            &support::commit_context()?,
            Prefer::Minimal,
        )
        .await?
        .outcome)
}

#[tokio::test]
async fn the_read_answers_two_hundred_with_the_version_in_its_etag() -> Result<(), Box<dyn Error>> {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(status_path()))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("ETag", format!("\"{VERSION_1}\""))
                .insert_header("Content-Type", "application/json")
                .set_body_string(status_json()?),
        )
        .mount(&server)
        .await;
    let answered = support::client(&server)?
        .ehr_status(&EhrId::new(EHR)?)
        .await?;
    match answered.outcome {
        EhrStatusGetAtTimeOutcome::Ok { body, headers } => {
            let version = ferrobridge_server::cdr::version_from_etag(
                "ehr_status_get_at_time",
                http::StatusCode::OK,
                headers.etag.as_deref(),
            )?;
            assert_eq!(VERSION_1, version.value());
            assert!(body.subject.external_ref.is_some());
        }
        other => return Err(format!("expected an EHR_STATUS, got {other:?}").into()),
    }
    Ok(())
}

#[tokio::test]
async fn the_read_reports_a_four_hundred_and_a_four_hundred_and_four() -> Result<(), Box<dyn Error>>
{
    for (status, wanted) in [(400, "BadRequest"), (404, "NotFound")] {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path(status_path()))
            .respond_with(
                ResponseTemplate::new(status).set_body_string(r#"{"message":"synthetic"}"#),
            )
            .mount(&server)
            .await;
        let answered = support::client(&server)?
            .ehr_status(&EhrId::new(EHR)?)
            .await?;
        let found = match answered.outcome {
            EhrStatusGetAtTimeOutcome::BadRequest { .. } => "BadRequest",
            EhrStatusGetAtTimeOutcome::NotFound { .. } => "NotFound",
            EhrStatusGetAtTimeOutcome::Ok { .. } => "Ok",
        };
        assert_eq!(wanted, found, "{status}");
    }
    Ok(())
}

#[tokio::test]
async fn the_update_sends_if_match_and_the_commit_headers() -> Result<(), Box<dyn Error>> {
    let server = MockServer::start().await;
    mount_update(
        &server,
        ResponseTemplate::new(204).insert_header("ETag", format!("\"{VERSION_2}\"")),
    )
    .await;
    let outcome = update(&server).await?;
    assert!(
        matches!(outcome, EhrStatusUpdateOutcome::NoContent { .. }),
        "{outcome:?}"
    );
    assert_eq!(
        vec![format!("\"{VERSION_1}\"")],
        support::request_header(&server, 0, "if-match").await?
    );
    assert_eq!(
        vec![String::from("return=minimal")],
        support::request_header(&server, 0, "prefer").await?
    );
    assert!(
        !support::request_header(&server, 0, "openehr-audit-details")
            .await?
            .is_empty(),
        "the update states its committer"
    );
    Ok(())
}

#[tokio::test]
async fn the_update_reads_a_two_hundred_with_the_new_status() -> Result<(), Box<dyn Error>> {
    let server = MockServer::start().await;
    mount_update(
        &server,
        ResponseTemplate::new(200)
            .insert_header("ETag", format!("\"{VERSION_2}\""))
            .insert_header("Content-Type", "application/json")
            .set_body_string(status_json()?),
    )
    .await;
    let outcome = update(&server).await?;
    assert!(
        matches!(outcome, EhrStatusUpdateOutcome::Ok { .. }),
        "{outcome:?}"
    );
    Ok(())
}

#[tokio::test]
async fn the_update_reports_the_412_with_the_latest_version() -> Result<(), Box<dyn Error>> {
    let server = MockServer::start().await;
    mount_update(
        &server,
        ResponseTemplate::new(412).insert_header("ETag", format!("\"{VERSION_2}\"")),
    )
    .await;
    match update(&server).await? {
        EhrStatusUpdateOutcome::PreconditionFailed { headers, .. } => {
            let latest = ferrobridge_server::cdr::optional_version_from_etag(
                "ehr_status_update",
                headers.etag.as_deref(),
            )?;
            assert_eq!(
                Some(VERSION_2.to_owned()),
                latest.map(|id| id.value().to_owned())
            );
        }
        other => return Err(format!("expected a 412, got {other:?}").into()),
    }
    Ok(())
}

#[tokio::test]
async fn the_update_reports_a_four_hundred_and_a_four_hundred_and_four()
-> Result<(), Box<dyn Error>> {
    for (status, wanted) in [(400, "BadRequest"), (404, "NotFound")] {
        let server = MockServer::start().await;
        mount_update(
            &server,
            ResponseTemplate::new(status).set_body_string(r#"{"message":"synthetic"}"#),
        )
        .await;
        let found = match update(&server).await? {
            EhrStatusUpdateOutcome::BadRequest { .. } => "BadRequest",
            EhrStatusUpdateOutcome::NotFound { .. } => "NotFound",
            EhrStatusUpdateOutcome::Ok { .. } => "Ok",
            EhrStatusUpdateOutcome::NoContent { .. } => "NoContent",
            EhrStatusUpdateOutcome::PreconditionFailed { .. } => "PreconditionFailed",
        };
        assert_eq!(wanted, found, "{status}");
    }
    Ok(())
}
