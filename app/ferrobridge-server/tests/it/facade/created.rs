// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The resources an outbound run creates: a `reference` target and a further
//! `hierarchy.split` occurrence, each bound and served on its own.

use axum::body::Body;
use ferrobridge_server::facade::identity::FhirResourceId;
use ferrobridge_server::facade::identity::derive;
use ferrobridge_server::facade::identity::store::Store;
use fhirconnect::engine::seam::IdentityRequest;
use http::Request;
use http::StatusCode;
use http::header;
use std::error::Error as StdError;

use super::Harness;
use super::VERSION_ONE;
use super::call;
use super::harness_over;
use super::settings;
use super::stub::mount_create;
use super::stub::mount_ehr;
use super::stub::mount_read;
use super::stub::mount_version_read;

/// The fixture directory whose contexts create resources going out.
const CREATED: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/it/created");

/// The profile of the context whose `reference` mapping creates an
/// Observation.
const REFERENCE_PROFILE: &str =
    "http://example.org/fhir/StructureDefinition/ferrobridge-created-reference";

/// The profile of the context that splits each anatomical location into a
/// Condition of its own.
const SPLIT_PROFILE: &str = "http://example.org/fhir/StructureDefinition/ferrobridge-created-split";

/// Returns the synthetic Condition claiming `profile`, with `members` added.
fn condition_claiming(profile: &str, members: serde_json::Value) -> serde_json::Value {
    let mut resource: serde_json::Value =
        serde_json::from_str(ferrobridge_testkit::fixtures::R4_CONDITION)
            .expect("the synthetic Condition reads");
    resource["meta"] = serde_json::json!({ "profile": [profile] });
    if let (Some(object), serde_json::Value::Object(added)) = (resource.as_object_mut(), members) {
        object.extend(added);
    }
    resource
}

/// Returns the Condition whose evidence names a contained Observation.
fn with_evidence() -> serde_json::Value {
    condition_claiming(
        REFERENCE_PROFILE,
        serde_json::json!({
            "contained": [{
                "resourceType": "Observation",
                "id": "evidence",
                "status": "final",
                "code": {"text": "Synthetic evidence one"}
            }],
            "evidence": [{"detail": [{"reference": "#evidence"}]}]
        }),
    )
}

/// Returns the Condition with one `bodySite` per text.
fn with_sites(texts: &[&str]) -> serde_json::Value {
    let sites: Vec<serde_json::Value> = texts
        .iter()
        .map(|text| serde_json::json!({ "text": text }))
        .collect();
    condition_claiming(SPLIT_PROFILE, serde_json::json!({ "bodySite": sites }))
}

/// Returns the canonical composition `resource` maps to under `context`.
fn composition_under(
    harness: &Harness,
    context: &str,
    resource: &serde_json::Value,
) -> serde_json::Value {
    let loaded = harness
        .facade
        .programs()
        .by_context(context)
        .cloned()
        .expect("the fixture context compiled");
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

/// Creates `resource` over the stub CDR serving `composition`, and returns
/// the answered body.
async fn create(
    harness: &Harness,
    resource: &serde_json::Value,
    composition: &serde_json::Value,
) -> Result<serde_json::Value, Box<dyn StdError>> {
    mount_ehr(&harness.cdr).await;
    mount_create(&harness.cdr, VERSION_ONE, composition).await;
    mount_read(&harness.cdr, VERSION_ONE, composition).await;
    mount_version_read(&harness.cdr, VERSION_ONE, composition).await;
    let (status, body) = call(
        harness.app(),
        Request::post("/fhir/Condition")
            .header(header::CONTENT_TYPE, "application/fhir+json")
            .body(Body::from(resource.to_string()))?,
    )
    .await?;
    assert_eq!(StatusCode::CREATED, status, "{body}");
    Ok(body)
}

/// Returns the reference the first evidence detail of `condition` holds.
fn evidence_reference(condition: &serde_json::Value) -> Option<&str> {
    condition["evidence"][0]["detail"][0]["reference"].as_str()
}

/// Creates the Condition with evidence and returns the created Observation's
/// id with the parent's body.
async fn created_observation(
    harness: &Harness,
) -> Result<(String, serde_json::Value), Box<dyn StdError>> {
    let composition = composition_under(
        harness,
        "ferrobridge_created_reference.context",
        &with_evidence(),
    );
    let parent = create(harness, &with_evidence(), &composition).await?;
    let reference = evidence_reference(&parent)
        .ok_or("the parent references the created Observation")?
        .to_owned();
    let id = reference
        .strip_prefix("Observation/")
        .ok_or_else(|| format!("the reference {reference} names an Observation by type and id"))?;
    Ok((id.to_owned(), parent))
}

#[tokio::test]
async fn a_created_resource_is_referenced_by_type_and_id_and_never_contained()
-> Result<(), Box<dyn StdError>> {
    // R4 references.html §2.3.0.8: a contained resource has no independent
    // existence, so a resource served on its own is referenced literally.
    let harness = harness_over(CREATED, settings()).await;
    let (id, parent) = created_observation(&harness).await?;
    assert!(
        parent.get("contained").is_none(),
        "no created resource is contained: {parent}"
    );
    assert!(
        FhirResourceId::new(&id).is_ok(),
        "{id} is inside the R4 id grammar"
    );
    Ok(())
}

#[tokio::test]
async fn a_created_resource_answers_a_read_of_its_own() -> Result<(), Box<dyn StdError>> {
    // R4 http.html §read: GET [base]/[type]/[id] answers the resource.
    let harness = harness_over(CREATED, settings()).await;
    let (id, parent) = created_observation(&harness).await?;
    let (status, body) = call(
        harness.app(),
        Request::get(format!("/fhir/Observation/{id}")).body(Body::empty())?,
    )
    .await?;
    assert_eq!(StatusCode::OK, status, "{body}");
    assert_eq!(Some("Observation"), body["resourceType"].as_str());
    assert_eq!(Some(id.as_str()), body["id"].as_str());
    assert_eq!(Some("final"), body["status"].as_str());
    assert_eq!(
        Some("Synthetic evidence one"),
        body["code"]["text"].as_str()
    );
    assert_eq!(Some("1"), body["meta"]["versionId"].as_str());
    assert!(
        body["meta"].get("profile").is_none(),
        "a created resource claims no profile: {body}"
    );
    let binding = harness
        .store
        .binding_of("Observation", &FhirResourceId::new(&id)?)?
        .ok_or("the created Observation is bound")?;
    let created = binding.created.ok_or("the binding names its parent")?;
    assert_eq!("Condition", created.parent_type);
    assert_eq!(parent["id"].as_str(), Some(created.parent_id.as_str()));
    let (status, vread) = call(
        harness.app(),
        Request::get(format!("/fhir/Observation/{id}/_history/1")).body(Body::empty())?,
    )
    .await?;
    assert_eq!(StatusCode::OK, status, "{vread}");
    assert_eq!(Some(id.as_str()), vread["id"].as_str());
    Ok(())
}

#[tokio::test]
async fn a_re_run_derives_the_same_created_id() -> Result<(), Box<dyn StdError>> {
    // The derivation is a digest over the parent's entry and the request, so
    // a fresh identity map over the same composition names the same id.
    let first = harness_over(CREATED, settings()).await;
    let (id, parent) = created_observation(&first).await?;
    let (status, again) = call(
        first.app(),
        Request::get(format!(
            "/fhir/Condition/{}",
            parent["id"].as_str().ok_or("the parent has an id")?
        ))
        .body(Body::empty())?,
    )
    .await?;
    assert_eq!(StatusCode::OK, status, "{again}");
    assert_eq!(
        Some(format!("Observation/{id}").as_str()),
        evidence_reference(&again)
    );
    let second = harness_over(CREATED, settings()).await;
    let (rerun, _parent) = created_observation(&second).await?;
    assert_eq!(id, rerun);
    Ok(())
}

#[tokio::test]
async fn a_further_split_occurrence_answers_a_read_of_its_own() -> Result<(), Box<dyn StdError>> {
    // HierarchyMappings.adoc §split: each occurrence creates a resource, and
    // the further one is served at the id the identity map derived for it.
    let harness = harness_over(CREATED, settings()).await;
    let resource = with_sites(&["Left knee", "Right knee"]);
    let composition = composition_under(&harness, "ferrobridge_created_split.context", &resource);
    let parent = create(&harness, &resource, &composition).await?;
    assert_eq!(Some("Left knee"), parent["bodySite"][0]["text"].as_str());
    assert!(parent.get("contained").is_none(), "{parent}");
    let parent_id = FhirResourceId::new(parent["id"].as_str().ok_or("the parent has an id")?)?;
    let binding = harness
        .store
        .binding_of("Condition", &parent_id)?
        .ok_or("the parent is bound")?;
    let request = IdentityRequest::new("Condition", "Condition", "ferrobridge_created_split")
        .with_occurrence(vec![2]);
    let id = derive::created_digest(&derive::created_key(&binding, &request));
    let (status, body) = call(
        harness.app(),
        Request::get(format!("/fhir/Condition/{id}")).body(Body::empty())?,
    )
    .await?;
    assert_eq!(StatusCode::OK, status, "{body}");
    assert_eq!(Some(id.as_str()), body["id"].as_str());
    assert_eq!(Some("Right knee"), body["bodySite"][0]["text"].as_str());
    assert_eq!(
        Some("Synthetic problem one"),
        body["code"]["text"].as_str(),
        "the content outside the split reaches the created resource"
    );
    Ok(())
}

#[tokio::test]
async fn an_update_of_a_created_resource_is_refused() -> Result<(), Box<dyn StdError>> {
    // No specification governs this: our own design, a created resource is
    // content of its parent's entry, so only the parent is revised.
    let harness = harness_over(CREATED, settings()).await;
    let resource = with_sites(&["Left knee", "Right knee"]);
    let composition = composition_under(&harness, "ferrobridge_created_split.context", &resource);
    let parent = create(&harness, &resource, &composition).await?;
    let parent_id = FhirResourceId::new(parent["id"].as_str().ok_or("the parent has an id")?)?;
    let binding = harness
        .store
        .binding_of("Condition", &parent_id)?
        .ok_or("the parent is bound")?;
    let request = IdentityRequest::new("Condition", "Condition", "ferrobridge_created_split")
        .with_occurrence(vec![2]);
    let id = derive::created_digest(&derive::created_key(&binding, &request));
    let mut revised = with_sites(&["Right knee"]);
    revised["id"] = serde_json::json!(id.as_str());
    let (status, body) = call(
        harness.app(),
        Request::put(format!("/fhir/Condition/{id}"))
            .header(header::CONTENT_TYPE, "application/fhir+json")
            .body(Body::from(revised.to_string()))?,
    )
    .await?;
    assert_eq!(StatusCode::METHOD_NOT_ALLOWED, status, "{body}");
    assert_eq!(Some("not-supported"), body["issue"][0]["code"].as_str());
    Ok(())
}

#[tokio::test]
async fn a_type_only_a_reference_creates_is_declared_readable() -> Result<(), Box<dyn StdError>> {
    // R4 capabilitystatement.html: the statement names the interactions each
    // type supports, and a created Observation answers read and vread alone.
    let harness = harness_over(CREATED, settings()).await;
    let (status, body) = call(
        harness.app(),
        Request::get("/fhir/metadata").body(Body::empty())?,
    )
    .await?;
    assert_eq!(StatusCode::OK, status, "{body}");
    let resources = body["rest"][0]["resource"]
        .as_array()
        .ok_or("the statement lists resources")?;
    let types: Vec<&str> = resources
        .iter()
        .filter_map(|resource| resource["type"].as_str())
        .collect();
    assert_eq!(vec!["Condition", "Observation"], types);
    let observation = resources
        .iter()
        .find(|resource| resource["type"] == "Observation")
        .ok_or("Observation is listed")?;
    let interactions: Vec<&str> = observation["interaction"]
        .as_array()
        .ok_or("the entry lists interactions")?
        .iter()
        .filter_map(|interaction| interaction["code"].as_str())
        .collect();
    assert_eq!(vec!["read", "vread"], interactions);
    let (status, refused) = call(
        harness.app(),
        Request::post("/fhir/Observation")
            .header(header::CONTENT_TYPE, "application/fhir+json")
            .body(Body::from(
                serde_json::json!({"resourceType": "Observation", "status": "final", "code": {"text": "x"}})
                    .to_string(),
            ))?,
    )
    .await?;
    assert_eq!(StatusCode::NOT_FOUND, status, "{refused}");
    assert_eq!(Some("not-supported"), refused["issue"][0]["code"].as_str());
    Ok(())
}
