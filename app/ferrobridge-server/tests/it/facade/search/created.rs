// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Search over a context that splits an entry into more than one resource:
//! every resource of the searched type is listed once, paged, counted and
//! found by `_id`.

use ferrobridge_server::cdr::ids::EhrId;
use ferrobridge_server::facade::identity::PersonId;
use ferrobridge_server::facade::identity::store::Store;
use ferrobridge_server::facade::search::Limits;
use http::StatusCode;
use std::error::Error as StdError;
use std::time::Duration;

use ferrobridge_server::terminology::Translator;
use ferrobridge_term::client::Client;
use ferrobridge_term::config::{Config, RetryPolicy, WireVersion};
use ferrobridge_testkit::stubs::terminology;
use wiremock::matchers;
use wiremock::{Mock, MockServer};

use super::super::Harness;
use super::super::VERSION_ONE;
use super::super::harness_built;
use super::super::harness_translating;
use super::super::settings;
use super::OTHER_CONTAINER;
use super::SUBJECT;
use super::get;
use super::link;
use super::matches;
use super::mount_count;
use super::mount_rows;
use super::row;

/// The fixture directory whose split context declares a search.
const CREATED: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/it/created");

/// The profile of the context that splits each anatomical location into a
/// Condition of its own.
const SPLIT_PROFILE: &str = "http://example.org/fhir/StructureDefinition/ferrobridge-created-split";

/// Returns a harness over the split context that answers search with
/// `limits`, its subject bound to the suite's EHR.
async fn splitting(limits: Limits) -> Harness {
    let harness = harness_built(CREATED, settings(), Some(limits)).await;
    let person = PersonId::new("http://example.org/fhir", SUBJECT).expect("a legal person key");
    harness
        .store
        .record_ehr(
            &person,
            &EhrId::new(super::super::EHR_ID).expect("a legal ehr_id"),
        )
        .expect("the subject records");
    harness
}

/// Returns the composition the Condition with two body sites maps to under
/// the split context.
fn two_sites(harness: &Harness) -> serde_json::Value {
    let mut resource: serde_json::Value =
        serde_json::from_str(ferrobridge_testkit::fixtures::R4_CONDITION)
            .expect("the synthetic Condition reads");
    resource["meta"] = serde_json::json!({ "profile": [SPLIT_PROFILE] });
    resource["bodySite"] = serde_json::json!([{ "text": "Left knee" }, { "text": "Right knee" }]);
    let loaded = harness
        .facade
        .programs()
        .by_context("ferrobridge_created_split.context")
        .cloned()
        .expect("the split context compiled");
    let document: fhir_types::codec::Value =
        serde_json::from_value(resource).expect("the Condition reads as a document");
    ferrobridge_server::facade::engine::inbound(
        loaded.program(),
        loaded.index(),
        &document,
        "2026-09-15T09:00:00Z",
        &settings(),
        &fhirconnect::engine::terminology::NoTerminology,
    )
    .expect("the Condition maps")
    .into_value()
    .into_value()
}

/// Returns the first `bodySite.text` of each matched resource, in order.
fn sites(body: &serde_json::Value) -> Vec<&str> {
    matches(body)
        .iter()
        .filter_map(|entry| entry["resource"]["bodySite"][0]["text"].as_str())
        .collect()
}

/// Returns the id of each matched resource, in order.
fn ids(body: &serde_json::Value) -> Vec<String> {
    matches(body)
        .iter()
        .filter_map(|entry| entry["resource"]["id"].as_str().map(str::to_owned))
        .collect()
}

#[tokio::test]
async fn a_split_entry_lists_every_resource_it_answers_once() -> Result<(), Box<dyn StdError>> {
    // HierarchyMappings.adoc §split: each occurrence is a resource of its own,
    // and R4 search.html §3.1.1: each matching resource is in the searchset.
    let harness = splitting(Limits::default()).await;
    let composition = two_sites(&harness);
    mount_rows(&harness.cdr, &[row(VERSION_ONE, &composition)]).await;
    let (status, body) = get(&harness, &format!("Condition?patient={SUBJECT}")).await?;
    assert_eq!(StatusCode::OK, status, "{body}");
    assert_eq!(vec!["Left knee", "Right knee"], sites(&body));
    let found = ids(&body);
    assert_eq!(2, found.len());
    assert_ne!(found.first(), found.get(1), "each resource has its own id");
    assert_eq!(Some(2), body["total"].as_u64());
    Ok(())
}

#[tokio::test]
async fn a_page_ends_inside_a_split_entry_and_the_next_page_resumes_there()
-> Result<(), Box<dyn StdError>> {
    // R4 search.html §3.1.1.5.3: a page holds no more than `_count`
    // resources, so the further occurrence opens the next page.
    let harness = splitting(Limits::default()).await;
    let composition = two_sites(&harness);
    mount_rows(&harness.cdr, &[row(VERSION_ONE, &composition)]).await;
    mount_count(&harness.cdr, std::slice::from_ref(&composition)).await;
    let (status, first) = get(
        &harness,
        &format!("Condition?patient={SUBJECT}&_count=1&_total=accurate"),
    )
    .await?;
    assert_eq!(StatusCode::OK, status, "{first}");
    assert_eq!(vec!["Left knee"], sites(&first));
    assert_eq!(
        Some(2),
        first["total"].as_u64(),
        "the total counts every resource the split answers"
    );
    let next = link(&first, "next").ok_or("the first page links the next")?;
    let path = next
        .strip_prefix("http://ferrobridge.invalid/fhir/")
        .ok_or("the next link is under the base")?;
    let (status, second) = get(&harness, path).await?;
    assert_eq!(StatusCode::OK, status, "{second}");
    assert_eq!(vec!["Right knee"], sites(&second));
    assert!(link(&second, "next").is_none(), "{second}");
    Ok(())
}

#[tokio::test]
async fn an_id_that_names_a_created_resource_answers_it_alone() -> Result<(), Box<dyn StdError>> {
    // R4 search.html §3.2.1.5.1: `_id` matches the logical id of the resource.
    let harness = splitting(Limits::default()).await;
    let composition = two_sites(&harness);
    mount_rows(&harness.cdr, &[row(VERSION_ONE, &composition)]).await;
    let (_, listed) = get(&harness, &format!("Condition?patient={SUBJECT}")).await?;
    let found = ids(&listed);
    let [parent, created] = found.as_slice() else {
        return Err(format!("two resources were listed: {listed}").into());
    };
    let (status, body) = get(&harness, &format!("Condition?_id={created}")).await?;
    assert_eq!(StatusCode::OK, status, "{body}");
    assert_eq!(vec![created.clone()], ids(&body));
    assert_eq!(vec!["Right knee"], sites(&body));
    let (status, body) = get(&harness, &format!("Condition?_id={parent}")).await?;
    assert_eq!(StatusCode::OK, status, "{body}");
    assert_eq!(vec![parent.clone()], ids(&body));
    Ok(())
}

/// The concept map the translating copy of the split model names on the
/// problem name. `example.org` is reserved for examples (RFC 6761 §6.5).
const CONCEPT_MAP: &str = "http://example.org/fhir/ConceptMap/ferrobridge-created-problems";

/// Writes the split context, its search and its model into a temporary tree,
/// the model's problem name translated through [`CONCEPT_MAP`].
fn translating_fixtures() -> Result<tempfile::TempDir, Box<dyn StdError>> {
    let root = tempfile::tempdir()?;
    for name in [
        "ferrobridge_created_split.context.yml",
        "ferrobridge_created_split.search.yml",
    ] {
        std::fs::copy(
            std::path::Path::new(CREATED).join(name),
            root.path().join(name),
        )?;
    }
    let model = std::fs::read_to_string(
        std::path::Path::new(CREATED).join("ferrobridge_created_split.yml"),
    )?;
    let problem = "      openehr: \"$archetype/data[at0001]/items[at0002]\"\n      type: \"CODEABLECONCEPT\"\n";
    assert!(
        model.contains(problem),
        "the split model maps the problem name"
    );
    let translated = model.replacen(
        problem,
        &format!("{problem}    conceptmap: \"{CONCEPT_MAP}\"\n"),
        1,
    );
    std::fs::write(
        root.path().join("ferrobridge_created_split.yml"),
        translated,
    )?;
    Ok(root)
}

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

#[tokio::test]
async fn a_code_shared_by_the_entries_of_a_page_and_its_total_is_translated_once()
-> Result<(), Box<dyn StdError>> {
    // NOTE: no specification governs this: our own design; one request
    // translates a code its page and its total share through one session.
    let composition = two_sites(&splitting(Limits::default()).await);
    let terminology = MockServer::start().await;
    Mock::given(matchers::method("POST"))
        .and(matchers::path("/r4/ConceptMap/$translate"))
        .respond_with(terminology::translate(
            "equivalent",
            "http://example.org/fhir/CodeSystem/synthetic-problems",
            "SYN-001",
            "Synthetic problem one",
        ))
        .mount(&terminology)
        .await;
    let fixtures = translating_fixtures()?;
    let harness = harness_translating(
        fixtures.path().to_str().ok_or("a UTF-8 temporary path")?,
        Limits::default(),
        translator(&terminology)?,
    )
    .await;
    let person = PersonId::new("http://example.org/fhir", SUBJECT)?;
    harness
        .store
        .record_ehr(&person, &EhrId::new(super::super::EHR_ID)?)?;
    let second = format!("{OTHER_CONTAINER}::ferrobridge.test::1");
    mount_rows(
        &harness.cdr,
        &[row(VERSION_ONE, &composition), row(&second, &composition)],
    )
    .await;

    let (status, body) = get(
        &harness,
        &format!("Condition?patient={SUBJECT}&_count=1&_total=accurate"),
    )
    .await?;
    assert_eq!(StatusCode::OK, status, "{body}");
    assert_eq!(1, matches(&body).len(), "{body}");
    assert_eq!(Some(4), body["total"].as_u64(), "{body}");
    let calls = terminology
        .received_requests()
        .await
        .ok_or("the terminology stub records requests")?
        .len();
    assert_eq!(
        1, calls,
        "the page and the total render three entries and translate their one code once"
    );

    let (status, body) = get(
        &harness,
        &format!("Condition?patient={SUBJECT}&_count=1&_total=accurate"),
    )
    .await?;
    assert_eq!(StatusCode::OK, status, "{body}");
    let calls = terminology
        .received_requests()
        .await
        .ok_or("the terminology stub records requests")?
        .len();
    assert_eq!(2, calls, "nothing is kept past the request");
    Ok(())
}
