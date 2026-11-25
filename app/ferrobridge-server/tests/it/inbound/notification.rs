// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! FHIR Subscription notifications through `POST /inbound/{source}`: the
//! Backport IG examples are verified and recorded, each event one work item
//! for its patient, a heartbeat that reveals a gap is recovered through
//! `$events`, and a notification without the declared client certificate
//! answers `401` unstored. The cases run only when `FERROBRIDGE_E2E=1`
//! admits the harness.

use std::error::Error;
use std::sync::Arc;
use std::time::Duration;

use ferrobridge_operator_api::sources::{Kind, Write};
use ferrobridge_server::inbound::intake::{Intake, routes};
use ferrobridge_server::inbound::keeper::Keeper;
use ferrobridge_server::inbound::recover::Recoverer;
use ferrobridge_server::inbound::store::InboxStore;
use ferrobridge_testkit::containers;
use http::{Request, StatusCode};
use serde_json::{Value, json};
use tower::ServiceExt as _;
use wiremock::matchers::{method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::intake::app_with;
use super::store::{create, replica, sources};

/// The vendored Backport IG example `Bundle-r4-notification-<name>.json`,
/// its subscription reference moved to `subscription`.
fn example(name: &str, subscription: &str) -> Result<Value, Box<dyn Error>> {
    let text = std::fs::read_to_string(format!(
        "{}/../../docs/specs/subscriptions-backport/Bundle-r4-notification-{name}.json",
        env!("CARGO_MANIFEST_DIR")
    ))?;
    let mut bundle: Value = serde_json::from_str(&text)?;
    bundle["entry"][0]["resource"]["parameter"][0]["valueReference"]["reference"] =
        json!(subscription);
    Ok(bundle)
}

/// A FHIR Subscription source authenticated by `Authorization`, and by the
/// client certificate name `mtls` when given, whose `subscription` serves
/// tenant `t1`.
fn subscribed(subscription: &str, mtls: Option<&str>) -> Write {
    let mut document = json!({
        "header": {"name": "Authorization", "keys": [{"secret": "webhook-key"}]},
        "subscriptions": {subscription: "t1"},
        "tenants": [{"tenant": "t1", "identifier_system": "urn:example:t1"}]
    });
    if let Some(name) = mtls {
        document["mtls"] = json!({"subjects": [name]});
    }
    Write {
        name: String::from("ehr"),
        kind: Kind::FhirSubscription,
        enabled: true,
        document,
    }
}

/// `bundle` posted to the source `ehr` with the shared header value.
fn post(bundle: &Value) -> Result<Request<axum::body::Body>, http::Error> {
    Request::post("/inbound/ehr")
        .header(http::header::CONTENT_TYPE, "application/fhir+json")
        .header(http::header::AUTHORIZATION, "synthetic-connector-secret")
        .body(axum::body::Body::from(bundle.to_string()))
}

const SUBSCRIPTION: &str = "https://ehr.example/fhir/Subscription/admission";

#[tokio::test]
async fn the_backport_examples_are_recorded_one_work_item_per_patient() -> Result<(), Box<dyn Error>>
{
    if !containers::e2e_enabled() {
        return Ok(());
    }
    let postgres = containers::postgres().await?;
    let store = replica(postgres.url()).await?;
    let secrets = tempfile::tempdir()?;
    let held = Arc::new(sources(&store, secrets.path())?);
    let source = create(&held, &subscribed(SUBSCRIPTION, None)).await?;
    let inbox = InboxStore::new(store.pool().clone());
    let app = app_with(routes(Arc::new(Intake::new(
        Arc::clone(&held),
        inbox.clone(),
        1024 * 1024,
    ))));

    for name in ["handshake", "id-only", "full-resource", "heartbeat"] {
        let answer = app
            .clone()
            .oneshot(post(&example(name, SUBSCRIPTION)?)?)
            .await?;
        assert_eq!(StatusCode::OK, answer.status(), "{name}");
    }
    let mut entities: Vec<String> = inbox
        .claim(uuid::Uuid::new_v4(), Duration::from_secs(30), 10)
        .await?
        .into_iter()
        .map(|item| item.key.entity)
        .collect();
    entities.sort();
    assert_eq!(
        vec![String::from(
            "https://example.org/fhir/Encounter/86009987-eabe-42bf-8c02-b112b18cb616"
        )],
        entities,
        "event 2 came once, id-only first; its repeat as full-resource stored nothing"
    );
    let gaps = Keeper::new(store.pool().clone()).gaps(source.id).await?;
    assert_eq!(1, gaps.len(), "event 1 was never delivered");
    assert_eq!((1, 1), (gaps[0].from, gaps[0].to));

    let unknown = app
        .clone()
        .oneshot(post(&example(
            "heartbeat",
            "https://ehr.example/fhir/Subscription/other",
        )?)?)
        .await?;
    assert_eq!(StatusCode::OK, unknown.status());
    let malformed = app
        .oneshot(post(&json!({"resourceType": "Bundle", "type": "batch"}))?)
        .await?;
    assert_eq!(StatusCode::BAD_REQUEST, malformed.status());
    Ok(())
}

#[tokio::test]
async fn a_gap_a_heartbeat_reveals_is_recovered_through_events() -> Result<(), Box<dyn Error>> {
    if !containers::e2e_enabled() {
        return Ok(());
    }
    let postgres = containers::postgres().await?;
    let store = replica(postgres.url()).await?;
    let secrets = tempfile::tempdir()?;
    let held = Arc::new(sources(&store, secrets.path())?);
    let stub = MockServer::start().await;
    let subscription = format!("{}/fhir/Subscription/admission", stub.uri());
    let source = create(&held, &subscribed(&subscription, None)).await?;
    let inbox = InboxStore::new(store.pool().clone());
    let app = app_with(routes(Arc::new(Intake::new(
        Arc::clone(&held),
        inbox.clone(),
        1024 * 1024,
    ))));
    for name in ["handshake", "heartbeat"] {
        let answer = app
            .clone()
            .oneshot(post(&example(name, &subscription)?)?)
            .await?;
        assert_eq!(StatusCode::OK, answer.status(), "{name}");
    }
    assert_eq!(0, inbox.pending(source.id).await?);

    let mut query_event = example("id-only", &subscription)?;
    query_event["entry"][0]["resource"]["parameter"][3]["valueCode"] = json!("query-event");
    let events = query_event["entry"][0]["resource"]["parameter"]
        .as_array_mut()
        .ok_or("parameters")?;
    events.push(json!({"name": "notification-event", "part": [
        {"name": "event-number", "valueString": "1"},
        {"name": "focus", "valueReference": {"reference": "https://ehr.example/fhir/Patient/p1"}}
    ]}));
    Mock::given(method("GET"))
        .and(path("/fhir/Subscription/admission/$events"))
        .and(query_param("eventsSinceNumber", "1"))
        .and(query_param("eventsUntilNumber", "2"))
        .and(query_param("content", "id-only"))
        .respond_with(ResponseTemplate::new(200).set_body_json(query_event))
        .expect(1)
        .mount(&stub)
        .await;

    let recoverer = Recoverer::new(store.clone(), Arc::clone(&held), Duration::from_secs(30));
    let lease = recoverer.lease_of(source.id)?;
    let generation = lease.claim().await?.ok_or("the lease is free")?;
    assert_eq!(2, recoverer.catch_up(&source, &lease, generation).await?);
    assert_eq!(
        2,
        inbox.pending(source.id).await?,
        "the patient and the encounter"
    );
    assert!(
        Keeper::new(store.pool().clone())
            .gaps(source.id)
            .await?
            .is_empty(),
        "the gap is closed"
    );
    stub.verify().await;
    Ok(())
}

#[tokio::test]
async fn a_notification_without_the_declared_certificate_answers_401() -> Result<(), Box<dyn Error>>
{
    if !containers::e2e_enabled() {
        return Ok(());
    }
    let postgres = containers::postgres().await?;
    let store = replica(postgres.url()).await?;
    let secrets = tempfile::tempdir()?;
    let held = Arc::new(sources(&store, secrets.path())?);
    let source = create(&held, &subscribed(SUBSCRIPTION, Some("ehr.example"))).await?;
    let inbox = InboxStore::new(store.pool().clone());
    let app = app_with(routes(Arc::new(Intake::new(
        Arc::clone(&held),
        inbox.clone(),
        1024 * 1024,
    ))));
    let answer = app
        .oneshot(post(&example("id-only", SUBSCRIPTION)?)?)
        .await?;
    assert_eq!(StatusCode::UNAUTHORIZED, answer.status());
    assert_eq!(0, inbox.pending(source.id).await?);
    assert!(
        Keeper::new(store.pool().clone())
            .gaps(source.id)
            .await?
            .is_empty(),
        "nothing of the notification is recorded"
    );
    Ok(())
}
