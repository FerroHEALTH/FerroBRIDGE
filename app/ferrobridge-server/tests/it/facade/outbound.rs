// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The outbound FHIR lane: each changed composition reaches a `wiremock`
//! FHIR endpoint as one R4 `transaction` Bundle
//! (<https://hl7.org/fhir/R4/http.html#transaction>), each resource `PUT` by
//! its id with `meta.versionId` and `meta.source`; a deletion and an erasure
//! `DELETE` what the composition was delivered as; a resource without a
//! subject is refused as poisoned; and a refused delivery leaves the event
//! unfinished.

use std::error::Error;
use std::time::Duration;

use ferrobridge_server::cdr::ids::EhrId;
use ferrobridge_server::change_feed::ChangedVersion;
use ferrobridge_server::change_feed::Domain;
use ferrobridge_server::change_feed::Event;
use ferrobridge_server::change_feed::Position;
use ferrobridge_server::change_feed::Seq;
use ferrobridge_server::change_feed::follow::Consumer;
use ferrobridge_server::change_feed::follow::Failure;
use ferrobridge_server::outbound::Outbound;
use ferrobridge_server::outbound::Settings;
use ferrobridge_server::outbound::sink::Credentials;
use ferrobridge_server::outbound::sink::Sink;
use wiremock::Mock;
use wiremock::MockServer;
use wiremock::ResponseTemplate;
use wiremock::matchers;

use super::CONTAINER;
use super::EHR_ID;
use super::Harness;
use super::VERSION_ONE;
use super::create_one;
use super::harness;
use super::stub::mount_read;

/// One case: the facade, the lane and the endpoint.
struct Lane {
    /// The facade over the stub CDR.
    harness: Harness,
    /// The FHIR endpoint.
    endpoint: MockServer,
    /// The lane.
    outbound: Outbound,
    /// The directory of the delivery record, kept for the case.
    _scratch: tempfile::TempDir,
}

/// Returns a lane over `harness` delivering to an endpoint that answers
/// `status`.
async fn lane(harness: Harness, status: u16) -> Result<Lane, Box<dyn Error>> {
    let endpoint = MockServer::start().await;
    Mock::given(matchers::method("POST"))
        .and(matchers::path("/fhir"))
        .respond_with(ResponseTemplate::new(status))
        .mount(&endpoint)
        .await;
    let scratch = tempfile::tempdir()?;
    let settings = Settings {
        reader: ferrobridge_server::change_feed::ReaderName::new("fhir-outbound")?,
        sink: Sink::new(
            format!("{}/fhir", endpoint.uri()).parse()?,
            Duration::from_secs(5),
            Credentials::None,
        )?,
        emitted: scratch.path().join("emitted.redb"),
    };
    let outbound = Outbound::start(
        &settings,
        std::sync::Arc::clone(&harness.facade),
        harness.facade.client().clone(),
    )?;
    Ok(Lane {
        harness,
        endpoint,
        outbound,
        _scratch: scratch,
    })
}

/// Returns the event that changed [`CONTAINER`] in [`EHR_ID`], an erasure
/// when `erasure` is set.
fn event(seq: u64, change_type: &str, erasure: bool) -> Result<Event, Box<dyn Error>> {
    Ok(Event {
        position: Position::Outbox {
            domain: Domain::Clinical,
            seq: Seq::new(seq),
        },
        erasure,
        contribution_id: None,
        ehr_id: Some(EhrId::new(EHR_ID)?),
        committed_at: None,
        versions: vec![ChangedVersion {
            vo_id: Some(CONTAINER.parse()?),
            kind: String::from("COMPOSITION"),
            sys_version: None,
            version_tree_id: None,
            change_type: change_type.to_owned(),
            template_id: None,
        }],
        envelope: serde_json::Map::new(),
    })
}

/// Returns the Bundles the endpoint received.
async fn bundles(endpoint: &MockServer) -> Vec<serde_json::Value> {
    endpoint
        .received_requests()
        .await
        .unwrap_or_default()
        .iter()
        .filter_map(|request| serde_json::from_slice(&request.body).ok())
        .collect()
}

/// Returns the `request.method` and `request.url` of every entry of
/// `bundle`.
fn requests(bundle: &serde_json::Value) -> Vec<(String, String)> {
    bundle["entry"]
        .as_array()
        .map(|entries| {
            entries
                .iter()
                .map(|entry| {
                    (
                        entry["request"]["method"]
                            .as_str()
                            .unwrap_or_default()
                            .to_owned(),
                        entry["request"]["url"]
                            .as_str()
                            .unwrap_or_default()
                            .to_owned(),
                    )
                })
                .collect()
        })
        .unwrap_or_default()
}

#[tokio::test]
async fn a_changed_composition_is_put_with_its_id_version_and_source() -> Result<(), Box<dyn Error>>
{
    let harness = harness().await;
    let id = create_one(&harness).await?;
    mount_read(&harness.cdr, VERSION_ONE, &harness.composition()).await;
    let lane = lane(harness, 200).await?;
    lane.outbound.process(&event(1, "249", false)?).await?;
    let received = bundles(&lane.endpoint).await;
    assert_eq!(1, received.len(), "one event, one Bundle");
    let bundle = &received[0];
    assert_eq!(Some("transaction"), bundle["type"].as_str(), "{bundle}");
    assert_eq!(
        vec![(String::from("PUT"), format!("Condition/{id}"))],
        requests(bundle),
        "{bundle}"
    );
    let resource = &bundle["entry"][0]["resource"];
    assert_eq!(Some(id.as_str()), resource["id"].as_str(), "{resource}");
    assert_eq!(
        Some("1"),
        resource["meta"]["versionId"].as_str(),
        "{resource}"
    );
    assert!(
        resource["meta"]["source"]
            .as_str()
            .is_some_and(|source| source.contains(EHR_ID)),
        "meta.source names the composition the resource came from: {resource}"
    );
    assert!(!resource["subject"].is_null(), "{resource}");
    Ok(())
}

#[tokio::test]
async fn an_event_delivered_again_puts_the_same_resource() -> Result<(), Box<dyn Error>> {
    let harness = harness().await;
    create_one(&harness).await?;
    mount_read(&harness.cdr, VERSION_ONE, &harness.composition()).await;
    let lane = lane(harness, 200).await?;
    lane.outbound.process(&event(1, "249", false)?).await?;
    lane.outbound.process(&event(1, "249", false)?).await?;
    let received = bundles(&lane.endpoint).await;
    assert_eq!(2, received.len());
    assert_eq!(
        requests(&received[0]),
        requests(&received[1]),
        "a delivery sent twice is the same upsert, at least once"
    );
    Ok(())
}

#[tokio::test]
async fn a_deleted_composition_is_retracted() -> Result<(), Box<dyn Error>> {
    let harness = harness().await;
    let id = create_one(&harness).await?;
    mount_read(&harness.cdr, VERSION_ONE, &harness.composition()).await;
    let lane = lane(harness, 200).await?;
    lane.outbound.process(&event(1, "249", false)?).await?;
    lane.harness.cdr.reset().await;
    Mock::given(matchers::method("GET"))
        .and(matchers::path(format!(
            "/ehr/{EHR_ID}/composition/{CONTAINER}"
        )))
        .respond_with(ResponseTemplate::new(204))
        .mount(&lane.harness.cdr)
        .await;
    lane.outbound.process(&event(2, "523", false)?).await?;
    let received = bundles(&lane.endpoint).await;
    assert_eq!(
        vec![(String::from("DELETE"), format!("Condition/{id}"))],
        requests(&received[1]),
        "the deletion retracts what the composition was delivered as"
    );
    Ok(())
}

#[tokio::test]
async fn an_erased_composition_is_retracted() -> Result<(), Box<dyn Error>> {
    let harness = harness().await;
    let id = create_one(&harness).await?;
    mount_read(&harness.cdr, VERSION_ONE, &harness.composition()).await;
    let lane = lane(harness, 200).await?;
    lane.outbound.process(&event(1, "249", false)?).await?;
    lane.outbound.process(&event(2, "erase", true)?).await?;
    let received = bundles(&lane.endpoint).await;
    assert_eq!(
        vec![(String::from("DELETE"), format!("Condition/{id}"))],
        requests(&received[1]),
        "the erasure retracts what the composition was delivered as"
    );
    Ok(())
}

#[tokio::test]
async fn a_resource_without_a_subject_is_refused_and_never_sent() -> Result<(), Box<dyn Error>> {
    let harness = harness().await;
    mount_read(&harness.cdr, VERSION_ONE, &harness.composition()).await;
    let lane = lane(harness, 200).await?;
    let error = lane
        .outbound
        .process(&event(1, "249", false)?)
        .await
        .expect_err("a resource no identity binds to a subject is refused");
    assert_eq!(Failure::Poisoned, error.failure(), "{error}");
    let cause = error.source().map(ToString::to_string);
    assert!(
        cause
            .as_deref()
            .is_some_and(|cause| cause.ends_with("names no subject, so it is not emitted")),
        "the refusal names the missing subject: {cause:?}"
    );
    assert!(
        bundles(&lane.endpoint).await.is_empty(),
        "nothing reaches the endpoint"
    );
    Ok(())
}

#[tokio::test]
async fn an_unavailable_endpoint_leaves_the_event_transient() -> Result<(), Box<dyn Error>> {
    let harness = harness().await;
    create_one(&harness).await?;
    mount_read(&harness.cdr, VERSION_ONE, &harness.composition()).await;
    let lane = lane(harness, 503).await?;
    let error = lane
        .outbound
        .process(&event(1, "249", false)?)
        .await
        .expect_err("a 503 does not finish the event");
    assert_eq!(Failure::Transient, error.failure(), "{error}");
    Ok(())
}

#[tokio::test]
async fn a_refused_delivery_is_poisoned() -> Result<(), Box<dyn Error>> {
    let harness = harness().await;
    create_one(&harness).await?;
    mount_read(&harness.cdr, VERSION_ONE, &harness.composition()).await;
    let lane = lane(harness, 422).await?;
    let error = lane
        .outbound
        .process(&event(1, "249", false)?)
        .await
        .expect_err("a 422 refuses the content");
    assert_eq!(Failure::Poisoned, error.failure(), "{error}");
    Ok(())
}
