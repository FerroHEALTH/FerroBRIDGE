// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The search interaction over a `wiremock` CDR: the declared parameters,
//! the AQL they write, the searchset Bundle, and every refusal.

mod load;
mod paging;
mod parameters;
mod wire;

use axum::body::Body;
use ferrobridge_server::cdr::ids::EhrId;
use ferrobridge_server::facade::identity::PersonId;
use ferrobridge_server::facade::identity::store::Store;
use ferrobridge_server::facade::search::Limits;
use http::Request;
use http::StatusCode;
use std::error::Error as StdError;
use wiremock::Mock;
use wiremock::MockServer;
use wiremock::ResponseTemplate;
use wiremock::matchers;

use super::EHR_ID;
use super::Harness;
use super::call;

/// The path of the AQL endpoint (`query-codegen.openapi.yaml`,
/// `query_execute_adhoc_query_body`).
const AQL: &str = "/query/aql";

/// The subject every case binds to [`EHR_ID`], as the identity map keys a
/// literal reference.
const SUBJECT: &str = "Patient/p-1";

/// A second version container, for a second composition.
const OTHER_CONTAINER: &str = "a1d0c6e8-0000-4000-8000-0000000000b2";

/// Returns a harness that answers search with the default page sizes, its
/// subject bound to [`EHR_ID`].
async fn searching() -> Harness {
    searching_with(Limits::default()).await
}

/// Returns a harness that answers search with `limits`, its subject bound to
/// [`EHR_ID`].
async fn searching_with(limits: Limits) -> Harness {
    let harness = super::search_harness(limits).await;
    let person = PersonId::new("http://example.org/fhir", SUBJECT).expect("a legal person key");
    harness
        .store
        .record_ehr(&person, &EhrId::new(EHR_ID).expect("a legal ehr_id"))
        .expect("the subject records");
    harness
}

/// Returns one result row: the EHR, the version and the composition.
fn row(version: &str, composition: &serde_json::Value) -> serde_json::Value {
    serde_json::json!([EHR_ID, version, composition])
}

/// Returns the ITS-REST result set carrying `rows`.
fn result_set(rows: &[serde_json::Value]) -> serde_json::Value {
    serde_json::json!({
        "q": "synthetic",
        "columns": [
            { "name": "ehr_id", "path": "/ehr_id/value" },
            { "name": "version_uid", "path": "/uid/value" },
            { "name": "composition", "path": "" }
        ],
        "rows": rows
    })
}

/// Whether a request body is a count query.
fn is_count(request: &wiremock::Request) -> bool {
    serde_json::from_slice::<serde_json::Value>(&request.body)
        .ok()
        .and_then(|body| body["q"].as_str().map(|text| text.contains("COUNT(*)")))
        .unwrap_or(false)
}

/// Answers every row query with `rows`.
async fn mount_rows(cdr: &MockServer, rows: &[serde_json::Value]) {
    Mock::given(matchers::method("POST"))
        .and(matchers::path(AQL))
        .and(|request: &wiremock::Request| !is_count(request))
        .respond_with(ResponseTemplate::new(200).set_body_json(result_set(rows)))
        .mount(cdr)
        .await;
}

/// Answers every count query with `count`.
async fn mount_count(cdr: &MockServer, count: u64) {
    Mock::given(matchers::method("POST"))
        .and(matchers::path(AQL))
        .and(is_count)
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "q": "synthetic",
            "columns": [{ "name": "total" }],
            "rows": [[count]]
        })))
        .mount(cdr)
        .await;
}

/// Returns the body of every AQL request the CDR received, in order.
async fn queries(harness: &Harness) -> Vec<serde_json::Value> {
    harness
        .cdr
        .received_requests()
        .await
        .unwrap_or_default()
        .iter()
        .filter(|request| request.url.path() == AQL)
        .filter_map(|request| serde_json::from_slice(&request.body).ok())
        .collect()
}

/// Sends `GET /fhir/{path_and_query}` and returns the status and the body.
async fn get(
    harness: &Harness,
    path_and_query: &str,
) -> Result<(StatusCode, serde_json::Value), Box<dyn StdError>> {
    call(
        harness.app(),
        Request::get(format!("/fhir/{path_and_query}")).body(Body::empty())?,
    )
    .await
}

/// Returns the values the query of `body` binds.
fn bound(body: &serde_json::Value) -> Vec<String> {
    body["query_parameters"]
        .as_object()
        .map(|parameters| {
            parameters
                .values()
                .filter_map(|value| value.as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default()
}

/// Returns the `url` of the link of `body` whose relation is `relation`.
fn link<'a>(body: &'a serde_json::Value, relation: &str) -> Option<&'a str> {
    body["link"]
        .as_array()?
        .iter()
        .find(|entry| entry["relation"].as_str() == Some(relation))
        .and_then(|entry| entry["url"].as_str())
}

/// Returns the entries of `body` whose `search.mode` is `match`.
fn matches(body: &serde_json::Value) -> Vec<&serde_json::Value> {
    body["entry"]
        .as_array()
        .map(|entries| {
            entries
                .iter()
                .filter(|entry| entry["search"]["mode"].as_str() == Some("match"))
                .collect()
        })
        .unwrap_or_default()
}
