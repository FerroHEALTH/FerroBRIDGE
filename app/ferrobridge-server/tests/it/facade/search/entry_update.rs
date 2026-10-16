// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! An update of one entry's resource replaces that entry in a new version of
//! the composition and keeps every other entry (#657).
//!
//! R4 http.html §3.1.0.4 (update): the update replaces the resource named, so
//! the other entries of the composition, each a resource of its own, stand
//! unchanged. The ids come from a search, because a search is what binds the
//! second entry of a composition to an id of its own.

use axum::body::Body;
use http::Request;
use http::StatusCode;
use http::header;
use std::error::Error as StdError;
use wiremock::Mock;
use wiremock::ResponseTemplate;
use wiremock::matchers;

use super::super::CONTAINER;
use super::super::EHR_ID;
use super::super::Harness;
use super::super::VERSION_ONE;
use super::super::VERSION_TWO;
use super::super::call;
use super::super::condition;
use super::super::named;
use super::super::raw;
use super::super::stub::mount_read;
use super::AQL;
use super::DIAGNOSIS;
use super::diagnosis;
use super::get;
use super::matches;
use super::node_row;
use super::result_set;
use super::searching;

/// The path of the composition's version container on the stub CDR.
fn container_path() -> String {
    format!("/ehr/{EHR_ID}/composition/{CONTAINER}")
}

/// Returns the composition one synthetic `Condition` coded `code` maps to.
fn coded(harness: &Harness, code: &str) -> serde_json::Value {
    let mut resource = condition();
    resource["code"]["coding"][0]["code"] = serde_json::json!(code);
    harness.composition_of(&resource)
}

/// Returns a composition holding two diagnoses, `SYN-001` then `SYN-002`,
/// with each diagnosis entry.
fn two_diagnoses(harness: &Harness) -> (serde_json::Value, serde_json::Value, serde_json::Value) {
    let mut composition = coded(harness, "SYN-001");
    let first = diagnosis(&composition);
    let second = diagnosis(&coded(harness, "SYN-002"));
    composition["content"]
        .as_array_mut()
        .expect("the composition has content")
        .push(second.clone());
    (composition, first, second)
}

/// Binds both entries of `composition` through a search each, and returns
/// the id of the first entry and of the second.
async fn entry_ids(
    harness: &Harness,
    composition: &serde_json::Value,
    first: &serde_json::Value,
    second: &serde_json::Value,
) -> Result<(String, String), Box<dyn StdError>> {
    for (code, node) in [("SYN-001", first), ("SYN-002", second)] {
        Mock::given(matchers::method("POST"))
            .and(matchers::path(AQL))
            .and(matchers::body_string_contains(format!("\"{code}\"")))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(result_set(&[node_row(
                    VERSION_ONE,
                    composition,
                    node,
                )])),
            )
            .mount(&harness.cdr)
            .await;
    }
    let mut ids = Vec::new();
    for code in ["SYN-001", "SYN-002"] {
        let (status, body) = get(harness, &format!("Condition?code={code}")).await?;
        assert_eq!(StatusCode::OK, status, "{body}");
        let id = matches(&body)
            .first()
            .and_then(|entry| entry["resource"]["id"].as_str())
            .ok_or("the search answers one match with an id")?
            .to_owned();
        ids.push(id);
    }
    let second_id = ids.pop().ok_or("two ids")?;
    let first_id = ids.pop().ok_or("two ids")?;
    assert_ne!(first_id, second_id, "two entries answered one id");
    Ok((first_id, second_id))
}

/// The stub CDR update that stores the composition it is sent as version
/// two and answers it back.
struct EchoUpdate;

impl wiremock::Respond for EchoUpdate {
    fn respond(&self, request: &wiremock::Request) -> ResponseTemplate {
        ResponseTemplate::new(200)
            .insert_header("ETag", format!("W/\"{VERSION_TWO}\""))
            .insert_header("Content-Type", "application/json")
            .set_body_bytes(request.body.clone())
    }
}

/// Returns the synthetic `Condition` with the id `id`, coded `code`.
fn revised(id: &str, code: &str) -> serde_json::Value {
    let mut resource = condition();
    resource["id"] = serde_json::json!(id);
    resource["code"]["coding"][0]["code"] = serde_json::json!(code);
    resource
}

/// Returns `PUT /fhir/Condition/{id}` carrying `body`, with `If-Match` when
/// one is given.
fn put(
    id: &str,
    body: &serde_json::Value,
    if_match: Option<&str>,
) -> Result<Request<Body>, Box<dyn StdError>> {
    let mut request = Request::put(format!("/fhir/Condition/{id}"))
        .header(header::CONTENT_TYPE, "application/fhir+json");
    if let Some(if_match) = if_match {
        request = request.header(header::IF_MATCH, if_match);
    }
    Ok(request.body(Body::from(named(body.clone(), id).to_string()))?)
}

/// Returns the diagnosis entries of `composition`, in `content` order.
fn diagnoses(composition: &serde_json::Value) -> Vec<&serde_json::Value> {
    composition["content"]
        .as_array()
        .map(|content| {
            content
                .iter()
                .filter(|item| item["archetype_node_id"].as_str() == Some(DIAGNOSIS))
                .collect()
        })
        .unwrap_or_default()
}

/// Whether `item` carries the code `code` anywhere.
fn carries(item: &serde_json::Value, code: &str) -> bool {
    item.to_string().contains(&format!("\"{code}\""))
}

/// Returns the composition the stub CDR was sent by the one update it took.
async fn updated(harness: &Harness) -> Result<serde_json::Value, Box<dyn StdError>> {
    let requests = harness.cdr.received_requests().await.unwrap_or_default();
    let puts: Vec<&wiremock::Request> = requests
        .iter()
        .filter(|request| request.method.as_str() == "PUT")
        .collect();
    assert_eq!(1, puts.len(), "the CDR took one update");
    let request = puts.first().ok_or("one update")?;
    assert_eq!(
        container_path(),
        request.url.path(),
        "the update names the version container"
    );
    Ok(serde_json::from_slice(&request.body)?)
}

/// Serves `composition` as the latest version two, ahead of any read mounted
/// before.
async fn mount_version_two_read(harness: &Harness, composition: &serde_json::Value) {
    Mock::given(matchers::method("GET"))
        .and(matchers::path(container_path()))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("ETag", format!("W/\"{VERSION_TWO}\""))
                .insert_header("Content-Type", "application/json")
                .set_body_string(composition.to_string()),
        )
        .with_priority(1)
        .mount(&harness.cdr)
        .await;
}

/// Returns the code of the resource `GET /fhir/Condition/{id}` answers.
async fn code_read(harness: &Harness, id: &str) -> Result<Option<String>, Box<dyn StdError>> {
    let (status, body) = get(harness, &format!("Condition/{id}")).await?;
    assert_eq!(StatusCode::OK, status, "{body}");
    assert_eq!(Some(id), body["id"].as_str(), "{body}");
    Ok(body["code"]["coding"][0]["code"]
        .as_str()
        .map(str::to_owned))
}

#[tokio::test]
async fn an_update_of_the_second_entry_keeps_the_first_and_replaces_the_second()
-> Result<(), Box<dyn StdError>> {
    let harness = searching().await;
    let (composition, first, second) = two_diagnoses(&harness);
    let (first_id, second_id) = entry_ids(&harness, &composition, &first, &second).await?;
    mount_read(&harness.cdr, VERSION_ONE, &composition).await;
    Mock::given(matchers::method("PUT"))
        .and(matchers::path(container_path()))
        .and(matchers::header("If-Match", format!("\"{VERSION_ONE}\"")))
        .respond_with(EchoUpdate)
        .mount(&harness.cdr)
        .await;

    let response = raw(
        harness.app(),
        put(&second_id, &revised(&second_id, "SYN-003"), Some("W/\"1\""))?,
    )
    .await?;
    assert_eq!(StatusCode::OK, response.status());
    assert_eq!(
        Some("W/\"2\""),
        response
            .headers()
            .get(header::ETAG)
            .and_then(|value| value.to_str().ok())
    );
    let bytes = axum::body::to_bytes(response.into_body(), 1024 * 1024).await?;
    let answered: serde_json::Value = serde_json::from_slice(&bytes)?;
    assert_eq!(
        Some(second_id.as_str()),
        answered["id"].as_str(),
        "{answered}"
    );
    assert_eq!(
        Some("SYN-003"),
        answered["code"]["coding"][0]["code"].as_str(),
        "the answer renders the updated entry: {answered}"
    );

    let sent = updated(&harness).await?;
    let entries = diagnoses(&sent);
    assert_eq!(2, entries.len(), "the update dropped an entry: {sent}");
    assert_eq!(
        Some(&&first),
        entries.first(),
        "the first entry is sent as it stood"
    );
    let replaced = entries.get(1).ok_or("a second entry")?;
    assert!(carries(replaced, "SYN-003"), "{replaced}");
    assert!(!carries(replaced, "SYN-002"), "{replaced}");
    assert_eq!(
        composition["content"].as_array().map(Vec::len),
        sent["content"].as_array().map(Vec::len),
        "the update changed the number of content items"
    );

    mount_version_two_read(&harness, &sent).await;
    assert_eq!(
        Some("SYN-001"),
        code_read(&harness, &first_id).await?.as_deref()
    );
    assert_eq!(
        Some("SYN-003"),
        code_read(&harness, &second_id).await?.as_deref()
    );
    Ok(())
}

#[tokio::test]
async fn an_update_of_the_first_entry_keeps_the_second() -> Result<(), Box<dyn StdError>> {
    let harness = searching().await;
    let (composition, first, second) = two_diagnoses(&harness);
    let (first_id, second_id) = entry_ids(&harness, &composition, &first, &second).await?;
    mount_read(&harness.cdr, VERSION_ONE, &composition).await;
    Mock::given(matchers::method("PUT"))
        .and(matchers::path(container_path()))
        .respond_with(EchoUpdate)
        .mount(&harness.cdr)
        .await;

    let (status, body) = call(
        harness.app(),
        put(&first_id, &revised(&first_id, "SYN-004"), None)?,
    )
    .await?;
    assert_eq!(StatusCode::OK, status, "{body}");

    let sent = updated(&harness).await?;
    let entries = diagnoses(&sent);
    assert_eq!(2, entries.len(), "the update dropped an entry: {sent}");
    let replaced = entries.first().ok_or("a first entry")?;
    assert!(carries(replaced, "SYN-004"), "{replaced}");
    assert_eq!(
        Some(&&second),
        entries.get(1),
        "the second entry is sent as it stood"
    );

    mount_version_two_read(&harness, &sent).await;
    assert_eq!(
        Some("SYN-004"),
        code_read(&harness, &first_id).await?.as_deref()
    );
    assert_eq!(
        Some("SYN-002"),
        code_read(&harness, &second_id).await?.as_deref()
    );
    Ok(())
}

#[tokio::test]
async fn an_update_of_one_entry_naming_a_stale_composition_version_is_four_hundred_and_twelve()
-> Result<(), Box<dyn StdError>> {
    // If-Match names the version of the composition, which every entry
    // shares (<https://hl7.org/fhir/R4/http.html#concurrency>).
    let harness = searching().await;
    let (composition, first, second) = two_diagnoses(&harness);
    let (_first_id, second_id) = entry_ids(&harness, &composition, &first, &second).await?;
    mount_read(&harness.cdr, VERSION_TWO, &composition).await;
    Mock::given(matchers::method("PUT"))
        .and(matchers::path(container_path()))
        .respond_with(EchoUpdate)
        .mount(&harness.cdr)
        .await;

    let response = raw(
        harness.app(),
        put(&second_id, &revised(&second_id, "SYN-003"), Some("W/\"1\""))?,
    )
    .await?;
    assert_eq!(StatusCode::PRECONDITION_FAILED, response.status());
    assert_eq!(
        Some("W/\"2\""),
        response
            .headers()
            .get(header::ETAG)
            .and_then(|value| value.to_str().ok())
    );
    assert_eq!(
        0,
        harness.received("PUT", &container_path()).await,
        "no update reached the CDR"
    );
    Ok(())
}

#[tokio::test]
async fn a_create_revising_the_source_of_the_second_entry_keeps_the_first()
-> Result<(), Box<dyn StdError>> {
    // A create of a source id the map consumed at another `meta.versionId`
    // is an update in effect, so it follows the same rule.
    let harness = searching().await;
    let (composition, first, second) = two_diagnoses(&harness);
    let (_first_id, second_id) = entry_ids(&harness, &composition, &first, &second).await?;
    mount_read(&harness.cdr, VERSION_ONE, &composition).await;
    Mock::given(matchers::method("PUT"))
        .and(matchers::path(container_path()))
        .respond_with(EchoUpdate)
        .mount(&harness.cdr)
        .await;
    let mut source = revised(&second_id, "SYN-003");
    source["meta"]["versionId"] = serde_json::json!("1");
    let (status, body) = call(harness.app(), put(&second_id, &source, None)?).await?;
    assert_eq!(StatusCode::OK, status, "{body}");
    let sent = updated(&harness).await?;
    mount_version_two_read(&harness, &sent).await;

    let mut later = revised(&second_id, "SYN-005");
    later["meta"]["versionId"] = serde_json::json!("2");
    let (status, body) = call(
        harness.app(),
        Request::post("/fhir/Condition")
            .header(header::CONTENT_TYPE, "application/fhir+json")
            .body(Body::from(later.to_string()))?,
    )
    .await?;
    assert_eq!(StatusCode::OK, status, "{body}");
    assert_eq!(Some(second_id.as_str()), body["id"].as_str(), "{body}");

    let requests = harness.cdr.received_requests().await.unwrap_or_default();
    let last = requests
        .iter()
        .rev()
        .find(|request| request.method.as_str() == "PUT")
        .ok_or("the create revised the composition")?;
    let resent: serde_json::Value = serde_json::from_slice(&last.body)?;
    let entries = diagnoses(&resent);
    assert_eq!(2, entries.len(), "the revision dropped an entry: {resent}");
    assert_eq!(Some(&&first), entries.first(), "the first entry stood");
    let replaced = entries.get(1).ok_or("a second entry")?;
    assert!(carries(replaced, "SYN-005"), "{replaced}");
    Ok(())
}
