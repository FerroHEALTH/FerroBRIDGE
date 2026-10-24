// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! LINKED on the wire: a Condition whose `link` names an Encounter commits
//! the Encounter as a composition of its own first, and writes its `ehr:` URI
//! into the Condition composition's `LINK.target`
//! (`types-of-mappings/concept-type/concept-mappings.adoc`, §Linked mappings).

use std::error::Error;
use std::sync::Arc;
use std::sync::Mutex;
use std::time::Duration;

use axum::body::Body;
use ferrobridge_server::facade::Facade;
use ferrobridge_server::facade::identity::store::MemoryStore;
use ferrobridge_server::facade::programs;
use ferrobridge_server::facade::references::Policy;
use http::Request;
use http::StatusCode;
use http::header;
use wiremock::Mock;
use wiremock::MockServer;
use wiremock::Respond;
use wiremock::ResponseTemplate;
use wiremock::matchers;

use super::EHR_ID;
use super::Harness;
use super::call;
use super::client;
use super::handle;
use super::settings;
use super::stub::mount_ehr;

/// The fixture directory of the linking and the linked context.
const FIXTURES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/it/linked");

/// The profile the linking Condition claims.
const LINKING_PROFILE: &str =
    "http://example.org/fhir/StructureDefinition/ferrobridge-linked-diagnosis";

/// The profile the linked Encounter claims.
const LINKED_PROFILE: &str =
    "http://example.org/fhir/StructureDefinition/ferrobridge-linked-encounter";

/// The version containers the stub CDR assigns, in commit order.
const CONTAINERS: [&str; 2] = [
    "a1a1a1a1-0000-4000-8000-0000000000a1",
    "b2b2b2b2-0000-4000-8000-0000000000b2",
];

/// A composition create that answers each commit `201` with the next
/// container of [`CONTAINERS`], echoing the composition it was sent, and
/// the `fail_at`th commit, when set, `422`.
#[derive(Debug)]
struct Committing {
    taken: Mutex<usize>,
    fail_at: Option<usize>,
}

impl Respond for Committing {
    fn respond(&self, request: &wiremock::Request) -> ResponseTemplate {
        let mut taken = self
            .taken
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let index = *taken;
        *taken = taken.saturating_add(1);
        if self.fail_at == Some(index) {
            return ferrobridge_testkit::stubs::its_rest::unprocessable(
                "the composition breaks the template",
                &["/content[0]: synthetic validation error"],
            );
        }
        let container = CONTAINERS
            .get(index)
            .copied()
            .unwrap_or("c3c3c3c3-0000-4000-8000-0000000000c3");
        ResponseTemplate::new(201)
            .insert_header("ETag", format!("W/\"{container}::ferrobridge.test::1\""))
            .insert_header("Content-Type", "application/json")
            .set_body_bytes(request.body.clone())
    }
}

/// Returns a harness over [`FIXTURES`] whose facade fetches references from
/// `base`, and whose CDR commits as `committing` answers.
async fn linking(base: &str, committing: Committing) -> Result<Harness, Box<dyn Error>> {
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
    mount_ehr(&cdr).await;
    Mock::given(matchers::method("POST"))
        .and(matchers::path(format!("/ehr/{EHR_ID}/composition")))
        .respond_with(committing)
        .mount(&cdr)
        .await;
    let set = programs::read_set(std::path::Path::new(FIXTURES))?;
    let templates = programs::fetch_templates(&set, &client(&cdr)).await?;
    let compiled = programs::compile_set(&set, &templates)?;
    let store = Arc::new(MemoryStore::new());
    let policy = Policy::new(vec![base.parse()?], 2, Duration::from_secs(5))?;
    let facade =
        Facade::new(compiled, handle(&store), client(&cdr), settings()).with_references(policy);
    Ok(Harness {
        cdr,
        store,
        facade: Arc::new(facade),
    })
}

/// Returns a sending site that serves the synthetic Encounter claiming
/// `profiles`.
async fn sender(profiles: &[&str]) -> MockServer {
    let sender = MockServer::start().await;
    Mock::given(matchers::method("GET"))
        .and(matchers::path("/fhir/Encounter/synthetic-encounter-1"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("Content-Type", "application/fhir+json")
                .set_body_json(serde_json::json!({
                    "resourceType": "Encounter",
                    "id": "synthetic-encounter-1",
                    "meta": { "profile": profiles },
                    "status": "finished",
                    "class": {
                        "system": "http://terminology.hl7.org/CodeSystem/v3-ActCode",
                        "code": "AMB"
                    },
                    "subject": {
                        "identifier": {
                            "system": "http://example.org/fhir/sid/ferrobridge-subject",
                            "value": "synthetic-subject-0001"
                        }
                    },
                    "reasonCode": [{
                        "coding": [{
                            "system": "http://example.org/fhir/CodeSystem/synthetic-problems",
                            "code": "SYN-002",
                            "display": "Synthetic encounter reason"
                        }],
                        "text": "Synthetic encounter reason"
                    }]
                })),
        )
        .mount(&sender)
        .await;
    sender
}

/// Returns the linking Condition, whose `encounter` names the Encounter
/// `sender` serves.
fn condition(sender: &MockServer) -> serde_json::Value {
    let mut resource: serde_json::Value =
        serde_json::from_str(ferrobridge_testkit::fixtures::R4_CONDITION)
            .unwrap_or_else(|_| serde_json::json!({}));
    resource["meta"] = serde_json::json!({ "profile": [LINKING_PROFILE] });
    resource["encounter"] = serde_json::json!({
        "reference": format!("{}/fhir/Encounter/synthetic-encounter-1", sender.uri())
    });
    resource
}

/// Posts the linking Condition and returns the answer.
async fn post(
    harness: &Harness,
    resource: &serde_json::Value,
) -> Result<(StatusCode, serde_json::Value), Box<dyn Error>> {
    call(
        harness.app(),
        Request::post("/fhir/Condition")
            .header(header::CONTENT_TYPE, "application/fhir+json")
            .body(Body::from(resource.to_string()))?,
    )
    .await
}

/// Returns the bodies of the compositions the CDR was sent, in order.
async fn committed(harness: &Harness) -> Vec<serde_json::Value> {
    harness
        .cdr
        .received_requests()
        .await
        .unwrap_or_default()
        .iter()
        .filter(|request| {
            request.method.as_str() == "POST"
                && request.url.path() == format!("/ehr/{EHR_ID}/composition")
        })
        .filter_map(|request| serde_json::from_slice(&request.body).ok())
        .collect()
}

/// Returns the `LINK.target` values a composition's entry carries.
fn link_targets(composition: &serde_json::Value) -> Vec<String> {
    composition
        .pointer("/content/0/links")
        .and_then(serde_json::Value::as_array)
        .map(|links| {
            links
                .iter()
                .filter_map(|link| link.pointer("/target/value")?.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default()
}

#[tokio::test]
async fn a_linked_resource_is_committed_first_and_its_composition_linked()
-> Result<(), Box<dyn Error>> {
    // concept-mappings.adoc §Linked mappings: the context the Encounter's
    // profile names "is executed and saved as a separate composition", which
    // "is linked using the openEHR links"; LINK.target is a DV_EHR_URI naming
    // the versioned composition (RM 1.1.0 data_types.html §DV_EHR_URI).
    let sender = sender(&[LINKED_PROFILE]).await;
    let harness = linking(
        &format!("{}/fhir", sender.uri()),
        Committing {
            taken: Mutex::new(0),
            fail_at: None,
        },
    )
    .await?;
    let (status, body) = post(&harness, &condition(&sender)).await?;
    assert_eq!(StatusCode::CREATED, status, "{body}");
    let sent = committed(&harness).await;
    assert_eq!(2, sent.len(), "two compositions were committed: {sent:?}");
    let [ref encounter, ref linking] = *sent.as_slice() else {
        return Err("two compositions".into());
    };
    assert!(
        encounter.to_string().contains("Synthetic encounter reason"),
        "the Encounter's composition went first: {encounter}"
    );
    assert_eq!(link_targets(encounter), Vec::<String>::new());
    assert_eq!(
        link_targets(linking),
        [format!("ehr:/{EHR_ID}/compositions/{}", CONTAINERS[0])],
        "the Condition links the Encounter's composition"
    );
    Ok(())
}

#[tokio::test]
async fn a_linked_resource_no_context_maps_commits_one_composition_with_no_link()
-> Result<(), Box<dyn Error>> {
    // §Linked mappings: only "if a context mapping for this `meta.url` is
    // found" is a composition made; the loss is declared.
    let sender = sender(&["http://example.org/fhir/StructureDefinition/unmapped"]).await;
    let harness = linking(
        &format!("{}/fhir", sender.uri()),
        Committing {
            taken: Mutex::new(0),
            fail_at: None,
        },
    )
    .await?;
    let (status, body) = post(&harness, &condition(&sender)).await?;
    assert_eq!(StatusCode::CREATED, status, "{body}");
    let sent = committed(&harness).await;
    assert_eq!(1, sent.len(), "{sent:?}");
    assert!(
        sent.iter()
            .all(|composition| link_targets(composition).is_empty())
    );
    Ok(())
}

#[tokio::test]
async fn a_linking_commit_the_cdr_refuses_names_the_linked_composition_it_left()
-> Result<(), Box<dyn Error>> {
    // The two commits cannot be one CONTRIBUTION (ITS-REST 1.1.0 gives an
    // UPDATE_VERSION no uid), so a refused second commit reports the first.
    let sender = sender(&[LINKED_PROFILE]).await;
    let harness = linking(
        &format!("{}/fhir", sender.uri()),
        Committing {
            taken: Mutex::new(0),
            fail_at: Some(1),
        },
    )
    .await?;
    let (status, body) = post(&harness, &condition(&sender)).await?;
    assert_eq!(StatusCode::UNPROCESSABLE_ENTITY, status, "{body}");
    let issues = body["issue"].as_array().ok_or("an OperationOutcome")?;
    assert!(
        issues.iter().any(|issue| {
            issue["severity"] == "warning"
                && issue["diagnostics"].as_str().is_some_and(|text| {
                    text.contains(&format!("{}::ferrobridge.test::1", CONTAINERS[0]))
                })
        }),
        "the refusal names the committed linked composition: {body}"
    );
    Ok(())
}

#[tokio::test]
async fn a_transaction_entry_that_links_a_resource_is_refused_whole()
-> Result<(), Box<dyn Error>> {
    // R4 http.html §transaction: all or nothing in one CONTRIBUTION, and the
    // linked composition's id is the CDR's to assign at its commit, so the
    // link cannot be written inside one; the refusal names why.
    let sender = sender(&[LINKED_PROFILE]).await;
    let harness = linking(
        &format!("{}/fhir", sender.uri()),
        Committing {
            taken: Mutex::new(0),
            fail_at: None,
        },
    )
    .await?;
    let encounter = serde_json::json!({
        "resourceType": "Encounter",
        "id": "bundled-encounter-1",
        "meta": { "profile": [LINKED_PROFILE] },
        "status": "finished",
        "class": {"system": "http://terminology.hl7.org/CodeSystem/v3-ActCode", "code": "AMB"},
        "subject": {"identifier": {
            "system": "http://example.org/fhir/sid/ferrobridge-subject",
            "value": "synthetic-subject-0001"
        }},
        "reasonCode": [{ "text": "Synthetic encounter reason" }]
    });
    let mut linking_condition = condition(&sender);
    linking_condition["encounter"] =
        serde_json::json!({ "reference": "urn:uuid:6f1d0000-0000-4000-8000-0000000000e1" });
    let bundle = serde_json::json!({
        "resourceType": "Bundle",
        "type": "transaction",
        "entry": [
            { "fullUrl": "urn:uuid:6f1d0000-0000-4000-8000-0000000000e1", "resource": encounter,
              "request": { "method": "POST", "url": "Encounter" } },
            { "fullUrl": "urn:uuid:6f1d0000-0000-4000-8000-0000000000c1",
              "resource": linking_condition,
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
    assert_eq!(StatusCode::UNPROCESSABLE_ENTITY, status, "{body}");
    assert!(
        body.to_string().contains("one CONTRIBUTION"),
        "the refusal says why: {body}"
    );
    let issues = body["issue"].as_array().ok_or("an OperationOutcome")?;
    assert!(
        issues.iter().any(|issue| {
            issue["code"] == "not-supported"
                && issue["diagnostics"]
                    .as_str()
                    .is_some_and(|text| text.contains("as an entry of a batch"))
        }),
        "the refusal names the way that commits: {body}"
    );
    assert!(
        committed(&harness).await.is_empty(),
        "nothing was committed"
    );
    Ok(())
}

#[tokio::test]
async fn a_linked_resource_no_context_maps_is_a_loss_the_writer_is_shown()
-> Result<(), Box<dyn StdError>> {
    // R4 http.html §Managing Return Content: `return=OperationOutcome` asks for
    // "hints and warnings", and the unmapped link is the run's declared loss.
    let sender = sender(&["http://example.org/fhir/StructureDefinition/unmapped"]).await;
    let harness = linking(
        &format!("{}/fhir", sender.uri()),
        Committing {
            taken: Mutex::new(0),
            fail_at: None,
        },
    )
    .await?;
    let (status, body) = call(
        harness.app(),
        Request::post("/fhir/Condition")
            .header(header::CONTENT_TYPE, "application/fhir+json")
            .header("Prefer", "return=OperationOutcome")
            .body(Body::from(condition(&sender).to_string()))?,
    )
    .await?;
    assert_eq!(StatusCode::CREATED, status, "{body}");
    let issues = body["issue"].as_array().ok_or("an OperationOutcome")?;
    assert!(
        issues
            .iter()
            .any(|issue| issue["severity"] == "warning" && issue["code"] == "incomplete"),
        "the unmapped link is named as a warning: {body}"
    );
    Ok(())
}

#[tokio::test]
async fn a_batch_entry_that_links_a_resource_commits_both_compositions()
-> Result<(), Box<dyn StdError>> {
    // R4 http.html §batch: "each entry is treated as if an individual
    // interaction or operation", so the entry runs the single create, which
    // commits the linked composition first and links it.
    let sender = sender(&[LINKED_PROFILE]).await;
    let harness = linking(
        &format!("{}/fhir", sender.uri()),
        Committing {
            taken: Mutex::new(0),
            fail_at: None,
        },
    )
    .await?;
    let bundle = serde_json::json!({
        "resourceType": "Bundle",
        "type": "batch",
        "entry": [{
            "resource": condition(&sender),
            "request": { "method": "POST", "url": "Condition" }
        }]
    });
    let (status, body) = call(
        harness.app(),
        Request::post("/fhir")
            .header(header::CONTENT_TYPE, "application/fhir+json")
            .body(Body::from(bundle.to_string()))?,
    )
    .await?;
    assert_eq!(StatusCode::OK, status, "{body}");
    assert_eq!(
        Some("201 Created"),
        body["entry"][0]["response"]["status"].as_str(),
        "{body}"
    );
    let sent = committed(&harness).await;
    let [_, ref linking] = *sent.as_slice() else {
        return Err(format!("two compositions: {sent:?}").into());
    };
    assert_eq!(
        link_targets(linking),
        [format!("ehr:/{EHR_ID}/compositions/{}", CONTAINERS[0])]
    );
    Ok(())
}
