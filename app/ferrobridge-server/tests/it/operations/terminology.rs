// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! A `conceptmap` mapping through the operations lane, translated on a
//! `wiremock` terminology server.
//!
//! A mapping that names a `conceptmap` has its codes "transformed using the
//! conceptmap" (`types-of-mappings/concept-type/manual.adoc`, §`ConceptMaps`).
//! The map is read as authored from openEHR to FHIR, so the openEHR-bound
//! call sends `reverse=true` and the FHIR-bound call sends none
//! (<https://hl7.org/fhir/R4/conceptmap-operation-translate.html>).

use std::error::Error as StdError;
use std::time::Duration;

use axum::body::Body;
use ferrobridge_server::terminology::Translator;
use ferrobridge_term::client::Client;
use ferrobridge_term::config::{Config, RetryPolicy, WireVersion};
use ferrobridge_testkit::stubs::terminology;
use http::{Request, StatusCode, header};
use wiremock::matchers::{body_string_contains, method, path};
use wiremock::{Mock, MockServer};

use super::{
    FHIR_JSON, OPENEHR_JSON, app_with, bundle, call, composition_text, mapping_tree_with,
    server_settings, state_with,
};

/// The concept map the synthetic mapping names. `example.org` is reserved
/// for examples (RFC 6761 §6.5).
const CONCEPT_MAP: &str = "http://example.org/fhir/ConceptMap/ferrobridge-server-problems";

/// The code system the synthetic Condition codes its problem in.
const FHIR_SYSTEM: &str = "http://example.org/fhir/CodeSystem/synthetic-problems";

/// The code the synthetic Condition carries.
const FHIR_CODE: &str = "SYN-001";

/// The code system the openEHR side of the map codes the problem in.
const OPENEHR_SYSTEM: &str = "http://example.org/fhir/CodeSystem/synthetic-openehr-problems";

/// The code the openEHR side of the map holds for [`FHIR_CODE`].
const OPENEHR_CODE: &str = "OE-001";

/// The synthetic model mapping of the lane's harness, with the problem name
/// translated through [`CONCEPT_MAP`].
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
  - name: "problemDiagnose"
    with:
      fhir: "$resource.code"
      openehr: "$archetype/data[at0001]/items[at0002]"
      type: "CODEABLECONCEPT"
    conceptmap: "http://example.org/fhir/ConceptMap/ferrobridge-server-problems"

  - name: "subject"
    unidirectional: "openehr->fhir"
    with:
      fhir: "$resource.subject"
    manual:
      - name: "subjectReference"
        fhir:
          - path: "reference"
            value: "Patient/synthetic-subject-0001"
"#;

/// Returns a translator over the R4 terminology server `server` mocks.
fn translator(server: &MockServer) -> Result<Translator, Box<dyn StdError>> {
    let config = Config::new(format!("{}/r4", server.uri()).parse()?, WireVersion::R4)
        .with_timeout(Duration::from_secs(5))
        .with_retry(RetryPolicy {
            max_attempts: 1,
            initial_backoff: Duration::from_millis(1),
            max_backoff: Duration::from_millis(1),
        });
    Ok(Translator::new(Some(Client::new(config)?)))
}

/// Mounts the map in both directions: a `reverse` call answers the openEHR
/// code, a forward call the FHIR code.
async fn mount_map(server: &MockServer) {
    Mock::given(method("POST"))
        .and(path("/r4/ConceptMap/$translate"))
        .and(body_string_contains(r#""name":"reverse""#))
        .respond_with(terminology::translate(
            "equivalent",
            OPENEHR_SYSTEM,
            OPENEHR_CODE,
            "Synthetic openEHR problem one",
        ))
        .with_priority(1)
        .mount(server)
        .await;
    Mock::given(method("POST"))
        .and(path("/r4/ConceptMap/$translate"))
        .respond_with(terminology::translate(
            "equivalent",
            FHIR_SYSTEM,
            FHIR_CODE,
            "Synthetic problem one",
        ))
        .with_priority(2)
        .mount(server)
        .await;
}

/// Returns the value of the `Parameters` parameter `name` the request
/// `index` of `server` carried.
async fn sent(
    server: &MockServer,
    index: usize,
    name: &str,
) -> Result<Option<serde_json::Value>, Box<dyn StdError>> {
    let requests = server
        .received_requests()
        .await
        .ok_or("the mock server records requests")?;
    let request = requests.get(index).ok_or("no request at that index")?;
    let body: serde_json::Value = serde_json::from_slice(&request.body)?;
    Ok(body
        .get("parameter")
        .and_then(serde_json::Value::as_array)
        .and_then(|parameters| {
            parameters
                .iter()
                .find(|parameter| parameter.get("name") == Some(&serde_json::json!(name)))
        })
        .and_then(serde_json::Value::as_object)
        .and_then(|parameter| {
            parameter
                .iter()
                .find(|(key, _)| key.starts_with("value"))
                .map(|(_, value)| value.clone())
        }))
}

#[tokio::test]
async fn a_conceptmap_translates_both_ways_with_reverse_only_going_into_openehr()
-> Result<(), Box<dyn StdError>> {
    let server = MockServer::start().await;
    mount_map(&server).await;
    let root = mapping_tree_with(MODEL_FILE)?;

    let (status, _, body) = call(
        app_with(&root, translator(&server)?)?,
        Request::post("/fhir/$toopenehr")
            .header(header::CONTENT_TYPE, FHIR_JSON)
            .body(Body::from(bundle()?))?,
    )
    .await?;
    assert_eq!(StatusCode::OK, status, "{body}");
    let answer: serde_json::Value = serde_json::from_str(&body)?;
    let built = composition_text(&answer)?;
    assert!(built.contains(OPENEHR_CODE), "{built}");
    assert!(
        !built.contains(FHIR_CODE),
        "the source code is never passed through: {built}"
    );

    assert_eq!(
        Some(serde_json::json!(true)),
        sent(&server, 0, "reverse").await?
    );
    assert_eq!(
        Some(serde_json::json!(FHIR_CODE)),
        sent(&server, 0, "code").await?
    );
    assert_eq!(
        Some(serde_json::json!(CONCEPT_MAP)),
        sent(&server, 0, "url").await?
    );
    assert_eq!(None, sent(&server, 0, "targetsystem").await?);

    let mut composition: serde_json::Value = serde_json::from_str(built)?;
    if let Some(object) = composition.as_object_mut() {
        object.insert(
            String::from("uid"),
            serde_json::json!({
                "_type": "OBJECT_VERSION_ID",
                "value": "d2b3c1a0-0000-4000-8000-000000000002::ferrobridge.example::1",
            }),
        );
    }
    let (status, _, body) = call(
        app_with(&root, translator(&server)?)?,
        Request::post("/fhir/tofhir")
            .header(header::CONTENT_TYPE, OPENEHR_JSON)
            .body(Body::from(composition.to_string()))?,
    )
    .await?;
    assert_eq!(StatusCode::OK, status, "{body}");
    assert!(body.contains(FHIR_CODE), "{body}");
    assert!(body.contains(FHIR_SYSTEM), "{body}");

    assert_eq!(None, sent(&server, 1, "reverse").await?);
    assert_eq!(
        Some(serde_json::json!(OPENEHR_CODE)),
        sent(&server, 1, "code").await?
    );
    assert_eq!(
        Some(serde_json::json!(OPENEHR_SYSTEM)),
        sent(&server, 1, "system").await?
    );
    let calls = server
        .received_requests()
        .await
        .ok_or("the mock server records requests")?
        .len();
    assert_eq!(2, calls, "each distinct code is translated once per run");
    Ok(())
}

#[tokio::test]
async fn a_refused_translation_refuses_the_run_with_the_upstream_status()
-> Result<(), Box<dyn StdError>> {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/r4/ConceptMap/$translate"))
        .respond_with(terminology::outcome(
            404,
            "not-found",
            Some("not-found"),
            "no ConceptMap with that canonical",
        ))
        .mount(&server)
        .await;
    let root = mapping_tree_with(MODEL_FILE)?;
    let (status, _, body) = call(
        app_with(&root, translator(&server)?)?,
        Request::post("/fhir/$toopenehr")
            .header(header::CONTENT_TYPE, FHIR_JSON)
            .body(Body::from(bundle()?))?,
    )
    .await?;
    assert_ne!(StatusCode::OK, status, "{body}");
    assert!(body.contains(CONCEPT_MAP), "{body}");
    assert!(body.contains("404"), "the upstream status travels: {body}");
    Ok(())
}

#[tokio::test]
async fn with_no_terminology_server_a_conceptmap_refuses_naming_the_missing_server()
-> Result<(), Box<dyn StdError>> {
    let root = mapping_tree_with(MODEL_FILE)?;
    let (status, _, body) = call(
        app_with(&root, Translator::default())?,
        Request::post("/fhir/$toopenehr")
            .header(header::CONTENT_TYPE, FHIR_JSON)
            .body(Body::from(bundle()?))?,
    )
    .await?;
    assert_ne!(StatusCode::OK, status, "{body}");
    assert!(
        body.contains("no terminology server is configured"),
        "the refusal names the missing server: {body}"
    );
    assert!(body.contains(CONCEPT_MAP), "{body}");
    Ok(())
}

#[tokio::test]
async fn a_run_the_engine_refuses_is_counted_as_a_mapping_failure() -> Result<(), Box<dyn StdError>>
{
    let root = mapping_tree_with(MODEL_FILE)?;
    let state = state_with(&root, Translator::default())?;
    let app = ferrobridge_server::router(std::sync::Arc::clone(&state), &server_settings());
    let (status, _, body) = call(
        app.clone(),
        Request::post("/fhir/$toopenehr")
            .header(header::CONTENT_TYPE, FHIR_JSON)
            .body(Body::from(bundle()?))?,
    )
    .await?;
    assert_eq!(StatusCode::UNPROCESSABLE_ENTITY, status, "{body}");
    assert_eq!(1, state.usage_window().take().mapping_failures);
    let (status, _, body) = call(
        app,
        Request::post("/fhir/$toopenehr")
            .header(header::CONTENT_TYPE, "text/plain")
            .body(Body::from("not a FHIR body"))?,
    )
    .await?;
    assert_eq!(StatusCode::UNSUPPORTED_MEDIA_TYPE, status, "{body}");
    assert_eq!(
        0,
        state.usage_window().take().mapping_failures,
        "a request refused before a program ran is no mapping failure"
    );
    Ok(())
}
