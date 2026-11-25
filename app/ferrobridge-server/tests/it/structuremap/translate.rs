// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The `translate` transform (FHIR R4 §7.7.0.7.2) through `ConceptMap/$translate`
//! on a `wiremock` terminology server: every output kind, the codings of a
//! `CodeableConcept` in order, and the refusals.

use std::error::Error;
use std::sync::Arc;
use std::time::Duration;

use ferrobridge_server::structuremap::engine::translate::Terminology;
use ferrobridge_server::structuremap::engine::{Engine, MapError};
use ferrobridge_server::structuremap::model::Known;
use ferrobridge_server::structuremap::parse::parse;
use ferrobridge_testkit::stubs::terminology;
use fhir_types::codec::Value;
use http::StatusCode;
use serde_json::json;
use wiremock::{Mock, MockServer, ResponseTemplate, matchers};

/// The concept map every case names.
const MAP: &str = "http://example.org/ConceptMap/gender";

/// The map's URL.
const URL: &str = "http://example.org/StructureMap/translate";

/// A logical model named `name` with the elements `elements`, each a path
/// below the type and its type code.
fn model(name: &str, elements: &[(&str, &str)]) -> Value {
    let mut listed = vec![json!({"path": name})];
    listed.extend(elements.iter().map(|(path, code)| {
        json!({"path": format!("{name}.{path}"), "max": "1", "type": [{"code": code}]})
    }));
    Value::from_serde_json(json!({
        "resourceType": "StructureDefinition",
        "kind": "logical",
        "name": name,
        "snapshot": {"element": listed}
    }))
}

/// The source model and the target model with one element per output.
fn structures() -> Result<Known, Box<dyn Error>> {
    let mut known = Known::default();
    for model in [
        model(
            "TSource",
            &[("coding", "Coding"), ("concept", "CodeableConcept")],
        ),
        model(
            "TTarget",
            &[
                ("code", "string"),
                ("system", "string"),
                ("display", "string"),
                ("coding", "Coding"),
                ("concept", "CodeableConcept"),
            ],
        ),
    ] {
        known.logical.add(&model)?;
    }
    Ok(known)
}

/// A terminology server answering every `$translate` with `answer`.
async fn server(answer: ResponseTemplate) -> MockServer {
    let server = MockServer::start().await;
    Mock::given(matchers::method("POST"))
        .and(matchers::path("/r4/ConceptMap/$translate"))
        .and(matchers::body_string_contains(MAP))
        .respond_with(answer)
        .mount(&server)
        .await;
    server
}

/// The engine running `rules` with `translate` through `server`.
fn engine(rules: &str, server: &MockServer) -> Result<Arc<Engine>, Box<dyn Error>> {
    let map = parse(&format!(
        "map \"{URL}\" = translate\n\
         group main(source src : TSource, target tgt : TTarget) {{ {rules} }}"
    ))?;
    let base = format!("{}/r4", server.uri()).parse()?;
    let config =
        ferrobridge_term::config::Config::new(base, ferrobridge_term::config::WireVersion::R4)
            .with_timeout(Duration::from_secs(5))
            .with_retry(ferrobridge_term::config::RetryPolicy {
                max_attempts: 1,
                initial_backoff: Duration::from_millis(1),
                max_backoff: Duration::from_millis(1),
            });
    let client = ferrobridge_term::client::Client::new(config)?;
    let translator = Terminology::new(client, tokio::runtime::Handle::current());
    Ok(Arc::new(
        Engine::new(vec![map], structures()?).with_translator(Arc::new(translator)),
    ))
}

/// Runs `engine` over `source` into a `TTarget`.
async fn run(
    engine: Arc<Engine>,
    source: serde_json::Value,
) -> Result<serde_json::Value, MapError> {
    let output = engine
        .transform_blocking(
            URL.to_owned(),
            vec![Value::from_serde_json(source)],
            String::from("TTarget"),
        )
        .await?;
    Ok(serde_json::to_value(&output.target).expect("a target serializes"))
}

/// A source coding the map translates.
fn source() -> serde_json::Value {
    json!({"coding": {"system": "http://example.org/gender", "code": "M"}})
}

#[tokio::test(flavor = "multi_thread")]
async fn every_output_kind_writes_its_part_of_the_match() -> Result<(), Box<dyn Error>> {
    let server = server(terminology::translate(
        "equivalent",
        "http://hl7.org/fhir/administrative-gender",
        "male",
        "Male",
    ))
    .await;
    let engine = engine(
        &[
            "src.coding as c -> tgt.code = translate(c, 'http://example.org/ConceptMap/gender', 'code');",
            "src.coding as c -> tgt.system = translate(c, 'http://example.org/ConceptMap/gender', 'system');",
            "src.coding as c -> tgt.display = translate(c, 'http://example.org/ConceptMap/gender', 'display');",
            "src.coding as c -> tgt.coding = translate(c, 'http://example.org/ConceptMap/gender', 'Coding');",
            "src.coding as c -> tgt.concept = translate(c, 'http://example.org/ConceptMap/gender', 'CodeableConcept');",
        ]
        .join(" "),
        &server,
    )?;
    let coding = json!({
        "system": "http://hl7.org/fhir/administrative-gender",
        "code": "male",
        "display": "Male"
    });
    assert_eq!(
        json!({
            "code": "male",
            "system": "http://hl7.org/fhir/administrative-gender",
            "display": "Male",
            "coding": coding,
            "concept": {"coding": [coding]}
        }),
        run(engine, source()).await?
    );
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn a_codeable_concept_answers_with_its_first_translated_coding() -> Result<(), Box<dyn Error>>
{
    let server = MockServer::start().await;
    Mock::given(matchers::method("POST"))
        .and(matchers::path("/r4/ConceptMap/$translate"))
        .and(matchers::body_string_contains("\"X\""))
        .respond_with(terminology::translate_no_match("no map for X"))
        .mount(&server)
        .await;
    Mock::given(matchers::method("POST"))
        .and(matchers::path("/r4/ConceptMap/$translate"))
        .and(matchers::body_string_contains("\"F\""))
        .respond_with(terminology::translate(
            "equal",
            "http://hl7.org/fhir/administrative-gender",
            "female",
            "Female",
        ))
        .mount(&server)
        .await;
    let engine = engine(
        "src.concept as c -> tgt.code = translate(c, 'http://example.org/ConceptMap/gender', 'code');",
        &server,
    )?;
    let target = run(
        engine,
        json!({"concept": {"coding": [
            {"system": "http://example.org/other", "code": "X"},
            {"code": "no system"},
            {"system": "http://example.org/gender", "code": "F"}
        ]}}),
    )
    .await?;
    assert_eq!(json!({"code": "female"}), target);
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn a_refused_translate_stops_the_map_with_the_upstream_status() -> Result<(), Box<dyn Error>>
{
    let server = server(terminology::outcome(
        404,
        "not-found",
        None,
        "no such concept map",
    ))
    .await;
    let engine = engine(
        "src.coding as c -> tgt.code = translate(c, 'http://example.org/ConceptMap/gender', 'code');",
        &server,
    )?;
    match run(engine, source()).await {
        Err(MapError::Translate { map, source }) => {
            assert_eq!(MAP, map);
            assert_eq!(Some(StatusCode::NOT_FOUND), source.status());
        }
        other => panic!("a refused translate answered {other:?}"),
    }
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn no_match_and_an_inexact_match_are_no_translation() -> Result<(), Box<dyn Error>> {
    for (answer, message) in [
        (
            terminology::translate_no_match("no map for M"),
            Some(String::from("no map for M")),
        ),
        (
            terminology::translate(
                "wider",
                "http://hl7.org/fhir/administrative-gender",
                "other",
                "Other",
            ),
            None,
        ),
    ] {
        let server = server(answer).await;
        let engine = engine(
            "src.coding as c -> tgt.code = translate(c, 'http://example.org/ConceptMap/gender', 'code');",
            &server,
        )?;
        assert_eq!(
            Err(MapError::NoTranslation {
                map: MAP.to_owned(),
                message,
            }),
            run(engine, source()).await
        );
    }
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn matches_naming_two_concepts_are_refused() -> Result<(), Box<dyn Error>> {
    let server = server(terminology::translate_matches(&[
        (
            "equivalent",
            "http://hl7.org/fhir/administrative-gender",
            "male",
            "Male",
        ),
        (
            "equivalent",
            "http://hl7.org/fhir/administrative-gender",
            "other",
            "Other",
        ),
    ]))
    .await;
    let engine = engine(
        "src.coding as c -> tgt.code = translate(c, 'http://example.org/ConceptMap/gender', 'code');",
        &server,
    )?;
    assert_eq!(
        Err(MapError::AmbiguousTranslation(MAP.to_owned())),
        run(engine, source()).await
    );
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn a_bare_code_and_an_unknown_output_are_refused_unsent() -> Result<(), Box<dyn Error>> {
    let server = MockServer::start().await;
    Mock::given(matchers::any())
        .respond_with(ResponseTemplate::new(500))
        .expect(0)
        .mount(&server)
        .await;
    for (rule, source) in [
        (
            "src.coding as c -> tgt.code = translate(c, 'http://example.org/ConceptMap/gender', 'code');",
            json!({"coding": "M"}),
        ),
        (
            "src.coding as c -> tgt.code = translate(c, 'http://example.org/ConceptMap/gender', 'value');",
            source(),
        ),
    ] {
        let engine = engine(rule, &server)?;
        assert!(matches!(
            run(engine, source).await,
            Err(MapError::Transform { .. })
        ));
    }
    Ok(())
}
