// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! A reference the request does not carry: read from this server, fetched
//! from an allow-listed base, or a declared skip.

use axum::body::Body;
use ferrobridge_server::facade::Facade;
use ferrobridge_server::facade::identity::store::MemoryStore;
use ferrobridge_server::facade::programs;
use ferrobridge_server::facade::references::Policy;
use http::Request;
use http::StatusCode;
use http::header;
use std::error::Error;
use std::sync::Arc;
use std::time::Duration;
use wiremock::Mock;
use wiremock::MockServer;
use wiremock::ResponseTemplate;
use wiremock::matchers;

use super::EHR_ID;
use super::Harness;
use super::VERSION_ONE;
use super::call;
use super::client;
use super::handle;
use super::settings;
use super::stub::ECHOED;
use super::stub::Echo;
use super::stub::mount_create;
use super::stub::mount_echo;
use super::stub::mount_ehr;
use super::stub::mount_read;

/// The fixture directory whose reference context reads an Observation.
const CREATED: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/it/created");

/// The fixture directory that maps the Observation a Condition's evidence
/// references as well, so one transaction carries both.
const BUNDLED: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/it/bundle_references");

/// The profile of the Observation context in [`BUNDLED`].
const OBSERVATION_PROFILE: &str =
    "http://example.org/fhir/StructureDefinition/ferrobridge-bundle-observation";

#[tokio::test]
async fn a_transaction_entry_answers_a_reference_to_it_by_full_url_and_by_type_and_id()
-> Result<(), Box<dyn Error>> {
    // engine/references.adoc: a reference is fetched only when it is "not
    // contained in the Bundle"; R4 bundle.html §references: an entry is
    // referenced by its fullUrl or, relative to the base, by type and id.
    for (full_url, reference) in [
        ("urn:uuid:5e3c0000-0000-4000-8000-000000000001", None),
        (
            "urn:uuid:5e3c0000-0000-4000-8000-000000000002",
            Some("Observation/bundled-obs-1"),
        ),
    ] {
        let harness = referencing_over(BUNDLED, Policy::default()).await;
        mount_ehr(&harness.cdr).await;
        mount_echo(&harness.cdr, ECHOED, Echo::Sent).await;
        let mut evidence = observation("bundled-obs-1", "Bundle evidence one");
        evidence["meta"] = serde_json::json!({ "profile": [OBSERVATION_PROFILE] });
        evidence["subject"] = serde_json::json!({
            "identifier": {
                "system": "http://example.org/fhir/sid/ferrobridge-subject",
                "value": "synthetic-subject-0001"
            }
        });
        let condition = citing(&[reference.unwrap_or(full_url)]);
        let bundle = serde_json::json!({
            "resourceType": "Bundle",
            "type": "transaction",
            "entry": [
                { "fullUrl": full_url, "resource": evidence,
                  "request": { "method": "POST", "url": "Observation" } },
                { "fullUrl": "urn:uuid:5e3c0000-0000-4000-8000-0000000000c1",
                  "resource": condition,
                  "request": { "method": "POST", "url": "Condition" } }
            ]
        });
        let (status, body) = call(
            harness.app(),
            Request::post("/fhir")
                .header(header::CONTENT_TYPE, "application/fhir+json")
                .body(Body::from(bundle.to_string()))?,
        )
        .await?;
        assert!(status.is_success(), "{status}: {body}");
        let sent = harness
            .sent("POST", &format!("/ehr/{EHR_ID}/contribution"))
            .await
            .ok_or("the CDR received the contribution")?;
        assert_eq!(
            2,
            sent.to_string().matches("Bundle evidence one").count(),
            "the Observation's own name and the Condition's comment both carry it"
        );
    }
    Ok(())
}

/// The profile of the context whose `reference` mapping reads an
/// Observation's code into the problem-diagnosis comment.
const REFERENCE_PROFILE: &str =
    "http://example.org/fhir/StructureDefinition/ferrobridge-created-reference";

/// The context of that profile.
const REFERENCE_CONTEXT: &str = "ferrobridge_created_reference.context";

/// Returns a harness over [`CREATED`] whose facade resolves references under
/// `policy`.
async fn referencing(policy: Policy) -> Harness {
    referencing_over(CREATED, policy).await
}

/// Returns a harness over the mapping set under `fixtures` whose facade
/// resolves references under `policy`.
async fn referencing_over(fixtures: &str, policy: Policy) -> Harness {
    let cdr = MockServer::start().await;
    Mock::given(matchers::method("GET"))
        .and(matchers::path(
            "/definition/template/adl1.4/ferrobridge.diagnose.v1",
        ))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("Content-Type", "application/xml")
                .set_body_string(ferrobridge_testkit::fixtures::DIAGNOSE_OPT),
        )
        .mount(&cdr)
        .await;
    let set = programs::read_set(std::path::Path::new(fixtures)).expect("the mapping set loads");
    let templates = programs::fetch_templates(&set, &client(&cdr))
        .await
        .expect("the template fetch answers");
    let compiled = programs::compile_set(&set, &templates).expect("the mapping set compiles");
    let store = Arc::new(MemoryStore::new());
    let facade =
        Facade::new(compiled, handle(&store), client(&cdr), settings()).with_references(policy);
    Harness {
        cdr,
        store,
        facade: Arc::new(facade),
    }
}

/// Returns the policy that fetches from `base` to `max_depth` levels.
fn allowing(base: &str, max_depth: u32) -> Policy {
    Policy::new(
        vec![base.parse().expect("a legal base")],
        max_depth,
        Duration::from_secs(5),
    )
    .expect("the client builds")
}

/// Returns the Condition of the reference profile with `members` added.
fn condition_with(members: serde_json::Value) -> serde_json::Value {
    let mut resource: serde_json::Value =
        serde_json::from_str(ferrobridge_testkit::fixtures::R4_CONDITION)
            .expect("the synthetic Condition reads");
    resource["id"] = serde_json::json!("ferrobridge-synthetic-condition-2");
    resource["meta"] = serde_json::json!({ "profile": [REFERENCE_PROFILE] });
    if let (Some(object), serde_json::Value::Object(added)) = (resource.as_object_mut(), members) {
        object.extend(added);
    }
    resource
}

/// Returns the Condition whose evidence names `reference`.
fn citing(references: &[&str]) -> serde_json::Value {
    let details: Vec<serde_json::Value> = references
        .iter()
        .map(|reference| serde_json::json!({ "reference": reference }))
        .collect();
    let mut resource = condition_with(serde_json::json!({ "evidence": [{ "detail": details }] }));
    resource["id"] = serde_json::json!("ferrobridge-synthetic-condition-3");
    resource
}

/// Returns a synthetic Observation with `id` and the code text `text`.
fn observation(id: &str, text: &str) -> serde_json::Value {
    serde_json::json!({
        "resourceType": "Observation",
        "id": id,
        "status": "final",
        "code": { "text": text }
    })
}

/// Returns the canonical composition `resource` maps to under the reference
/// context, with no reference resolved beyond what it contains.
fn composition_of(harness: &Harness, resource: &serde_json::Value) -> serde_json::Value {
    let programs = harness.facade.programs();
    let loaded = programs
        .by_context(REFERENCE_CONTEXT)
        .expect("the reference context compiled");
    let document: fhir_types::codec::Value =
        serde_json::from_value(resource.clone()).expect("the Condition reads as a document");
    ferrobridge_server::facade::engine::inbound(
        loaded.program(),
        loaded.index(),
        &document,
        "2026-09-15T09:00:00Z",
        &settings(),
        &fhirconnect::engine::terminology::NoTerminology,
        &fhirconnect::engine::demographics::IssuerDemographics::new(crate::facade::BASE_URL)
            .expect("the test base issues identities"),
    )
    .expect("the Condition maps")
    .into_value()
    .into_value()
}

/// Posts `resource` as a create over a CDR that answers with `stored`, and
/// returns the status and the composition the CDR was sent.
async fn post(
    harness: &Harness,
    resource: &serde_json::Value,
    stored: &serde_json::Value,
) -> Result<(StatusCode, Option<serde_json::Value>), Box<dyn Error>> {
    mount_ehr(&harness.cdr).await;
    mount_create(&harness.cdr, VERSION_ONE, stored).await;
    let (status, body) = call(
        harness.app(),
        Request::post("/fhir/Condition")
            .header(header::CONTENT_TYPE, "application/fhir+json")
            .body(Body::from(resource.to_string()))?,
    )
    .await?;
    assert!(
        status == StatusCode::CREATED || status == StatusCode::OK,
        "{status}: {body}"
    );
    let sent = harness
        .sent("POST", &format!("/ehr/{EHR_ID}/composition"))
        .await;
    Ok((status, sent))
}

/// Returns the problem-diagnosis comment text a sent composition carries.
fn comment(composition: &serde_json::Value) -> Option<String> {
    let text = composition.to_string();
    ["Synthetic evidence one", "Remote evidence one"]
        .into_iter()
        .find(|marker| text.contains(marker))
        .map(String::from)
}

/// Returns how many requests `server` received.
async fn received(server: &MockServer) -> usize {
    server.received_requests().await.unwrap_or_default().len()
}

#[tokio::test]
async fn a_relative_reference_to_a_committed_resource_is_read_from_this_server()
-> Result<(), Box<dyn Error>> {
    // R4 references.html §2.3.0.2: a relative reference is relative to the
    // service base, so it names a resource this facade committed.
    let harness = referencing(Policy::default()).await;
    let first = condition_with(serde_json::json!({
        "contained": [observation("evidence", "Synthetic evidence one")],
        "evidence": [{ "detail": [{ "reference": "#evidence" }] }]
    }));
    let stored = composition_of(&harness, &first);
    let created = created_observation(&harness, &stored).await?;
    let (_, sent) = post(
        &harness,
        &citing(&[&format!("Observation/{created}")]),
        &stored,
    )
    .await?;
    let sent = sent.ok_or("the CDR received the composition")?;
    assert_eq!(
        Some(String::from("Synthetic evidence one")),
        comment(&sent),
        "the committed Observation's code reached the composition"
    );
    Ok(())
}

/// Returns the id of the Observation the outbound mapping of `stored`
/// creates, as the read of its parent names it.
async fn created_observation(
    harness: &Harness,
    stored: &serde_json::Value,
) -> Result<String, Box<dyn Error>> {
    mount_ehr(&harness.cdr).await;
    mount_create(&harness.cdr, VERSION_ONE, stored).await;
    mount_read(&harness.cdr, VERSION_ONE, stored).await;
    let (status, body) = call(
        harness.app(),
        Request::post("/fhir/Condition")
            .header(header::CONTENT_TYPE, "application/fhir+json")
            .body(Body::from(
                condition_with(serde_json::json!({
                    "contained": [observation("evidence", "Synthetic evidence one")],
                    "evidence": [{ "detail": [{ "reference": "#evidence" }] }]
                }))
                .to_string(),
            ))?,
    )
    .await?;
    assert!(status.is_success(), "{status}: {body}");
    let reference = body["evidence"][0]["detail"][0]["reference"]
        .as_str()
        .ok_or("the parent references the created Observation")?;
    Ok(reference
        .strip_prefix("Observation/")
        .ok_or("an Observation by type and id")?
        .to_owned())
}

#[tokio::test]
async fn an_absolute_reference_off_the_allow_list_is_never_fetched() -> Result<(), Box<dyn Error>> {
    // engine/references.adoc: "If not possible, the engine proceeds with the
    // mapping"; no specification governs the allow-list: our own design.
    let sender = MockServer::start().await;
    let harness = referencing(Policy::default()).await;
    let resource = citing(&[&format!("{}/fhir/Observation/obs-1", sender.uri())]);
    let stored = composition_of(&harness, &resource);
    let (_, sent) = post(&harness, &resource, &stored).await?;
    assert_eq!(0, received(&sender).await, "nothing was fetched");
    assert_eq!(
        None,
        comment(&sent.ok_or("the CDR received the composition")?)
    );
    Ok(())
}

#[tokio::test]
async fn an_absolute_reference_on_the_allow_list_is_fetched_once() -> Result<(), Box<dyn Error>> {
    // engine/references.adoc: "the engine should attempt to GET it from the
    // sending site", and "keep track of which ones are already resolved".
    let sender = MockServer::start().await;
    Mock::given(matchers::method("GET"))
        .and(matchers::path("/fhir/Observation/obs-1"))
        .and(matchers::header("accept", "application/fhir+json"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(observation("obs-1", "Remote evidence one")),
        )
        .mount(&sender)
        .await;
    let base = format!("{}/fhir", sender.uri());
    let harness = referencing(allowing(&base, 2)).await;
    let literal = format!("{base}/Observation/obs-1");
    let resource = citing(&[&literal, &literal]);
    let stored = composition_of(&harness, &resource);
    let (_, sent) = post(&harness, &resource, &stored).await?;
    assert_eq!(1, received(&sender).await, "one literal is fetched once");
    assert_eq!(
        Some(String::from("Remote evidence one")),
        comment(&sent.ok_or("the CDR received the composition")?)
    );
    Ok(())
}

#[tokio::test]
async fn a_depth_of_zero_fetches_nothing() -> Result<(), Box<dyn Error>> {
    let sender = MockServer::start().await;
    let base = format!("{}/fhir", sender.uri());
    let harness = referencing(allowing(&base, 0)).await;
    let resource = citing(&[&format!("{base}/Observation/obs-1")]);
    let stored = composition_of(&harness, &resource);
    let (_, _) = post(&harness, &resource, &stored).await?;
    assert_eq!(0, received(&sender).await);
    Ok(())
}

/// Returns whether one issue of `outcome` has `severity` and `code` and
/// names every word of `words` in its diagnostics.
fn names(outcome: &serde_json::Value, severity: &str, code: &str, words: &[&str]) -> bool {
    outcome["issue"].as_array().is_some_and(|issues| {
        issues.iter().any(|issue| {
            issue["severity"] == severity
                && issue["code"] == code
                && issue["diagnostics"]
                    .as_str()
                    .is_some_and(|text| words.iter().all(|word| text.contains(word)))
        })
    })
}

/// The status and body of one answer.
type Answer = (StatusCode, serde_json::Value);

/// Posts `resource` as a create and as a `$validate`, and returns both
/// answers.
async fn create_and_validate(
    harness: &Harness,
    resource: &serde_json::Value,
) -> Result<(Answer, Answer), Box<dyn Error>> {
    mount_ehr(&harness.cdr).await;
    let created = call(
        harness.app(),
        Request::post("/fhir/Condition")
            .header(header::CONTENT_TYPE, "application/fhir+json")
            .body(Body::from(resource.to_string()))?,
    )
    .await?;
    let validated = call(
        harness.app(),
        Request::post("/fhir/Condition/$validate")
            .header(header::CONTENT_TYPE, "application/fhir+json")
            .body(Body::from(resource.to_string()))?,
    )
    .await?;
    Ok((created, validated))
}

#[tokio::test]
async fn a_failed_fetch_refuses_the_request_with_the_upstream_status() -> Result<(), Box<dyn Error>>
{
    // engine/Fail.adoc: "either all data should be mapped, or the mapping
    // should fail"; a sending site that fails is an upstream failure, never an
    // absence, and answers the facade's UPSTREAM row (R4 http.html, 502).
    let sender = MockServer::start().await;
    Mock::given(matchers::method("GET"))
        .respond_with(ResponseTemplate::new(503))
        .mount(&sender)
        .await;
    let base = format!("{}/fhir", sender.uri());
    let harness = referencing(allowing(&base, 2)).await;
    let resource = citing(&[&format!("{base}/Observation/obs-1")]);
    let ((status, outcome), (validated, dry)) = create_and_validate(&harness, &resource).await?;
    assert_eq!(StatusCode::BAD_GATEWAY, status, "{outcome}");
    assert!(
        names(
            &outcome,
            "error",
            "transient",
            &["503", "Observation/obs-1"]
        ),
        "the refusal names the reference and the status: {outcome}"
    );
    assert_eq!(
        None,
        harness
            .sent("POST", &format!("/ehr/{EHR_ID}/composition"))
            .await,
        "nothing was committed"
    );
    assert_eq!(StatusCode::BAD_GATEWAY, validated, "{dry}");
    assert!(
        names(&dry, "error", "transient", &["503", "Observation/obs-1"]),
        "{dry}"
    );
    Ok(())
}

#[tokio::test]
async fn a_body_that_is_not_the_referenced_resource_refuses_the_request()
-> Result<(), Box<dyn Error>> {
    let sender = MockServer::start().await;
    Mock::given(matchers::method("GET"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(observation("obs-other", "Remote evidence one")),
        )
        .mount(&sender)
        .await;
    let base = format!("{}/fhir", sender.uri());
    let harness = referencing(allowing(&base, 2)).await;
    let resource = citing(&[&format!("{base}/Observation/obs-1")]);
    let ((status, outcome), _) = create_and_validate(&harness, &resource).await?;
    assert_eq!(StatusCode::BAD_GATEWAY, status, "{outcome}");
    assert!(
        names(&outcome, "error", "transient", &["Observation/obs-1"]),
        "{outcome}"
    );
    Ok(())
}

#[tokio::test]
async fn a_resource_the_sending_site_does_not_hold_is_a_skip_that_names_the_status()
-> Result<(), Box<dyn Error>> {
    // engine/references.adoc: "If not possible, the engine proceeds with the
    // mapping"; a 404 says the resource is absent (R4 http.html §read).
    let sender = MockServer::start().await;
    Mock::given(matchers::method("GET"))
        .respond_with(ResponseTemplate::new(404))
        .mount(&sender)
        .await;
    let base = format!("{}/fhir", sender.uri());
    let harness = referencing(allowing(&base, 2)).await;
    let resource = citing(&[&format!("{base}/Observation/obs-1")]);
    let stored = composition_of(&harness, &resource);
    let (status, sent) = post(&harness, &resource, &stored).await?;
    assert_eq!(StatusCode::CREATED, status, "the mapping proceeds");
    assert_eq!(
        None,
        comment(&sent.ok_or("the CDR received the composition")?)
    );
    let (status, outcome) = call(
        harness.app(),
        Request::post("/fhir/Condition/$validate")
            .header(header::CONTENT_TYPE, "application/fhir+json")
            .body(Body::from(resource.to_string()))?,
    )
    .await?;
    assert_eq!(StatusCode::OK, status, "{outcome}");
    assert!(
        names(
            &outcome,
            "warning",
            "not-found",
            &["404", "Observation/obs-1"]
        ),
        "the skip names the reference and the status: {outcome}"
    );
    Ok(())
}

#[tokio::test]
async fn a_transaction_whose_reference_fetch_fails_is_refused_whole_with_502()
-> Result<(), Box<dyn Error>> {
    let sender = MockServer::start().await;
    Mock::given(matchers::method("GET"))
        .respond_with(ResponseTemplate::new(500))
        .mount(&sender)
        .await;
    let base = format!("{}/fhir", sender.uri());
    let harness = referencing(allowing(&base, 2)).await;
    mount_ehr(&harness.cdr).await;
    let bundle = serde_json::json!({
        "resourceType": "Bundle",
        "type": "transaction",
        "entry": [
            { "fullUrl": "urn:uuid:5e3c0000-0000-4000-8000-0000000000c2",
              "resource": citing(&[&format!("{base}/Observation/obs-1")]),
              "request": { "method": "POST", "url": "Condition" } }
        ]
    });
    let (status, outcome) = call(
        harness.app(),
        Request::post("/fhir")
            .header(header::CONTENT_TYPE, "application/fhir+json")
            .body(Body::from(bundle.to_string()))?,
    )
    .await?;
    assert_eq!(StatusCode::BAD_GATEWAY, status, "{outcome}");
    assert!(
        names(
            &outcome,
            "error",
            "transient",
            &["500", "Observation/obs-1"]
        ),
        "{outcome}"
    );
    assert_eq!(
        None,
        harness
            .sent("POST", &format!("/ehr/{EHR_ID}/contribution"))
            .await,
        "nothing was committed"
    );
    Ok(())
}

#[tokio::test]
async fn a_reference_cycle_fetches_each_literal_once() -> Result<(), Box<dyn Error>> {
    // engine/references.adoc: the engine "should either stop at a given level
    // of references or keep track of which ones are already resolved"; both
    // hold here, so a cycle inside the depth ends at its first repeat.
    let sender = MockServer::start().await;
    let base = format!("{}/fhir", sender.uri());
    for (id, member) in [("obs-1", "obs-2"), ("obs-2", "obs-1")] {
        let mut found = observation(id, "Remote evidence one");
        found["hasMember"] =
            serde_json::json!([{ "reference": format!("{base}/Observation/{member}") }]);
        Mock::given(matchers::method("GET"))
            .and(matchers::path(format!("/fhir/Observation/{id}")))
            .respond_with(ResponseTemplate::new(200).set_body_json(found))
            .mount(&sender)
            .await;
    }
    let harness = referencing(allowing(&base, 8)).await;
    let resource = citing(&[&format!("{base}/Observation/obs-1")]);
    let stored = composition_of(&harness, &resource);
    let (status, _) = post(&harness, &resource, &stored).await?;
    assert_eq!(StatusCode::CREATED, status);
    assert_eq!(2, received(&sender).await, "each literal of the cycle once");
    Ok(())
}
