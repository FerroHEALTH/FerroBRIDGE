// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The batch Bundle: each entry processed on its own and answered in a
//! `batch-response`, in request order
//! (<https://hl7.org/fhir/R4/http.html#transaction>, §3.1.0.11).

use ferrobridge_server::cdr::ids::EhrId;
use ferrobridge_server::facade::identity::PersonId;
use ferrobridge_server::facade::identity::store::Store;
use ferrobridge_server::facade::search::Limits;
use http::StatusCode;
use std::error::Error as StdError;
use wiremock::Mock;
use wiremock::MockServer;
use wiremock::ResponseTemplate;
use wiremock::matchers;

use super::EHR_ID;
use super::Harness;
use super::VERSION_ONE;
use super::VERSION_TWO;
use super::call;
use super::condition;
use super::first_issue;
use super::harness;
use super::named;
use super::post_condition;
use super::post_transaction;
use super::revised_condition;
use super::stub::mount_create;
use super::stub::mount_ehr;
use super::stub::mount_read;
use super::stub::mount_version_two;
use super::writes;

/// A second version container, for a second composition.
const OTHER_CONTAINER: &str = "a1d0c6e8-0000-4000-8000-0000000000b2";

/// Returns a `batch` Bundle carrying `entries`.
pub(super) fn batch_of(entries: &[serde_json::Value]) -> serde_json::Value {
    serde_json::json!({ "resourceType": "Bundle", "type": "batch", "entry": entries })
}

/// Returns a batch entry of `method` on `url`, carrying `resource` when given.
pub(super) fn entry(
    method: &str,
    url: &str,
    resource: Option<serde_json::Value>,
) -> serde_json::Value {
    let mut entry = serde_json::json!({ "request": { "method": method, "url": url } });
    if let Some(resource) = resource {
        entry["resource"] = resource;
    }
    entry
}

/// Returns the synthetic Condition under the sender id `id`.
pub(super) fn condition_as(id: &str) -> serde_json::Value {
    let mut resource = condition();
    resource["id"] = serde_json::json!(id);
    resource
}

/// Returns the `response.status` of every entry of a `batch-response`.
pub(super) fn statuses(body: &serde_json::Value) -> Vec<&str> {
    body["entry"]
        .as_array()
        .map(|entries| {
            entries
                .iter()
                .filter_map(|entry| entry["response"]["status"].as_str())
                .collect()
        })
        .unwrap_or_default()
}

/// Mounts a composition create that answers `201` with `version` once.
pub(super) async fn mount_create_once(cdr: &MockServer, version: &str, body: &serde_json::Value) {
    Mock::given(matchers::method("POST"))
        .and(matchers::path(format!("/ehr/{EHR_ID}/composition")))
        .respond_with(
            ResponseTemplate::new(201)
                .insert_header("ETag", format!("W/\"{version}\""))
                .insert_header("Content-Type", "application/json")
                .set_body_string(body.to_string()),
        )
        .up_to_n_times(1)
        .mount(cdr)
        .await;
}

/// Creates the fixture Condition at [`VERSION_ONE`] through a create stub that
/// answers once, and returns its logical id, so a later stub answers the
/// batch's own creates.
async fn created(harness: &Harness) -> Result<String, Box<dyn StdError>> {
    mount_ehr(&harness.cdr).await;
    mount_create_once(&harness.cdr, VERSION_ONE, &harness.composition()).await;
    let (status, body) = call(harness.app(), post_condition(&condition())).await?;
    assert_eq!(StatusCode::CREATED, status, "{body}");
    Ok(body["id"].as_str().ok_or("an id")?.to_owned())
}

/// Returns a harness that answers search, with the subject `Patient/p-1`
/// bound to [`EHR_ID`].
async fn searching() -> Result<Harness, Box<dyn StdError>> {
    let harness = super::search_harness(Limits::default()).await;
    harness.store.record_ehr(
        &PersonId::new("http://example.org/fhir", "Patient/p-1")?,
        &EhrId::new(EHR_ID)?,
    )?;
    Ok(harness)
}

/// Answers every AQL query with one row of `composition` at `version`.
async fn mount_search(cdr: &MockServer, version: &str, composition: &serde_json::Value) {
    Mock::given(matchers::method("POST"))
        .and(matchers::path("/query/aql"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "q": "synthetic",
            "columns": [
                { "name": "ehr_id", "path": "/ehr_id/value" },
                { "name": "version_uid", "path": "/uid/value" },
                { "name": "composition", "path": "" }
            ],
            "rows": [[EHR_ID, version, composition]]
        })))
        .mount(cdr)
        .await;
}

#[tokio::test]
async fn a_mixed_batch_answers_every_entry_in_request_order() -> Result<(), Box<dyn StdError>> {
    // R4 http.html §3.1.0.11.3: one response entry per request entry, in the
    // same order, each with its status, location and ETag.
    let harness = searching().await?;
    mount_ehr(&harness.cdr).await;
    let id = created(&harness).await?;
    let second = condition_as("ferrobridge-synthetic-condition-2");
    let other_version = format!("{OTHER_CONTAINER}::ferrobridge.test::1");
    mount_create(
        &harness.cdr,
        &other_version,
        &harness.composition_of(&second),
    )
    .await;
    mount_read(&harness.cdr, VERSION_ONE, &harness.composition()).await;
    mount_version_two(&harness).await;
    mount_search(&harness.cdr, VERSION_ONE, &harness.composition()).await;

    let (status, body) = post_transaction(
        &harness,
        &batch_of(&[
            entry("GET", &format!("Condition/{id}"), None),
            entry("POST", "Condition", Some(second)),
            entry("GET", "Condition?patient=p-1", None),
            entry(
                "PUT",
                &format!("Condition/{id}"),
                Some(named(revised_condition(), &id)),
            ),
        ]),
    )
    .await?;
    assert_eq!(StatusCode::OK, status, "{body}");
    assert_eq!(Some("Bundle"), body["resourceType"].as_str());
    assert_eq!(Some("batch-response"), body["type"].as_str());
    assert_eq!(
        vec!["200 OK", "201 Created", "200 OK", "200 OK"],
        statuses(&body),
        "{body}"
    );
    let read = &body["entry"][0];
    assert_eq!(Some("Condition"), read["resource"]["resourceType"].as_str());
    assert_eq!(Some(id.as_str()), read["resource"]["id"].as_str());
    assert_eq!(Some("W/\"1\""), read["response"]["etag"].as_str());
    let created = &body["entry"][1]["response"];
    assert!(
        created["location"]
            .as_str()
            .is_some_and(|location| location.contains("/fhir/Condition/")
                && location.ends_with("/_history/1")),
        "{body}"
    );
    let searched = &body["entry"][2]["resource"];
    assert_eq!(Some("searchset"), searched["type"].as_str(), "{body}");
    assert_eq!(Some(1), searched["total"].as_u64(), "{body}");
    assert_eq!(
        Some("W/\"2\""),
        body["entry"][3]["response"]["etag"].as_str(),
        "{body}"
    );
    assert_eq!((2, 1, 0), writes(&harness).await, "two creates, one update");
    Ok(())
}

#[tokio::test]
async fn a_cdr_refusal_of_one_entry_is_that_entry_status_and_the_others_proceed()
-> Result<(), Box<dyn StdError>> {
    // R4 http.html §3.1.0.11.1: the batch answers 200 whatever the entries'
    // outcomes, and an entry other than 2xx failed on its own.
    let harness = harness().await;
    mount_ehr(&harness.cdr).await;
    let id = created(&harness).await?;
    Mock::given(matchers::method("POST"))
        .and(matchers::path(format!("/ehr/{EHR_ID}/composition")))
        .respond_with(ferrobridge_testkit::stubs::its_rest::unprocessable(
            "synthetic validation failure",
            &["/content[0]: synthetic"],
        ))
        .mount(&harness.cdr)
        .await;
    mount_read(&harness.cdr, VERSION_ONE, &harness.composition()).await;

    let (status, body) = post_transaction(
        &harness,
        &batch_of(&[
            entry(
                "POST",
                "Condition",
                Some(condition_as("ferrobridge-synthetic-condition-3")),
            ),
            entry("GET", &format!("Condition/{id}"), None),
        ]),
    )
    .await?;
    assert_eq!(StatusCode::OK, status, "{body}");
    assert_eq!(
        vec!["422 Unprocessable Entity", "200 OK"],
        statuses(&body),
        "{body}"
    );
    let outcome = &body["entry"][0]["response"]["outcome"];
    assert_eq!(Some("OperationOutcome"), outcome["resourceType"].as_str());
    assert!(
        outcome.to_string().contains("synthetic validation failure"),
        "the CDR's error body travels in the entry's outcome: {body}"
    );
    assert!(body["entry"][0]["resource"].is_null(), "{body}");
    assert_eq!(
        Some("Condition"),
        body["entry"][1]["resource"]["resourceType"].as_str()
    );
    Ok(())
}

#[tokio::test]
async fn an_entry_referencing_another_entry_is_refused_and_the_other_proceeds()
-> Result<(), Box<dyn StdError>> {
    // R4 http.html §3.1.0.11.1: a reference to another entry being created in
    // the batch is non-conformant.
    let harness = harness().await;
    mount_ehr(&harness.cdr).await;
    mount_create(&harness.cdr, VERSION_ONE, &harness.composition()).await;
    let mut dependent = condition_as("ferrobridge-synthetic-condition-4");
    dependent["evidence"] = serde_json::json!([{ "detail": [{ "reference": "urn:uuid:7f3a0000-0000-4000-8000-000000000001" }] }]);
    let mut first = entry("POST", "Condition", Some(condition()));
    first["fullUrl"] = serde_json::json!("urn:uuid:7f3a0000-0000-4000-8000-000000000001");
    let mut second = entry("POST", "Condition", Some(dependent));
    second["fullUrl"] = serde_json::json!("urn:uuid:7f3a0000-0000-4000-8000-000000000002");

    let (status, body) = post_transaction(&harness, &batch_of(&[first, second])).await?;
    assert_eq!(StatusCode::OK, status, "{body}");
    assert_eq!(
        vec!["201 Created", "400 Bad Request"],
        statuses(&body),
        "{body}"
    );
    let issue = first_issue(&body["entry"][1]["response"]["outcome"]);
    assert_eq!(Some("invalid"), issue["code"].as_str(), "{body}");
    assert_eq!(
        Some("Bundle.entry[1].resource"),
        issue["location"][0].as_str()
    );
    assert_eq!(
        (1, 0, 0),
        writes(&harness).await,
        "only the first is committed"
    );
    Ok(())
}

#[tokio::test]
async fn an_entry_without_a_request_refuses_the_whole_batch() -> Result<(), Box<dyn StdError>> {
    // R4 http.html §3.1.0.11: each entry SHALL carry request details, and a
    // POST or PUT a resource.
    let harness = harness().await;
    mount_ehr(&harness.cdr).await;
    mount_create(&harness.cdr, VERSION_ONE, &harness.composition()).await;
    let (status, body) = post_transaction(
        &harness,
        &batch_of(&[
            entry("POST", "Condition", Some(condition())),
            serde_json::json!({ "resource": condition_as("ferrobridge-synthetic-condition-5") }),
            entry("PUT", "Condition/c-1", None),
        ]),
    )
    .await?;
    assert_eq!(StatusCode::BAD_REQUEST, status, "{body}");
    let locations: Vec<&str> = body["issue"]
        .as_array()
        .map(|issues| {
            issues
                .iter()
                .filter_map(|issue| issue["location"][0].as_str())
                .collect()
        })
        .unwrap_or_default();
    assert_eq!(
        vec!["Bundle.entry[1].request", "Bundle.entry[2].resource"],
        locations,
        "{body}"
    );
    assert_eq!((0, 0, 0), writes(&harness).await, "nothing runs");
    Ok(())
}

#[tokio::test]
async fn an_entry_of_an_interaction_the_server_does_not_declare_answers_its_route_status()
-> Result<(), Box<dyn StdError>> {
    // The CapabilityStatement declares no delete, and no route answers
    // history, so each entry answers what its route would.
    let harness = harness().await;
    let (status, body) = post_transaction(
        &harness,
        &batch_of(&[
            entry("DELETE", "Condition/c-1", None),
            entry("GET", "Condition/c-1/_history", None),
            entry("GET", "Condition/c-unknown", None),
            entry("GET", "http://elsewhere.invalid/fhir/Condition/c-1", None),
        ]),
    )
    .await?;
    assert_eq!(StatusCode::OK, status, "{body}");
    assert_eq!(
        vec![
            "405 Method Not Allowed",
            "404 Not Found",
            "404 Not Found",
            "400 Bad Request"
        ],
        statuses(&body),
        "{body}"
    );
    assert_eq!(
        Some("not-supported"),
        first_issue(&body["entry"][0]["response"]["outcome"])["code"].as_str()
    );
    Ok(())
}

#[tokio::test]
async fn an_absolute_url_under_this_base_reads_as_the_relative_one() -> Result<(), Box<dyn StdError>>
{
    let harness = harness().await;
    mount_ehr(&harness.cdr).await;
    let id = created(&harness).await?;
    mount_read(&harness.cdr, VERSION_ONE, &harness.composition()).await;
    let (status, body) = post_transaction(
        &harness,
        &batch_of(&[entry(
            "GET",
            &format!("{}/Condition/{id}", super::BASE_URL),
            None,
        )]),
    )
    .await?;
    assert_eq!(StatusCode::OK, status, "{body}");
    assert_eq!(vec!["200 OK"], statuses(&body), "{body}");
    assert_eq!(
        Some(format!("{}/Condition/{id}", super::BASE_URL).as_str()),
        body["entry"][0]["fullUrl"].as_str()
    );
    Ok(())
}

#[tokio::test]
async fn an_entry_if_match_reaches_its_update_as_the_header() -> Result<(), Box<dyn StdError>> {
    // Bundle.entry.request.ifMatch is the If-Match of the entry's update
    // (R4 bundle-definitions.html#Bundle.entry.request.ifMatch).
    let harness = harness().await;
    mount_ehr(&harness.cdr).await;
    let id = created(&harness).await?;
    mount_read(&harness.cdr, VERSION_TWO, &harness.composition()).await;
    mount_version_two(&harness).await;
    let mut stale = entry(
        "PUT",
        &format!("Condition/{id}"),
        Some(named(revised_condition(), &id)),
    );
    stale["request"]["ifMatch"] = serde_json::json!("W/\"1\"");
    let (status, body) = post_transaction(&harness, &batch_of(&[stale])).await?;
    assert_eq!(StatusCode::OK, status, "{body}");
    assert_eq!(vec!["412 Precondition Failed"], statuses(&body), "{body}");
    assert_eq!(
        (1, 0, 0),
        writes(&harness).await,
        "the stale update commits nothing"
    );
    Ok(())
}

#[tokio::test]
async fn prefer_minimal_answers_each_written_entry_without_its_resource()
-> Result<(), Box<dyn StdError>> {
    // R4 http.html §3.1.0.11.3: a resource is included in an entry as the
    // Prefer header specifies.
    let harness = harness().await;
    mount_ehr(&harness.cdr).await;
    mount_create(&harness.cdr, VERSION_ONE, &harness.composition()).await;
    let request = http::Request::post("/fhir")
        .header(http::header::CONTENT_TYPE, "application/fhir+json")
        .header("Prefer", "return=minimal")
        .body(axum::body::Body::from(
            batch_of(&[entry("POST", "Condition", Some(condition()))]).to_string(),
        ))?;
    let (status, body) = call(harness.app(), request).await?;
    assert_eq!(StatusCode::OK, status, "{body}");
    assert_eq!(vec!["201 Created"], statuses(&body), "{body}");
    assert!(body["entry"][0]["resource"].is_null(), "{body}");
    assert!(
        body["entry"][0]["response"]["location"].is_string(),
        "{body}"
    );
    Ok(())
}

#[tokio::test]
async fn an_empty_batch_answers_an_empty_batch_response() -> Result<(), Box<dyn StdError>> {
    // FHIR JSON forbids an empty array (<https://hl7.org/fhir/R4/json.html>),
    // so a batch Bundle with no entry omits the element.
    let harness = harness().await;
    let (status, body) = post_transaction(
        &harness,
        &serde_json::json!({ "resourceType": "Bundle", "type": "batch" }),
    )
    .await?;
    assert_eq!(StatusCode::OK, status, "{body}");
    assert_eq!(Some("batch-response"), body["type"].as_str());
    assert!(body["entry"].is_null(), "{body}");
    Ok(())
}
#[test]
fn a_batch_with_one_failing_entry_answers_200_and_counts_the_failure()
-> Result<(), Box<dyn StdError>> {
    // R4 http.html §3.1.0.11.1: a batch answers 200 whatever its entries'
    // outcomes, each entry carrying its own status and outcome. R4 gives a
    // batch-response no element for a count (bundle.html, bdl-1 keeps `total`
    // to a searchset or history), so the count of failed entries is the
    // field of the batch's log line: no specification governs this, our own
    // design.
    let logs = crate::support::Logs::default();
    let capture = ferrobridge_server::telemetry::subscriber(
        ferrobridge_server::telemetry::Rendering::Json,
        "info",
        false,
        logs.clone(),
    );
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    let (status, body) = tracing::subscriber::with_default(capture, || {
        runtime.block_on(async {
            let harness = harness().await;
            mount_ehr(&harness.cdr).await;
            let id = created(&harness).await?;
            Mock::given(matchers::method("POST"))
                .and(matchers::path(format!("/ehr/{EHR_ID}/composition")))
                .respond_with(ferrobridge_testkit::stubs::its_rest::unprocessable(
                    "synthetic validation failure",
                    &["/content[0]: synthetic"],
                ))
                .mount(&harness.cdr)
                .await;
            mount_read(&harness.cdr, VERSION_ONE, &harness.composition()).await;
            post_transaction(
                &harness,
                &batch_of(&[
                    entry(
                        "POST",
                        "Condition",
                        Some(condition_as("ferrobridge-synthetic-condition-4")),
                    ),
                    entry("GET", &format!("Condition/{id}"), None),
                ]),
            )
            .await
        })
    })?;
    assert_eq!(StatusCode::OK, status, "{body}");
    assert_eq!(
        vec!["422 Unprocessable Entity", "200 OK"],
        statuses(&body),
        "{body}"
    );
    assert_eq!(
        Some("OperationOutcome"),
        body["entry"][0]["response"]["outcome"]["resourceType"].as_str(),
        "the failed entry carries its outcome: {body}"
    );
    let text = logs.text();
    let answered = text
        .lines()
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
        .find(|line| line["message"].as_str() == Some("a batch Bundle was answered"))
        .ok_or("the batch logged its answer")?;
    assert_eq!(Some(2), answered["entries"].as_u64(), "{answered}");
    assert_eq!(Some(1), answered["failed"].as_u64(), "{answered}");
    Ok(())
}
