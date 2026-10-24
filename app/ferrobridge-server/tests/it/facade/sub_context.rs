// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Sub-contexts through the facade: only root contexts are loaded as programs
//! and split a Bundle, and a resource a parent pulls in through `slotContext`
//! is not processed again as a root
//! (`docs/specs/fhirconnect/modules/ROOT/pages/engine/bundles.adoc`).

use std::error::Error;

use http::StatusCode;

use super::BASE_URL;
use super::EHR_ID;
use super::Harness;
use super::harness_over;
use super::post_transaction;
use super::settings;
use super::stub::ECHOED;
use super::stub::Echo;
use super::stub::mount_echo;
use super::stub::mount_ehr;

/// The mapping set of this suite: a root context and the sub-context it
/// enters for the evidence Observation.
const FIXTURES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/it/fixtures_sub_context");

/// The profile the root context maps.
const PROFILE: &str = "http://example.org/fhir/StructureDefinition/ferrobridge-sub-parent";

/// The `fullUrl` the evidence Observation takes in the Bundle.
const EVIDENCE: &str = "urn:uuid:5f0e7d3a-0000-4000-8000-0000000000e1";

/// Returns a harness over this suite's mapping set.
async fn harness() -> Harness {
    harness_over(FIXTURES, settings()).await
}

/// The synthetic Condition claiming the root profile, its evidence naming
/// the Observation entry when `evidence` is set.
fn condition(evidence: bool) -> serde_json::Value {
    let mut resource: serde_json::Value =
        serde_json::from_str(ferrobridge_testkit::fixtures::R4_CONDITION)
            .expect("the synthetic Condition reads");
    resource["meta"] = serde_json::json!({ "profile": [PROFILE] });
    if evidence {
        resource["evidence"] = serde_json::json!([{ "detail": [{ "reference": EVIDENCE }] }]);
    }
    resource
}

/// The synthetic evidence Observation.
fn observation() -> serde_json::Value {
    serde_json::json!({
        "resourceType": "Observation",
        "status": "final",
        "code": { "coding": [{
            "system": "http://example.org/fhir/CodeSystem/synthetic",
            "code": "s-1",
            "display": "Synthetic finding"
        }] }
    })
}

/// A transaction of the Condition and the Observation, in that order.
fn bundle(evidence: bool) -> serde_json::Value {
    serde_json::json!({
        "resourceType": "Bundle",
        "type": "transaction",
        "entry": [
            { "fullUrl": "urn:uuid:5f0e7d3a-0000-4000-8000-0000000000c1", "resource": condition(evidence),
              "request": { "method": "POST", "url": "Condition" } },
            { "fullUrl": EVIDENCE, "resource": observation(),
              "request": { "method": "POST", "url": "Observation" } }
        ]
    })
}

#[tokio::test]
async fn a_sub_context_is_no_program_and_its_template_is_never_fetched() {
    // bundles.adoc: "Only root contexts (`scope: root`, the default) take part in this. A `scope:
    // sub` context is entered via `slotContext` only". The stub serves no template of the
    // sub-context, so the harness loads only because none is fetched.
    let harness = harness().await;
    let programs = harness.facade.programs();
    let contexts: Vec<&str> = programs
        .loaded()
        .iter()
        .map(|loaded| loaded.program().context().as_str())
        .collect();
    assert_eq!(contexts, ["ferrobridge_sub_parent.context"]);
    assert!(
        !programs.supports("Observation"),
        "the sub-context's start resource is no root type"
    );
}

#[tokio::test]
async fn a_resource_a_sub_context_consumed_is_not_processed_again_as_a_root()
-> Result<(), Box<dyn Error>> {
    // bundles.adoc §Resources consumed by a sub-context: "A resource that a parent context pulls
    // in via `slotContext` is marked as resolved. The engine does not process it again as a
    // root". No program maps an Observation, so a transaction would refuse it as a root.
    let harness = harness().await;
    mount_ehr(&harness.cdr).await;
    mount_echo(&harness.cdr, ECHOED, Echo::Sent).await;
    let (status, body) = post_transaction(&harness, &bundle(true)).await?;
    assert_eq!(StatusCode::OK, status, "{body}");
    let entries = body["entry"].as_array().ok_or("the response has entries")?;
    assert_eq!(
        entries.len(),
        2,
        "one response entry per request entry: {body}"
    );
    let parent = &entries[0]["response"];
    assert_eq!(Some("201 Created"), parent["status"].as_str(), "{body}");
    assert!(
        parent["location"]
            .as_str()
            .is_some_and(|location| location.starts_with(&format!("{BASE_URL}/Condition/"))),
        "{body}"
    );
    let consumed = &entries[1]["response"];
    assert_eq!(Some("200 OK"), consumed["status"].as_str(), "{body}");
    assert!(consumed["location"].is_null(), "{body}");
    assert_eq!(
        1,
        harness
            .received("POST", &format!("/ehr/{EHR_ID}/contribution"))
            .await,
        "the Bundle commits one contribution"
    );
    let sent = harness
        .sent("POST", &format!("/ehr/{EHR_ID}/contribution"))
        .await
        .ok_or("the contribution was sent")?;
    let versions = sent["versions"].as_array().ok_or("the versions")?;
    assert_eq!(versions.len(), 1, "one composition, the parent's: {sent}");
    assert!(
        sent.to_string().contains("s-1"),
        "the consumed Observation's code is in the parent composition"
    );
    Ok(())
}

#[tokio::test]
async fn a_resource_nothing_consumes_is_still_processed_as_a_root() -> Result<(), Box<dyn Error>> {
    // bundles.adoc: "Resources nothing consumes are still processed as roots", so the
    // Observation no program maps refuses the transaction.
    let harness = harness().await;
    mount_ehr(&harness.cdr).await;
    mount_echo(&harness.cdr, ECHOED, Echo::Sent).await;
    let (status, body) = post_transaction(&harness, &bundle(false)).await?;
    assert_eq!(StatusCode::UNPROCESSABLE_ENTITY, status, "{body}");
    assert!(body.to_string().contains(EVIDENCE), "{body}");
    Ok(())
}
