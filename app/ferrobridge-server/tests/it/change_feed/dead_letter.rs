// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The bounded retry and the dead-letter store: a poisoned event is charged
//! one pass at a time and parked on the bound with its cause, a transient
//! failure is never charged, a replay offers a parked event again, and two
//! consumers of one CDR each acknowledge their own finished prefix.
//!
//! Every event is synthetic: sequence numbers and identifiers invented for
//! the test.

use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::error::Error;
use std::num::NonZeroU32;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::PoisonError;

use ferrobridge_server::change_feed::ChangeFeed;
use ferrobridge_server::change_feed::Domain;
use ferrobridge_server::change_feed::Event;
use ferrobridge_server::change_feed::FeedError;
use ferrobridge_server::change_feed::Page;
use ferrobridge_server::change_feed::PageSize;
use ferrobridge_server::change_feed::Position;
use ferrobridge_server::change_feed::ReaderName;
use ferrobridge_server::change_feed::Seq;
use ferrobridge_server::change_feed::dead_letter::DeadLetters;
use ferrobridge_server::change_feed::ferroehr::FerroEhrFeed;
use ferrobridge_server::change_feed::follow::Consumer;
use ferrobridge_server::change_feed::follow::ConsumerError;
use ferrobridge_server::change_feed::follow::Cursor;
use ferrobridge_server::change_feed::follow::Parking;
use ferrobridge_server::change_feed::follow::PassError;
use ferrobridge_server::change_feed::follow::pass;
use serde_json::json;
use wiremock::matchers::{body_json, method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::Recording;
use super::client;
use super::reader;

/// A clinical outbox in memory, with one cursor per reader.
#[derive(Debug)]
struct Outbox {
    /// The sequence numbers the outbox holds.
    seqs: Vec<u64>,
    /// The last acknowledged sequence number per reader.
    cursors: Mutex<BTreeMap<String, u64>>,
}

impl Outbox {
    /// Returns the outbox holding `seqs`.
    fn new(seqs: &[u64]) -> Self {
        Self {
            seqs: seqs.to_vec(),
            cursors: Mutex::new(BTreeMap::new()),
        }
    }

    /// Returns the cursor of `reader`.
    fn cursor(&self, reader: &ReaderName) -> Option<u64> {
        self.cursors
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(reader.as_str())
            .copied()
    }
}

/// Returns the synthetic event at `seq`.
fn event(seq: u64) -> Event {
    Event {
        position: clinical(seq),
        erasure: false,
        contribution_id: None,
        ehr_id: None,
        committed_at: None,
        versions: Vec::new(),
        envelope: serde_json::Map::new(),
    }
}

/// Returns the position of `seq` in the clinical outbox.
fn clinical(seq: u64) -> Position {
    Position::Outbox {
        domain: Domain::Clinical,
        seq: Seq::new(seq),
    }
}

#[async_trait::async_trait]
impl ChangeFeed for Outbox {
    async fn page(&self, reader: &ReaderName, limit: PageSize) -> Result<Page, FeedError> {
        let after = self.cursor(reader).unwrap_or(0);
        Ok(Page {
            events: self
                .seqs
                .iter()
                .filter(|seq| **seq > after)
                .take(usize::from(limit.get()))
                .map(|seq| event(*seq))
                .collect(),
        })
    }

    async fn acknowledge(&self, reader: &ReaderName, last: &Position) -> Result<(), FeedError> {
        let Position::Outbox { seq, .. } = last else {
            return Err(FeedError::Foreign {
                position: last.clone(),
            });
        };
        self.cursors
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(reader.as_str().to_owned(), seq.get());
        Ok(())
    }
}

/// A consumer that fails chosen events, poisoned or transient, until it is
/// healed, and records every offer.
#[derive(Debug, Default)]
struct Failing {
    /// The events it fails with a poisoned failure.
    poisoned: BTreeSet<u64>,
    /// The events it fails with a transient failure.
    transient: BTreeSet<u64>,
    /// Whether it finishes every event from now on.
    healed: Mutex<bool>,
    /// The sequence numbers offered, in order.
    offered: Mutex<Vec<u64>>,
}

impl Failing {
    /// Returns the sequence numbers offered.
    fn offered(&self) -> Vec<u64> {
        self.offered
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    /// Finishes every event from now on.
    fn heal(&self) {
        *self.healed.lock().unwrap_or_else(PoisonError::into_inner) = true;
    }
}

#[async_trait::async_trait]
impl Consumer for Failing {
    fn name(&self) -> &'static str {
        "change-feed-failing"
    }

    async fn process(&self, event: &Event) -> Result<(), ConsumerError> {
        let seq = event.seq().map_or(0, Seq::get);
        self.offered
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(seq);
        if *self.healed.lock().unwrap_or_else(PoisonError::into_inner) {
            return Ok(());
        }
        if self.poisoned.contains(&seq) {
            return Err(ConsumerError::poisoned(
                "the synthetic event names no template",
            ));
        }
        if self.transient.contains(&seq) {
            return Err(ConsumerError::transient("the synthetic sink is down"));
        }
        Ok(())
    }
}

/// Returns a cursor parking on the third poisoned pass into a store in the
/// returned directory.
fn parking() -> Result<(tempfile::TempDir, Arc<DeadLetters>, Cursor), Box<dyn Error>> {
    let dir = tempfile::tempdir()?;
    let store = Arc::new(DeadLetters::open(&dir.path().join("dead-letters.redb"))?);
    let cursor = Cursor::parking(Parking {
        store: Arc::clone(&store),
        max_attempts: NonZeroU32::new(3).ok_or("a bound")?,
    });
    Ok((dir, store, cursor))
}

/// Returns the page size every case asks for.
fn limit() -> PageSize {
    PageSize::new(10).expect("a page size in range")
}

#[tokio::test]
async fn a_poisoned_event_is_parked_on_the_bound_and_the_cursor_moves_past_it()
-> Result<(), Box<dyn Error>> {
    let outbox = Outbox::new(&[1, 2, 3]);
    let consumer = Failing {
        poisoned: BTreeSet::from([2]),
        ..Failing::default()
    };
    let (_dir, store, mut cursor) = parking()?;
    for _below_the_bound in 0..2 {
        let failed = pass(&outbox, &reader(), &consumer, limit(), &mut cursor).await;
        assert!(
            matches!(failed, Err(PassError::Consumer { .. })),
            "{failed:?}"
        );
        assert_eq!(Some(1), outbox.cursor(&reader()));
        assert!(store.parked(&reader())?.is_empty());
    }
    let done = pass(&outbox, &reader(), &consumer, limit(), &mut cursor).await?;
    assert_eq!(vec![clinical(3)], done.acknowledged);
    assert_eq!(Some(3), outbox.cursor(&reader()));
    let parked = store.parked(&reader())?;
    let [entry] = parked.as_slice() else {
        return Err("one parked event".into());
    };
    assert_eq!(clinical(2), entry.event.position);
    assert_eq!(3, entry.attempts);
    assert!(
        entry
            .cause
            .contains("the synthetic event names no template"),
        "{}",
        entry.cause
    );
    Ok(())
}

#[tokio::test]
async fn a_transient_failure_never_charges_the_bound() -> Result<(), Box<dyn Error>> {
    let outbox = Outbox::new(&[1, 2]);
    let consumer = Failing {
        transient: BTreeSet::from([2]),
        ..Failing::default()
    };
    let (_dir, store, mut cursor) = parking()?;
    for _pass in 0..6 {
        let failed = pass(&outbox, &reader(), &consumer, limit(), &mut cursor).await;
        assert!(
            matches!(failed, Err(PassError::Consumer { .. })),
            "{failed:?}"
        );
    }
    assert_eq!(Some(1), outbox.cursor(&reader()));
    assert!(store.parked(&reader())?.is_empty());
    assert_eq!(
        1,
        store.charge(&reader(), &clinical(2))?,
        "no pass was charged"
    );
    Ok(())
}

#[tokio::test]
async fn an_event_that_recovers_below_the_bound_starts_its_count_again()
-> Result<(), Box<dyn Error>> {
    let outbox = Outbox::new(&[1]);
    let consumer = Failing {
        poisoned: BTreeSet::from([1]),
        ..Failing::default()
    };
    let (_dir, store, mut cursor) = parking()?;
    let failed = pass(&outbox, &reader(), &consumer, limit(), &mut cursor).await;
    assert!(failed.is_err());
    consumer.heal();
    pass(&outbox, &reader(), &consumer, limit(), &mut cursor).await?;
    assert_eq!(
        1,
        store.charge(&reader(), &clinical(1))?,
        "the count was forgotten"
    );
    Ok(())
}

#[tokio::test]
async fn a_replay_offers_the_parked_event_again_before_the_next_page() -> Result<(), Box<dyn Error>>
{
    let outbox = Outbox::new(&[1]);
    let consumer = Failing {
        poisoned: BTreeSet::from([1]),
        ..Failing::default()
    };
    let (_dir, store, mut cursor) = parking()?;
    for _bound in 0..3 {
        let _outcome = pass(&outbox, &reader(), &consumer, limit(), &mut cursor).await;
    }
    let id = store
        .parked(&reader())?
        .first()
        .map(|parked| parked.id)
        .ok_or("one parked event")?;
    pass(&outbox, &reader(), &consumer, limit(), &mut cursor).await?;
    assert_eq!(
        vec![1, 1, 1],
        consumer.offered(),
        "no replay without a request"
    );
    assert!(store.request_replay(&reader(), id)?);
    pass(&outbox, &reader(), &consumer, limit(), &mut cursor).await?;
    let parked = store.parked(&reader())?;
    let entry = parked.first().ok_or("still parked")?;
    assert_eq!((4, false), (entry.attempts, entry.replay), "parked again");
    consumer.heal();
    assert!(store.request_replay(&reader(), id)?);
    pass(&outbox, &reader(), &consumer, limit(), &mut cursor).await?;
    assert_eq!(vec![1, 1, 1, 1, 1], consumer.offered());
    assert!(store.parked(&reader())?.is_empty());
    Ok(())
}

#[tokio::test]
async fn a_replay_that_meets_a_transient_failure_stays_requested() -> Result<(), Box<dyn Error>> {
    let outbox = Outbox::new(&[]);
    let (_dir, store, mut cursor) = parking()?;
    let id = store.park(
        &reader(),
        &event(9),
        "the synthetic event names no template",
        3,
    )?;
    assert!(store.request_replay(&reader(), id)?);
    let consumer = Failing {
        transient: BTreeSet::from([9]),
        ..Failing::default()
    };
    let failed = pass(&outbox, &reader(), &consumer, limit(), &mut cursor).await;
    assert!(
        matches!(failed, Err(PassError::Replay { .. })),
        "{failed:?}"
    );
    let parked = store.parked(&reader())?;
    let entry = parked.first().ok_or("still parked")?;
    assert_eq!((3, true), (entry.attempts, entry.replay));
    Ok(())
}

/// The feed root the stub serves.
const ROOT: &str = "/ext";

/// Mounts the clinical outbox of `reader` on `server`, serving `seqs`, an
/// empty party outbox, and an acknowledgement of `last` expected once.
async fn reader_feed(server: &MockServer, reader: &str, seqs: &[u64], last: u64) {
    let feed = format!("{ROOT}/admin/outbox/{reader}");
    let events: Vec<serde_json::Value> = seqs
        .iter()
        .map(|seq| {
            json!({
                "seq": seq,
                "contribution_id": format!("0d4a3c52-5b51-4b4b-9e0e-{seq:012}"),
                "ehr_id": "7d44b88c-4199-4bad-97dc-d78268e01398",
                "committed_at": "2026-11-02T10:00:00Z",
                "versions": [{
                    "vo_id": "8849182c-82ad-4088-a07f-48ead4180515",
                    "kind": "COMPOSITION",
                    "sys_version": 3,
                    "change_type": "251",
                    "template_id": "Synthetic vital signs"
                }]
            })
        })
        .collect();
    for (domain, served) in [("clinical", events), ("party", Vec::new())] {
        Mock::given(method("GET"))
            .and(path(feed.as_str()))
            .and(query_param("domain", domain))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(json!({ "reader": reader, "events": served })),
            )
            .mount(server)
            .await;
    }
    Mock::given(method("POST"))
        .and(path(format!("{feed}/ack")))
        .and(query_param("domain", "clinical"))
        .and(body_json(json!({ "last_seq": last })))
        .respond_with(ResponseTemplate::new(204))
        .expect(1)
        .mount(server)
        .await;
}

#[tokio::test]
async fn two_consumers_of_one_cdr_each_acknowledge_their_own_finished_prefix()
-> Result<(), Box<dyn Error>> {
    let server = MockServer::start().await;
    reader_feed(&server, "ferrobridge-events", &[5, 6, 7], 7).await;
    reader_feed(&server, "ferrobridge-omop", &[5, 6, 7], 5).await;
    let feed = FerroEhrFeed::new(client(&server)?, format!("{}{ROOT}", server.uri()).parse()?);
    let events = ReaderName::new("ferrobridge-events")?;
    let omop = ReaderName::new("ferrobridge-omop")?;
    let finishing = Recording::default();
    let stopping = Recording::failing_at(1);
    let mut first = Cursor::default();
    let mut second = Cursor::default();
    let done = pass(&feed, &events, &finishing, limit(), &mut first).await?;
    assert_eq!(vec![clinical(7)], done.acknowledged);
    let failed = pass(&feed, &omop, &stopping, limit(), &mut second).await;
    assert!(
        matches!(failed, Err(PassError::Consumer { .. })),
        "{failed:?}"
    );
    assert_eq!(
        Some(&clinical(5)),
        second.acknowledged(clinical(5).stream())
    );
    assert_eq!(Some(&clinical(7)), first.acknowledged(clinical(7).stream()));
    Ok(())
}

/// A consumer that finishes every event and records each settle, failing
/// it while `refuse` holds.
#[derive(Debug, Default)]
struct Settling {
    /// Whether the settle fails.
    refuse: Mutex<bool>,
    /// How many settles ran.
    settled: Mutex<usize>,
}

#[async_trait::async_trait]
impl Consumer for Settling {
    fn name(&self) -> &'static str {
        "change-feed-settling"
    }

    async fn process(&self, _event: &Event) -> Result<(), ConsumerError> {
        Ok(())
    }

    async fn settle(&self) -> Result<(), ConsumerError> {
        *self.settled.lock().unwrap_or_else(PoisonError::into_inner) += 1;
        if *self.refuse.lock().unwrap_or_else(PoisonError::into_inner) {
            return Err(ConsumerError::transient("the synthetic derivation failed"));
        }
        Ok(())
    }
}

#[tokio::test]
async fn a_pass_settles_before_it_acknowledges_and_a_failed_settle_acknowledges_nothing()
-> Result<(), Box<dyn Error>> {
    let outbox = Outbox::new(&[1, 2]);
    let consumer = Settling {
        refuse: Mutex::new(true),
        ..Settling::default()
    };
    let mut cursor = Cursor::default();
    let failed = pass(&outbox, &reader(), &consumer, limit(), &mut cursor).await;
    assert!(
        matches!(failed, Err(PassError::Settle { .. })),
        "{failed:?}"
    );
    assert_eq!(None, outbox.cursor(&reader()), "nothing was acknowledged");
    *consumer
        .refuse
        .lock()
        .unwrap_or_else(PoisonError::into_inner) = false;
    pass(&outbox, &reader(), &consumer, limit(), &mut cursor).await?;
    assert_eq!(Some(2), outbox.cursor(&reader()));
    pass(&outbox, &reader(), &consumer, limit(), &mut cursor).await?;
    assert_eq!(
        2,
        *consumer
            .settled
            .lock()
            .unwrap_or_else(PoisonError::into_inner),
        "an empty page settles nothing"
    );
    Ok(())
}
