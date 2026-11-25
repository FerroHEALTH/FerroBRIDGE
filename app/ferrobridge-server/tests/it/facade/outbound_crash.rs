// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The outbound lane stopped at each point of a delivery: before the
//! endpoint answers, after it answers and before the commit, and after the
//! commit. In every case the endpoint ends holding each resource once, the
//! delivery record holds what was delivered, and the watermark covers the
//! event: no effect is lost and none is applied twice, since a redelivery is
//! the same idempotent `PUT` (<https://hl7.org/fhir/R4/http.html#transaction>).
//! The cases run only when `FERROBRIDGE_E2E=1` admits the harness.

use std::collections::BTreeMap;
use std::error::Error;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::PoisonError;
use std::time::Duration;

use ferrobridge_server::change_feed::ChangeFeed;
use ferrobridge_server::change_feed::PageSize;
use ferrobridge_server::change_feed::Position;
use ferrobridge_server::change_feed::aql::AqlFeed;
use ferrobridge_server::change_feed::aql::Start;
use ferrobridge_server::change_feed::follow::Cursor;
use ferrobridge_server::change_feed::follow::pass;
use ferrobridge_server::change_feed::watermark::Watermarks;
use ferrobridge_server::outbound::Outbound;
use ferrobridge_server::outbound::Settings;
use ferrobridge_server::outbound::sink::Sink;
use ferrobridge_server::store::lease::Term;
use ferrobridge_testkit::containers;
use ferrobridge_testkit::stubs::its_rest;
use ferrohealth_secret::secret_url::SecretUrl;
use ferrohealth_smart::client::outbound::OutboundClient;
use ferrohealth_smart::config::ServerTls;
use ferrohealth_smart::config::endpoint::EndpointSection;
use serde_json::json;
use wiremock::Mock;
use wiremock::MockServer;
use wiremock::Request;
use wiremock::ResponseTemplate;
use wiremock::matchers;

use super::CONTAINER;
use super::EHR_ID;
use super::VERSION_ONE;
use super::create_one;
use super::harness;
use super::stub::mount_read;
use crate::change_feed::led;
use crate::change_feed::reader;

/// The instant the lane's reader starts at.
const START: &str = "2026-11-02T09:00:00Z";

/// The instant the composition was committed at.
const COMMITTED: &str = "2026-11-02T10:00:00Z";

/// An endpoint that records every Bundle it is sent and answers `200` after
/// the delay it holds at the time.
#[derive(Debug, Clone, Default)]
struct Endpoint {
    /// The Bundles received, in order.
    received: Arc<Mutex<Vec<serde_json::Value>>>,
    /// How long the next answer waits.
    delay: Arc<Mutex<Duration>>,
}

impl Endpoint {
    /// Returns the Bundles received.
    fn received(&self) -> Vec<serde_json::Value> {
        self.received
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    /// Makes every later answer wait `delay`.
    fn delay(&self, delay: Duration) {
        *self.delay.lock().unwrap_or_else(PoisonError::into_inner) = delay;
    }

    /// What the endpoint holds once every entry received is applied in
    /// order: each `PUT` writes its resource at its URL and each `DELETE`
    /// removes it.
    fn holds(&self) -> BTreeMap<String, serde_json::Value> {
        let mut held = BTreeMap::new();
        for bundle in self.received() {
            for entry in bundle["entry"].as_array().cloned().unwrap_or_default() {
                let url = entry["request"]["url"]
                    .as_str()
                    .unwrap_or_default()
                    .to_owned();
                match entry["request"]["method"].as_str() {
                    Some("PUT") => {
                        held.insert(url, entry["resource"].clone());
                    }
                    Some("DELETE") => {
                        held.remove(&url);
                    }
                    _ => {}
                }
            }
        }
        held
    }
}

impl wiremock::Respond for Endpoint {
    fn respond(&self, request: &Request) -> ResponseTemplate {
        if let Ok(bundle) = serde_json::from_slice(&request.body) {
            self.received
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .push(bundle);
        }
        let delay = *self.delay.lock().unwrap_or_else(PoisonError::into_inner);
        ResponseTemplate::new(200).set_delay(delay)
    }
}

/// One case: the lane, its AQL source over the stub CDR, the endpoint and
/// the lease the follower holds.
struct Crash {
    /// The lane.
    outbound: Arc<Outbound>,
    /// The source.
    feed: AqlFeed,
    /// The endpoint's recorder.
    endpoint: Endpoint,
    /// The lease.
    term: Term,
    /// The created resource, as `Condition/[id]`.
    reference: String,
    /// The stub CDR and the endpoint, kept for the case.
    _servers: (MockServer, MockServer),
    /// The state store's container, kept for the case.
    _postgres: containers::Postgres,
}

impl Crash {
    /// Runs one pass to its end.
    async fn pass(&self) -> Result<(), Box<dyn Error>> {
        let mut cursor = Cursor::default();
        pass(
            &self.feed,
            &reader(),
            self.outbound.as_ref(),
            PageSize::new(10).ok_or("a page size")?,
            &mut cursor,
            &self.term,
        )
        .await?;
        Ok(())
    }

    /// Starts a pass on its own task, which the case kills.
    fn spawn(&self) -> tokio::task::JoinHandle<()> {
        let (feed, outbound, term) = (
            self.feed.clone(),
            Arc::clone(&self.outbound),
            self.term.clone(),
        );
        tokio::spawn(async move {
            let mut cursor = Cursor::default();
            if let Some(size) = PageSize::new(10) {
                let _outcome = pass(
                    &feed,
                    &reader(),
                    outbound.as_ref(),
                    size,
                    &mut cursor,
                    &term,
                )
                .await;
            }
        })
    }

    /// Waits until the endpoint received `count` Bundles.
    async fn until_received(&self, count: usize) -> Result<(), Box<dyn Error>> {
        for _ in 0..1000 {
            if self.endpoint.received().len() >= count {
                return Ok(());
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        Err(format!("the endpoint never received {count} Bundles").into())
    }

    /// Asserts the endpoint holds the resource once, the record holds its
    /// reference and the watermark covers the commit.
    async fn assert_delivered_once(&self) -> Result<(), Box<dyn Error>> {
        let held = self.endpoint.holds();
        assert_eq!(
            vec![self.reference.clone()],
            held.keys().cloned().collect::<Vec<_>>(),
            "the endpoint holds the resource once"
        );
        let refs: Vec<String> = sqlx::query_scalar(
            "SELECT refs FROM outbound_delivered WHERE versioned_object_id = $1",
        )
        .bind(CONTAINER)
        .fetch_one(self.term.pool())
        .await?;
        assert_eq!(vec![self.reference.clone()], refs, "the record holds it");
        let watermark = self
            .feed
            .positions(&reader())
            .await?
            .pop()
            .ok_or("the watermark")?;
        let Position::Committed(watermark) = watermark else {
            return Err("an AQL watermark".into());
        };
        assert_eq!(
            COMMITTED,
            watermark.text(),
            "the watermark covers the event"
        );
        let before = self.endpoint.received().len();
        self.pass().await?;
        assert_eq!(
            before,
            self.endpoint.received().len(),
            "the event is not offered again"
        );
        Ok(())
    }
}

/// Returns a case, `None` unless `FERROBRIDGE_E2E=1`.
async fn crash() -> Result<Option<Crash>, Box<dyn Error>> {
    if !containers::e2e_enabled() {
        return Ok(None);
    }
    let postgres = containers::postgres().await?;
    let (store, term) = led(postgres.url()).await?;
    let harness = harness().await;
    let id = create_one(&harness).await?;
    mount_read(&harness.cdr, VERSION_ONE, &harness.composition()).await;
    Mock::given(matchers::method("POST"))
        .and(matchers::path("/query/aql"))
        .and(matchers::body_string_contains("VERSION v[ALL_VERSIONS]"))
        .respond_with(its_rest::result_set(
            "SELECT synthetic",
            &[
                ("#0", "/ehr_id/value"),
                ("#1", "/uid/value"),
                ("#2", "/contribution/id/value"),
                ("#3", "/commit_audit/time_committed/value"),
                ("#4", "/commit_audit/change_type/defining_code/code_string"),
                ("#5", "/archetype_details/template_id/value"),
            ],
            &[vec![
                json!(EHR_ID),
                json!(VERSION_ONE),
                json!("0d4a3c52-5b51-4b4b-9e0e-000000000001"),
                json!(COMMITTED),
                json!("249"),
                json!(null),
            ]],
        ))
        .mount(&harness.cdr)
        .await;
    let server = MockServer::start().await;
    let endpoint = Endpoint::default();
    Mock::given(matchers::method("POST"))
        .and(matchers::path("/fhir"))
        .respond_with(endpoint.clone())
        .mount(&server)
        .await;
    let settings = Settings {
        reader: reader(),
        sink: Sink::new(OutboundClient::build(
            &EndpointSection {
                url: SecretUrl::new(format!("{}/fhir", server.uri())),
                timeout: ferrohealth_config::units::Duration::from_secs(30),
                ..EndpointSection::default()
            },
            "fhir_outbound.endpoint",
            &ServerTls,
        )?),
    };
    let client = harness.facade.client().clone();
    let outbound = Arc::new(Outbound::start(
        &settings,
        Arc::clone(&harness.facade),
        client.clone(),
        &store,
    ));
    let feed = AqlFeed::new(
        client,
        Watermarks::new(store.pool().clone()),
        Start::At(String::from(START)),
    );
    feed.begin(&reader(), &term).await?;
    Ok(Some(Crash {
        outbound,
        feed,
        endpoint,
        term,
        reference: format!("Condition/{id}"),
        _servers: (harness.cdr, server),
        _postgres: postgres,
    }))
}

#[tokio::test]
async fn a_lane_killed_before_the_endpoint_answers_delivers_once() -> Result<(), Box<dyn Error>> {
    let Some(case) = crash().await? else {
        return Ok(());
    };
    case.endpoint.delay(Duration::from_secs(10));
    let task = case.spawn();
    case.until_received(1).await?;
    task.abort();
    let _killed = task.await;
    case.endpoint.delay(Duration::ZERO);
    case.pass().await?;
    assert_eq!(
        2,
        case.endpoint.received().len(),
        "the unanswered delivery is sent again"
    );
    case.assert_delivered_once().await
}

#[tokio::test]
async fn a_lane_killed_after_the_endpoint_answers_and_before_the_commit_delivers_once()
-> Result<(), Box<dyn Error>> {
    let Some(case) = crash().await? else {
        return Ok(());
    };
    let mut held = case.term.pool().begin().await?;
    sqlx::query("SELECT generation FROM lane_lease WHERE lane = $1 FOR UPDATE")
        .bind(case.term.lease().lane().as_str())
        .execute(&mut *held)
        .await?;
    let task = case.spawn();
    case.until_received(1).await?;
    tokio::time::sleep(Duration::from_millis(500)).await;
    task.abort();
    let _killed = task.await;
    held.rollback().await?;
    let recorded: i64 = sqlx::query_scalar("SELECT count(*) FROM outbound_delivered")
        .fetch_one(case.term.pool())
        .await?;
    assert_eq!(
        0, recorded,
        "the kill lost the commit, record and watermark together"
    );
    case.pass().await?;
    assert_eq!(
        2,
        case.endpoint.received().len(),
        "the uncommitted delivery is sent again"
    );
    case.assert_delivered_once().await
}

#[tokio::test]
async fn a_lane_killed_after_the_commit_never_delivers_again() -> Result<(), Box<dyn Error>> {
    let Some(case) = crash().await? else {
        return Ok(());
    };
    case.pass().await?;
    assert_eq!(1, case.endpoint.received().len());
    case.assert_delivered_once().await
}
