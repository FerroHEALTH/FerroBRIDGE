// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! A terminology server that fails while an inbound resource is mapped: the
//! write answers the upstream row, never the caller's fault.

use ferrobridge_server::facade::identity::store::MemoryStore;
use ferrobridge_testkit::stubs::terminology;
use http::StatusCode;
use std::error::Error as StdError;
use std::sync::Arc;
use wiremock::Mock;
use wiremock::MockServer;
use wiremock::ResponseTemplate;
use wiremock::matchers;

use crate::support::translator;

use super::EHR_ID;
use super::FIXTURES;
use super::Harness;
use super::call;
use super::condition;
use super::facade_over;
use super::handle;
use super::post_condition;
use super::settings;
use super::stub::mount_ehr;

/// The concept map the translating copy of the facade model names on the
/// problem name. `example.org` is reserved for examples (RFC 6761 §6.5).
const CONCEPT_MAP: &str = "http://example.org/fhir/ConceptMap/ferrobridge-facade-problems";

/// Writes the facade fixture tree into a temporary directory, its problem
/// name translated through [`CONCEPT_MAP`].
fn translating_fixtures() -> Result<tempfile::TempDir, Box<dyn StdError>> {
    let root = tempfile::tempdir()?;
    for name in [
        "ferrobridge_facade.context.yml",
        "ferrobridge_facade.search.yml",
    ] {
        std::fs::copy(
            std::path::Path::new(FIXTURES).join(name),
            root.path().join(name),
        )?;
    }
    let model =
        std::fs::read_to_string(std::path::Path::new(FIXTURES).join("ferrobridge_facade.yml"))?;
    let problem = "      type: \"CODEABLECONCEPT\"\n";
    assert!(model.contains(problem), "the model maps the problem name");
    let translated = model.replacen(
        problem,
        &format!("{problem}    conceptmap: \"{CONCEPT_MAP}\"\n"),
        1,
    );
    std::fs::write(root.path().join("ferrobridge_facade.yml"), translated)?;
    Ok(root)
}

/// Returns a harness over the translating tree whose `$translate` calls reach
/// `server`.
async fn translating(
    fixtures: &tempfile::TempDir,
    server: &MockServer,
) -> Result<Harness, Box<dyn StdError>> {
    let store = Arc::new(MemoryStore::new());
    let (cdr, facade) = facade_over(
        fixtures.path().to_str().ok_or("a UTF-8 temporary path")?,
        settings(),
        None,
        handle(&store),
        translator(server)?,
    )
    .await;
    Ok(Harness { cdr, store, facade })
}

/// Returns whether the first issue of `outcome` is an error coded `code`
/// whose diagnostics carry every one of `words`.
fn first_names(outcome: &serde_json::Value, code: &str, words: &[&str]) -> bool {
    let issue = &outcome["issue"][0];
    issue["severity"] == "error"
        && issue["code"] == code
        && issue["diagnostics"]
            .as_str()
            .is_some_and(|text| words.iter().all(|word| text.contains(word)))
}

#[tokio::test]
async fn a_terminology_server_failure_refuses_the_write_with_502_and_the_upstream_status()
-> Result<(), Box<dyn StdError>> {
    // An upstream failure is never the caller's fault: the terminology server
    // answering 503 is the facade's UPSTREAM row (R4 http.html, 502 transient).
    let server = MockServer::start().await;
    Mock::given(matchers::method("POST"))
        .and(matchers::path("/r4/ConceptMap/$translate"))
        .respond_with(ResponseTemplate::new(503))
        .mount(&server)
        .await;
    let fixtures = translating_fixtures()?;
    let harness = translating(&fixtures, &server).await?;
    mount_ehr(&harness.cdr).await;

    let (status, outcome) = call(harness.app(), post_condition(&condition())).await?;
    assert_eq!(StatusCode::BAD_GATEWAY, status, "{outcome}");
    assert!(
        first_names(&outcome, "transient", &["503", CONCEPT_MAP]),
        "the refusal names the concept map and the upstream status: {outcome}"
    );
    assert_eq!(
        None,
        harness
            .sent("POST", &format!("/ehr/{EHR_ID}/composition"))
            .await,
        "nothing was committed"
    );
    Ok(())
}

/// Returns a harness whose `$translate` calls reach a terminology server
/// that answers `503`, with the EHR mounted.
async fn failing_terminology() -> Result<(Harness, MockServer, tempfile::TempDir), Box<dyn StdError>>
{
    let server = MockServer::start().await;
    Mock::given(matchers::method("POST"))
        .and(matchers::path("/r4/ConceptMap/$translate"))
        .respond_with(ResponseTemplate::new(503))
        .mount(&server)
        .await;
    let fixtures = translating_fixtures()?;
    let harness = translating(&fixtures, &server).await?;
    mount_ehr(&harness.cdr).await;
    Ok((harness, server, fixtures))
}

#[tokio::test]
async fn validate_answers_502_when_the_terminology_server_fails() -> Result<(), Box<dyn StdError>> {
    // R4 operation-resource-validate.html: the outcome judges the resource,
    // and a failed terminology server says nothing about it.
    let (harness, _server, _fixtures) = failing_terminology().await?;
    let (status, outcome) = call(
        harness.app(),
        http::Request::post("/fhir/Condition/$validate")
            .header(http::header::CONTENT_TYPE, "application/fhir+json")
            .body(axum::body::Body::from(condition().to_string()))?,
    )
    .await?;
    assert_eq!(StatusCode::BAD_GATEWAY, status, "{outcome}");
    assert!(
        first_names(&outcome, "transient", &["503", CONCEPT_MAP]),
        "{outcome}"
    );
    Ok(())
}

#[tokio::test]
async fn a_transaction_answers_502_when_the_terminology_server_fails()
-> Result<(), Box<dyn StdError>> {
    let (harness, _server, _fixtures) = failing_terminology().await?;
    let bundle = serde_json::json!({
        "resourceType": "Bundle",
        "type": "transaction",
        "entry": [{
            "fullUrl": "urn:uuid:7c2e0000-0000-4000-8000-0000000000c1",
            "resource": condition(),
            "request": { "method": "POST", "url": "Condition" }
        }]
    });
    let (status, outcome) = call(
        harness.app(),
        http::Request::post("/fhir")
            .header(http::header::CONTENT_TYPE, "application/fhir+json")
            .body(axum::body::Body::from(bundle.to_string()))?,
    )
    .await?;
    assert_eq!(StatusCode::BAD_GATEWAY, status, "{outcome}");
    assert!(
        first_names(&outcome, "transient", &["503", CONCEPT_MAP]),
        "{outcome}"
    );
    assert!(
        harness
            .sent("POST", &format!("/ehr/{EHR_ID}/contribution"))
            .await
            .is_none(),
        "nothing was committed"
    );
    Ok(())
}

#[tokio::test]
async fn a_refused_translation_refuses_the_write_with_502_and_the_upstream_status()
-> Result<(), Box<dyn StdError>> {
    let server = MockServer::start().await;
    Mock::given(matchers::method("POST"))
        .and(matchers::path("/r4/ConceptMap/$translate"))
        .respond_with(terminology::outcome(
            404,
            "not-found",
            Some("not-found"),
            "no ConceptMap with that canonical",
        ))
        .mount(&server)
        .await;
    let fixtures = translating_fixtures()?;
    let harness = translating(&fixtures, &server).await?;
    mount_ehr(&harness.cdr).await;

    let (status, outcome) = call(harness.app(), post_condition(&condition())).await?;
    assert_eq!(StatusCode::BAD_GATEWAY, status, "{outcome}");
    assert!(
        first_names(&outcome, "transient", &["404", CONCEPT_MAP]),
        "{outcome}"
    );
    Ok(())
}

#[tokio::test]
async fn an_untranslated_required_code_stays_the_callers_422() -> Result<(), Box<dyn StdError>> {
    // A translation the server answers with no match is an outcome, never an
    // upstream failure, so the refusal stays the mapping's own 422.
    let server = MockServer::start().await;
    Mock::given(matchers::method("POST"))
        .and(matchers::path("/r4/ConceptMap/$translate"))
        .respond_with(terminology::translate_no_match(
            "no translation for that code",
        ))
        .mount(&server)
        .await;
    let fixtures = translating_fixtures()?;
    let harness = translating(&fixtures, &server).await?;
    mount_ehr(&harness.cdr).await;

    let (status, outcome) = call(harness.app(), post_condition(&condition())).await?;
    assert_eq!(StatusCode::UNPROCESSABLE_ENTITY, status, "{outcome}");
    assert!(first_names(&outcome, "processing", &[]), "{outcome}");
    Ok(())
}
