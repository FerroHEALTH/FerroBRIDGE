// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The `$validate` dry run, which writes nothing.

use axum::body::Body;
use http::Request;
use http::StatusCode;
use http::header;
use std::error::Error;

use fhirconnect::operations::issues::OPENEHR_PATH_SYSTEM;

use super::EHR_ID;
use super::ONE_WAY_FIXTURES;
use super::VALIDATE;
use super::assert_nothing_written;
use super::call;
use super::condition;
use super::create_one;
use super::diagnostics_of;
use super::first_issue;
use super::harness;
use super::harness_over;
use super::settings;

#[tokio::test]
async fn validate_answers_two_hundred_and_commits_nothing() -> Result<(), Box<dyn Error>> {
    let harness = harness().await;
    let request = Request::post(VALIDATE)
        .header(header::CONTENT_TYPE, "application/fhir+json")
        .body(Body::from(condition().to_string()))?;
    let (status, body) = call(harness.app(), request).await?;
    assert_eq!(StatusCode::OK, status, "{body}");
    assert_eq!(
        Some("information"),
        first_issue(&body)["severity"].as_str(),
        "a resource that would commit reports information: {body}"
    );
    assert_eq!(
        0,
        harness.received("POST", &format!("/ehr/{EHR_ID}")).await,
        "the dry run committed something"
    );
    assert_eq!(
        0,
        harness.received("GET", "/ehr").await,
        "the dry run resolved an EHR it never writes into"
    );
    Ok(())
}

#[tokio::test]
async fn validate_reports_the_verdict_of_a_resource_that_would_not_commit()
-> Result<(), Box<dyn Error>> {
    let harness = harness().await;
    let mut resource = condition();
    resource["subject"] = serde_json::json!({ "display": "no identifier and no reference" });
    let request = Request::post(VALIDATE)
        .header(header::CONTENT_TYPE, "application/fhir+json")
        .body(Body::from(resource.to_string()))?;
    let (status, body) = call(harness.app(), request).await?;
    assert_eq!(StatusCode::OK, status, "both verdicts answer 200: {body}");
    let issue = first_issue(&body);
    assert_eq!(Some("error"), issue["severity"].as_str(), "{body}");
    assert!(
        issue["details"]["text"]
            .as_str()
            .is_some_and(|text| text.contains("documents no composition-validation route")),
        "the outcome says which validator ran: {body}"
    );
    Ok(())
}

#[tokio::test]
async fn validate_mirrors_the_create_paths_operation_level_statuses() -> Result<(), Box<dyn Error>>
{
    let harness = harness().await;
    let mut resource = condition();
    resource["meta"] = serde_json::json!({ "profile": ["http://example.org/other"] });
    let request = Request::post(VALIDATE)
        .header(header::CONTENT_TYPE, "application/fhir+json")
        .body(Body::from(resource.to_string()))?;
    let (status, body) = call(harness.app(), request).await?;
    assert_eq!(
        StatusCode::UNPROCESSABLE_ENTITY,
        status,
        "no program is the create path's 422 here too: {body}"
    );
    let (unsupported, _) = call(
        harness.app(),
        Request::post("/fhir/Observation/$validate")
            .header(header::CONTENT_TYPE, "application/fhir+json")
            .body(Body::from(condition().to_string()))?,
    )
    .await?;
    assert_eq!(StatusCode::NOT_FOUND, unsupported);
    Ok(())
}

/// Returns `error` and every cause behind it as one line, the way the facade
/// renders a refusal into `issue.diagnostics`.
fn chain_of(error: &dyn Error) -> String {
    let mut line = error.to_string();
    let mut cause = error.source();
    while let Some(source) = cause {
        line.push_str(": ");
        line.push_str(&source.to_string());
        cause = source.source();
    }
    line
}

/// Returns the synthetic Condition with no diagnosis name, which the
/// synthetic template requires.
fn condition_without_code() -> serde_json::Value {
    let mut resource = condition();
    if let Some(object) = resource.as_object_mut() {
        object.remove("code");
    }
    resource
}

#[tokio::test]
async fn validate_names_the_template_and_the_ehr_disposition() -> Result<(), Box<dyn Error>> {
    // The carry-over case of #115: `information` issues naming what would be
    // committed and where.
    let harness = harness().await;
    let request = Request::post(VALIDATE)
        .header(header::CONTENT_TYPE, "application/fhir+json")
        .body(Body::from(condition().to_string()))?;
    let (status, body) = call(harness.app(), request).await?;
    assert_eq!(StatusCode::OK, status, "{body}");
    let information: Vec<&serde_json::Value> = body["issue"]
        .as_array()
        .ok_or("the outcome carries issues")?
        .iter()
        .filter(|issue| issue["severity"].as_str() == Some("information"))
        .collect();
    assert!(
        information.iter().any(|issue| issue["diagnostics"]
            .as_str()
            .is_some_and(|text| text.contains("ferrobridge.diagnoses.v1"))),
        "an information issue names the template: {body}"
    );
    assert!(
        information.iter().any(|issue| issue["diagnostics"]
            .as_str()
            .is_some_and(|text| text.contains("synthetic-subject-0001")
                && text.contains("writes into an existing EHR only"))),
        "an information issue names the subject and the EHR policy: {body}"
    );
    assert_nothing_written(&harness).await;
    assert_eq!(
        0,
        harness.received("GET", "/ehr").await,
        "the dry run asked the CDR for an EHR"
    );
    Ok(())
}

#[tokio::test]
async fn validate_names_the_ehr_the_identity_map_already_holds() -> Result<(), Box<dyn Error>> {
    let harness = harness().await;
    create_one(&harness).await?;
    let before = harness.received("POST", "/ehr").await;
    let request = Request::post(VALIDATE)
        .header(header::CONTENT_TYPE, "application/fhir+json")
        .body(Body::from(condition().to_string()))?;
    let (status, body) = call(harness.app(), request).await?;
    assert_eq!(StatusCode::OK, status, "{body}");
    assert!(
        diagnostics_of(&body)
            .iter()
            .any(|text| text.contains(EHR_ID)),
        "the disposition names the EHR the map holds: {body}"
    );
    assert_eq!(
        before,
        harness.received("POST", "/ehr").await,
        "the dry run wrote into the CDR"
    );
    Ok(())
}

#[tokio::test]
async fn validate_carries_the_validators_message_verbatim_and_writes_nothing()
-> Result<(), Box<dyn Error>> {
    let harness = harness().await;
    let resource = condition_without_code();
    let loaded = harness
        .facade
        .programs()
        .by_context("ferrobridge_facade.context")
        .cloned()
        .ok_or("the fixture context compiled")?;
    let document: fhir_types::codec::Value = serde_json::from_value(resource.clone())?;
    let refusal = ferrobridge_server::facade::engine::inbound(
        loaded.program(),
        loaded.index(),
        &document,
        "2026-09-15T09:00:00Z",
        &settings(),
        &fhirconnect::engine::terminology::NoTerminology,
        &fhirconnect::engine::demographics::IssuerDemographics::new(crate::facade::BASE_URL)
            .expect("the test base issues identities"),
    )
    .err()
    .ok_or("the synthetic template requires the diagnosis name")?;
    let request = Request::post(VALIDATE)
        .header(header::CONTENT_TYPE, "application/fhir+json")
        .body(Body::from(resource.to_string()))?;
    let (status, body) = call(harness.app(), request).await?;
    assert_eq!(StatusCode::OK, status, "both verdicts answer 200: {body}");
    let issue = first_issue(&body);
    assert_eq!(Some("error"), issue["severity"].as_str(), "{body}");
    assert_eq!(
        Some(chain_of(&refusal).as_str()),
        issue["diagnostics"].as_str(),
        "the validator's message travels verbatim: {body}"
    );
    assert_nothing_written(&harness).await;
    assert_eq!(0, harness.received("GET", "/ehr").await);
    Ok(())
}

#[tokio::test]
async fn validate_refuses_at_the_operation_level_as_create_does() -> Result<(), Box<dyn Error>> {
    let harness = harness().await;
    let mut unknown_member = condition();
    unknown_member["notAnElement"] = serde_json::json!("x");
    let mut other_profile = condition();
    other_profile["meta"] = serde_json::json!({ "profile": ["http://example.org/other"] });
    for (media, body) in [
        ("application/fhir+xml", String::from("<Condition/>")),
        ("application/fhir+json", unknown_member.to_string()),
        ("application/fhir+json", other_profile.to_string()),
        ("application/fhir+json", String::from("{")),
    ] {
        let created = call(
            harness.app(),
            Request::post("/fhir/Condition")
                .header(header::CONTENT_TYPE, media)
                .body(Body::from(body.clone()))?,
        )
        .await?;
        let validated = call(
            harness.app(),
            Request::post(VALIDATE)
                .header(header::CONTENT_TYPE, media)
                .body(Body::from(body.clone()))?,
        )
        .await?;
        assert_eq!(
            created.0, validated.0,
            "create and $validate answer the same status for {body}"
        );
        assert_eq!(
            first_issue(&created.1)["code"],
            first_issue(&validated.1)["code"],
            "create and $validate refuse with the same issue code for {body}"
        );
    }
    assert_nothing_written(&harness).await;
    Ok(())
}

/// Returns the issues of a dry-run outcome whose `details.text` contains
/// `text`.
fn issues_naming<'a>(body: &'a serde_json::Value, text: &str) -> Vec<&'a serde_json::Value> {
    body["issue"]
        .as_array()
        .map(|issues| {
            issues
                .iter()
                .filter(|issue| {
                    issue["details"]["text"]
                        .as_str()
                        .is_some_and(|found| found.contains(text))
                })
                .collect()
        })
        .unwrap_or_default()
}

#[tokio::test]
async fn validate_locates_a_loss_by_its_fhirpath_and_its_openehr_path() -> Result<(), Box<dyn Error>>
{
    // `engine/rest-api.adoc` §Reporting partial results with `OperationOutcome`:
    // the gap is explicit, and R4 `OperationOutcome.issue.expression` is where
    // the FHIRPath of the element it concerns goes.
    let harness = harness_over(ONE_WAY_FIXTURES, settings()).await;
    let request = Request::post(VALIDATE)
        .header(header::CONTENT_TYPE, "application/fhir+json")
        .body(Body::from(condition().to_string()))?;
    let (status, body) = call(harness.app(), request).await?;
    assert_eq!(StatusCode::OK, status, "{body}");
    let skipped = issues_naming(&body, "the mapping comment did not run");
    let issue = skipped
        .first()
        .ok_or("the one-way mapping is a declared loss")?;
    assert_eq!(Some("warning"), issue["severity"].as_str(), "{body}");
    assert_eq!(Some("incomplete"), issue["code"].as_str(), "{body}");
    assert_eq!(
        serde_json::json!(["Condition.note.text"]),
        issue["expression"],
        "the loss carries its FHIRPath: {body}"
    );
    assert_eq!(
        Some(OPENEHR_PATH_SYSTEM),
        issue["details"]["coding"][0]["system"].as_str(),
        "the loss carries its openEHR path: {body}"
    );
    assert!(
        issue["details"]["coding"][0]["code"]
            .as_str()
            .is_some_and(|path| path.ends_with("/data[at0001]/items[at0069]/value")),
        "{body}"
    );
    let defaulted = issues_naming(&body, "the engine filled /composer");
    let issue = defaulted
        .first()
        .ok_or("a defaulted field is a declared loss")?;
    assert_eq!(Some("information"), issue["severity"].as_str(), "{body}");
    assert_eq!(
        serde_json::Value::Null,
        issue["expression"],
        "a defaulted composition field has no FHIR element: {body}"
    );
    assert_eq!(
        Some("/composer"),
        issue["details"]["coding"][0]["code"].as_str(),
        "{body}"
    );
    assert_nothing_written(&harness).await;
    Ok(())
}
