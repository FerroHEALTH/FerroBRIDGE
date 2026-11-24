// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! One leader per reader across replicas: a leader killed mid-page leaves
//! the watermark where it was, the replica that takes the lane over offers
//! every event, and the former leader's cursor write is refused. The cases
//! run only when `FERROBRIDGE_E2E=1` admits the harness.
//!
//! Every row is synthetic: identifiers and instants invented for the test.

use std::error::Error;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::PoisonError;
use std::time::Duration;

use ferrobridge_server::change_feed::ChangeFeed;
use ferrobridge_server::change_feed::Committed;
use ferrobridge_server::change_feed::Event;
use ferrobridge_server::change_feed::FeedError;
use ferrobridge_server::change_feed::Ledger;
use ferrobridge_server::change_feed::PageSize;
use ferrobridge_server::change_feed::Position;
use ferrobridge_server::change_feed::aql::AqlFeed;
use ferrobridge_server::change_feed::aql::Start;
use ferrobridge_server::change_feed::follow::Consumer;
use ferrobridge_server::change_feed::follow::ConsumerError;
use ferrobridge_server::change_feed::follow::Cursor;
use ferrobridge_server::change_feed::follow::pass;
use ferrobridge_server::change_feed::watermark::Watermarks;
use ferrobridge_server::store::Settings;
use ferrobridge_server::store::Store;
use ferrobridge_server::store::lease::Lane;
use ferrobridge_server::store::lease::LeaseError;
use ferrobridge_server::store::lease::Term;
use ferrobridge_testkit::containers;
use ferrobridge_testkit::stubs::its_rest;
use serde_json::json;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, Request, ResponseTemplate};

use super::client;
use super::reader;

/// The period the leases of these cases hold.
const TTL: Duration = Duration::from_millis(400);

/// The instant the reader starts at.
const START: &str = "2026-11-02T09:00:00Z";

/// The columns of the source's projection.
const COLUMNS: [(&str, &str); 6] = [
    ("#0", "/ehr_id/value"),
    ("#1", "/uid/value"),
    ("#2", "/contribution/id/value"),
    ("#3", "/commit_audit/time_committed/value"),
    ("#4", "/commit_audit/change_type/defining_code/code_string"),
    ("#5", "/archetype_details/template_id/value"),
];

/// Returns the synthetic row of the version `object`, committed at its own
/// second.
fn row(object: u32) -> Vec<serde_json::Value> {
    vec![
        json!("7d44b88c-4199-4bad-97dc-d78268e01398"),
        json!(format!(
            "8849182c-82ad-4088-a07f-{object:012}::synthetic.cdr::1"
        )),
        json!(format!("0d4a3c52-5b51-4b4b-9e0e-{object:012}")),
        json!(format!("2026-11-02T10:00:{object:02}Z")),
        json!("249"),
        json!("Synthetic vital signs"),
    ]
}

/// Answers a query with the rows committed at or after its `since`, as a CDR
/// does.
fn answer(request: &Request) -> ResponseTemplate {
    let body: serde_json::Value = serde_json::from_slice(&request.body).unwrap_or_default();
    let since = body["query_parameters"]["since"]
        .as_str()
        .unwrap_or_default()
        .to_owned();
    let rows: Vec<Vec<serde_json::Value>> = (1..=6)
        .map(row)
        .filter(|row| row[3].as_str().unwrap_or_default() >= since.as_str())
        .collect();
    its_rest::result_set("SELECT synthetic", &COLUMNS, &rows)
}

/// A consumer that records each event it finishes and, when `stall` holds,
/// never returns from the second event it is offered.
#[derive(Debug, Default)]
struct Stalling {
    /// Whether the second offer stalls.
    stall: bool,
    /// The contributions finished, in order.
    finished: Mutex<Vec<String>>,
    /// How many events were offered.
    offered: Mutex<usize>,
}

impl Stalling {
    /// Returns how many events were offered.
    fn offered(&self) -> usize {
        *self.offered.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Returns the contributions finished.
    fn finished(&self) -> Vec<String> {
        self.finished
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }
}

#[async_trait::async_trait]
impl Consumer for Stalling {
    fn name(&self) -> &'static str {
        "change-feed-takeover"
    }

    async fn process(&self, event: &Event) -> Result<(), ConsumerError> {
        let index = {
            let mut offered = self.offered.lock().unwrap_or_else(PoisonError::into_inner);
            *offered += 1;
            *offered
        };
        if self.stall && index == 2 {
            std::future::pending::<()>().await;
        }
        self.finished
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(
                event
                    .contribution_id
                    .as_ref()
                    .map(|id| id.as_str().to_owned())
                    .unwrap_or_default(),
            );
        Ok(())
    }
}

/// A replica: its own store handle, so its own holder id.
async fn replica(url: &str) -> Result<Store, Box<dyn Error>> {
    Ok(Store::open(&Settings {
        connection: omop_cdm::connection::CdmConnection::new(url, None)?,
        max_connections: 4,
        acquire_timeout: Duration::from_secs(5),
        seal: None,
    })
    .await?)
}

/// Takes the test reader's lane on `store`.
async fn claim(store: &Store) -> Result<Option<Term>, Box<dyn Error>> {
    let lease = store.lease(Lane::new(format!("change-feed:{}", reader()))?, TTL);
    Ok(lease
        .claim()
        .await?
        .map(|generation| Term::new(lease, generation)))
}

#[tokio::test]
async fn a_takeover_after_a_leader_dies_mid_page_skips_no_event_and_fences_the_former_leader()
-> Result<(), Box<dyn Error>> {
    if !containers::e2e_enabled() {
        return Ok(());
    }
    let postgres = containers::postgres().await?;
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/query/aql"))
        .respond_with(answer)
        .mount(&server)
        .await;
    let first = replica(postgres.url()).await?;
    let second = replica(postgres.url()).await?;
    let limit = PageSize::new(3).ok_or("a page size")?;
    let source = |store: &Store| -> Result<AqlFeed, Box<dyn Error>> {
        Ok(AqlFeed::new(
            client(&server)?,
            Watermarks::new(store.pool().clone()),
            Start::At(String::from(START)),
        ))
    };

    let old = claim(&first).await?.ok_or("the lane is free")?;
    assert!(claim(&second).await?.is_none(), "one leader at a time");
    let leading = source(&first)?;
    leading.begin(&reader(), &old).await?;
    let dying = Arc::new(Stalling {
        stall: true,
        ..Stalling::default()
    });
    let task = {
        let (feed, consumer, term) = (leading.clone(), Arc::clone(&dying), old.clone());
        tokio::spawn(async move {
            let mut cursor = Cursor::default();
            pass(
                &feed,
                &reader(),
                consumer.as_ref(),
                limit,
                &mut cursor,
                &term,
            )
            .await
        })
    };
    for _ in 0..500 {
        if dying.offered() >= 2 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    task.abort();
    assert_eq!(1, dying.finished().len(), "the leader died mid-page");

    tokio::time::sleep(TTL * 2).await;
    let new = claim(&second).await?.ok_or("the lease ran out")?;
    assert!(new.generation() > old.generation());
    let taking = source(&second)?;
    taking.begin(&reader(), &new).await?;
    let survivor = Stalling::default();
    let mut cursor = Cursor::default();
    for _ in 0..5 {
        let done = pass(&taking, &reader(), &survivor, limit, &mut cursor, &new).await?;
        if done.read == 0 {
            break;
        }
    }
    let expected: Vec<String> = (1..=6_u32)
        .map(|object| format!("0d4a3c52-5b51-4b4b-9e0e-{object:012}"))
        .collect();
    assert_eq!(expected, survivor.finished(), "every event, none skipped");

    let stale = Committed::new(
        "2026-11-02T10:00:01Z".parse()?,
        String::from("2026-11-02T10:00:01Z"),
        std::collections::BTreeSet::from([String::from(
            "8849182c-82ad-4088-a07f-000000000001::synthetic.cdr::1",
        )]),
    );
    let mut ledger = Ledger::new(&old);
    let refused = leading
        .record(&mut ledger, &reader(), &Position::Committed(stale))
        .await;
    assert!(
        matches!(refused, Err(FeedError::Lease(LeaseError::Stale { .. }))),
        "{refused:?}"
    );
    let kept = taking
        .positions(&reader())
        .await?
        .pop()
        .ok_or("the new leader's watermark")?;
    let Position::Committed(kept) = kept else {
        return Err("an AQL watermark".into());
    };
    assert_eq!("2026-11-02T10:00:06Z", kept.text());
    Ok(())
}
