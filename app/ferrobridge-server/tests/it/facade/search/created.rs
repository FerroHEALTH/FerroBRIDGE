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

use super::super::Harness;
use super::super::VERSION_ONE;
use super::super::harness_built;
use super::super::settings;
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
