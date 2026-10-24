// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The display of a coding that carries none, resolved through `$lookup` on a
//! `wiremock` terminology server going into openEHR.
//!
//! "We recommend to resolve the code display using a terminology server when
//! mapping to openEHR, since the value is mandatory in openEHR. Alternatively
//! the coding.code can be used if a term server is not an option"
//! (FHIRconnect `types-of-mappings/data-type/DV_CODED_TEXT.adoc`).

use std::error::Error;

use axum::body::Body;
use ferrobridge_server::terminology::Translator;
use ferrobridge_testkit::stubs::terminology;
use http::{Request, StatusCode, header};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer};

use crate::support::translator;

use super::{FHIR_JSON, PROFILE, app_with, call, composition_text, mapping_tree_with};

/// The code system of the undisplayed verification status. `example.org`
/// is reserved for examples (RFC 6761 §6.5).
const SYSTEM: &str = "http://example.org/fhir/CodeSystem/synthetic-certainty";

/// The code of the undisplayed verification status.
const CODE: &str = "CERT-7";

/// The display the stub server knows for [`CODE`].
const DISPLAY: &str = "Synthetic certainty seven";

/// The synthetic model mapping: the problem name from the text, and the
/// verification status into the coded certainty.
const MODEL_FILE: &str = r#"grammar: FHIRConnect/v1.0.0
type: model
metadata:
  name: ferrobridge_server_diagnose
  version: 1.0.0
spec:
  system: FHIR
  version: R4
  openEhrConfig:
    archetype: openEHR-EHR-EVALUATION.problem_diagnosis.v1
  fhirConfig:
    structureDefinition: http://hl7.org/fhir/StructureDefinition/Condition

mappings:
  - name: "problemName"
    with:
      fhir: "$resource.code.text"
      openehr: "$archetype/data[at0001]/items[at0002]"

  - name: "certainty"
    with:
      fhir: "$resource.verificationStatus"
      openehr: "$archetype/data[at0001]/items[at0073]"
"#;

/// Returns a Bundle with one Condition whose verification status carries a
/// coding with no display.
fn bundle() -> String {
    serde_json::json!({
        "resourceType": "Bundle",
        "type": "collection",
        "entry": [{"resource": {
            "resourceType": "Condition",
            "meta": {"profile": [PROFILE]},
            "code": {"text": "Synthetic problem one"},
            "subject": {"reference": "Patient/synthetic-subject-0001"},
            "verificationStatus": {"coding": [{"system": SYSTEM, "code": CODE}]}
        }}]
    })
    .to_string()
}

/// Maps the Bundle through `$toopenehr` over `translator`.
async fn mapped(translator: Translator) -> Result<(StatusCode, String), Box<dyn Error>> {
    let root = mapping_tree_with(MODEL_FILE)?;
    let (status, _, body) = call(
        app_with(&root, translator)?,
        Request::post("/fhir/$toopenehr")
            .header(header::CONTENT_TYPE, FHIR_JSON)
            .body(Body::from(bundle()))?,
    )
    .await?;
    Ok((status, body))
}

/// Mounts the stub's `$lookup` answer.
async fn mount_lookup(server: &MockServer, answer: wiremock::ResponseTemplate) {
    Mock::given(method("POST"))
        .and(path("/r4/CodeSystem/$lookup"))
        .respond_with(answer)
        .mount(server)
        .await;
}

#[tokio::test]
async fn a_display_the_server_knows_becomes_the_coded_value() -> Result<(), Box<dyn Error>> {
    let server = MockServer::start().await;
    mount_lookup(
        &server,
        terminology::lookup("Synthetic certainty", DISPLAY, None),
    )
    .await;
    let (status, body) = mapped(translator(&server)?).await?;
    assert_eq!(StatusCode::OK, status, "{body}");
    let answer: serde_json::Value = serde_json::from_str(&body)?;
    let built = composition_text(&answer)?;
    assert!(built.contains(DISPLAY), "{built}");
    let calls = server
        .received_requests()
        .await
        .ok_or("the mock server records requests")?
        .len();
    assert_eq!(1, calls, "the display is looked up once");
    Ok(())
}

#[tokio::test]
async fn a_code_the_server_does_not_know_refuses_by_default() -> Result<(), Box<dyn Error>> {
    let server = MockServer::start().await;
    mount_lookup(
        &server,
        terminology::outcome(404, "not-found", Some("not-found"), "no such code"),
    )
    .await;
    let (status, body) = mapped(translator(&server)?).await?;
    assert_ne!(StatusCode::OK, status, "{body}");
    assert!(body.contains(CODE), "the refusal names the code: {body}");
    Ok(())
}

#[tokio::test]
async fn a_code_the_server_does_not_know_keeps_its_code_when_the_fallback_is_configured()
-> Result<(), Box<dyn Error>> {
    let server = MockServer::start().await;
    mount_lookup(
        &server,
        terminology::outcome(404, "not-found", Some("not-found"), "no such code"),
    )
    .await;
    let (status, body) = mapped(translator(&server)?.with_display_fallback(true)).await?;
    assert_eq!(StatusCode::OK, status, "{body}");
    let answer: serde_json::Value = serde_json::from_str(&body)?;
    let built = composition_text(&answer)?;
    assert!(built.contains(&format!("\"value\":\"{CODE}\"")), "{built}");
    Ok(())
}

#[tokio::test]
async fn a_lookup_the_server_refuses_refuses_the_run_with_the_upstream_status()
-> Result<(), Box<dyn Error>> {
    let server = MockServer::start().await;
    mount_lookup(
        &server,
        terminology::outcome(503, "transient", None, "the server is down"),
    )
    .await;
    let (status, body) = mapped(translator(&server)?.with_display_fallback(true)).await?;
    assert_ne!(StatusCode::OK, status, "{body}");
    assert!(body.contains("503"), "the upstream status travels: {body}");
    Ok(())
}

#[tokio::test]
async fn with_no_terminology_server_the_code_stands_as_the_value() -> Result<(), Box<dyn Error>>
{
    let (status, body) = mapped(Translator::default()).await?;
    assert_eq!(StatusCode::OK, status, "{body}");
    let answer: serde_json::Value = serde_json::from_str(&body)?;
    assert!(
        composition_text(&answer)?.contains(&format!("\"value\":\"{CODE}\"")),
        "{body}"
    );
    Ok(())
}
