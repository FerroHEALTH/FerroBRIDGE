// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! `GET /operator/events`: every change-feed consumer with its parked events
//! and the AMQP topology, empty when no consumer runs, and never the broker
//! URL or an event body; `POST /operator/events/{consumer}/parked/{id}/replay`
//! marks one parked event for a replay.

use std::error::Error;
use std::sync::Arc;
use std::time::Duration;

use ferrobridge_operator_api::events::{Events, Source, State};
use ferrobridge_server::change_feed::dead_letter::DeadLetters;
use ferrobridge_server::change_feed::health::Following;
use ferrobridge_server::change_feed::report::{Follower, Report};
use ferrobridge_server::change_feed::{Domain, Event, Position, ReaderName, Seq};
use ferrobridge_server::events::{Queue, Settings};
use http::StatusCode;
use secrecy::SecretString;

use super::{Case, OPERATOR, bytes};

/// A marker the parked event's envelope carries, which no answer may hold.
const ENVELOPE_MARKER: &str = "ferrobridge-envelope-marker";

/// The parked event, synthetic: clinical seq 7 of one contribution.
fn poisoned() -> Result<Event, Box<dyn Error>> {
    let mut envelope = serde_json::Map::new();
    envelope.insert(
        String::from("marker"),
        serde_json::Value::from(ENVELOPE_MARKER),
    );
    Ok(Event {
        position: Position::Outbox {
            domain: Domain::Clinical,
            seq: Seq::new(7),
        },
        erasure: false,
        contribution_id: Some(ferrobridge_server::cdr::ids::ContributionUid::new(
            "0d4a3c52-5b51-4b4b-9e0e-000000000007",
        )?),
        ehr_id: None,
        committed_at: None,
        versions: Vec::new(),
        envelope,
    })
}

/// Returns a report over the reference CDR's feed: the events lane, stalled
/// after one acknowledged event and one published message with one parked
/// event, whose broker URL carries `password`, and a second consumer under
/// its own reader; the store lives in the returned directory.
fn report(password: &str) -> Result<(tempfile::TempDir, Report, u64), Box<dyn Error>> {
    let dir = tempfile::tempdir()?;
    let store = Arc::new(DeadLetters::open(&dir.path().join("dead-letters.redb"))?);
    let reader = ReaderName::new("ferrobridge")?;
    let id = store.park(&reader, &poisoned()?, "the event names no template", 5)?;
    let following = Following::new(ferrobridge_server::events::NAME);
    following.acknowledged(&[Position::Outbox {
        domain: Domain::Clinical,
        seq: Seq::new(42),
    }]);
    following.delivered();
    following.stalled("the AMQP broker could not be reached");
    let report = Report::new(
        ferrobridge_server::change_feed::Source::FerroEhr {
            root: "https://ehr.invalid/ferroehr/rest/extensions/v1".parse()?,
        },
        store,
    )
    .with_follower(Follower {
        name: ferrobridge_server::events::NAME,
        reader: reader.clone(),
        following,
    })
    .with_follower(Follower {
        name: "change-feed-second",
        reader: ReaderName::new("ferrobridge-second")?,
        following: Following::new("change-feed-second"),
    })
    .with_events(Settings {
        reader,
        url: SecretString::from(format!("amqp://bridge:{password}@broker.invalid/%2f")),
        exchange: String::from("ferroehr.events"),
        queues: vec![Queue {
            name: String::from("ferroehr.events.analytics"),
            binding: String::from("COMPOSITION.*.*"),
        }],
        confirm_timeout: Duration::from_secs(30),
    });
    Ok((dir, report, id))
}

/// Reads the events document `case` answers.
async fn document(case: &Case) -> Result<(String, Events), Box<dyn Error>> {
    let response = case.get("/operator/events", Some(OPERATOR)).await?;
    if response.status() != StatusCode::OK {
        return Err(format!("the document answered {}", response.status()).into());
    }
    let bytes = bytes(response).await?;
    Ok((
        String::from_utf8(bytes.clone())?,
        serde_json::from_slice(&bytes)?,
    ))
}

#[tokio::test]
async fn a_bridge_with_no_consumer_answers_an_empty_document() -> Result<(), Box<dyn Error>> {
    let case = Case::new().await?;
    let (_text, document) = document(&case).await?;
    assert_eq!(Events::default(), document);
    Ok(())
}

#[tokio::test]
async fn a_running_lane_reports_its_consumer_and_topology() -> Result<(), Box<dyn Error>> {
    let password = uuid::Uuid::new_v4().to_string();
    let (_dir, report, _id) = report(&password)?;
    let case = Case::reporting(report).await?;
    let (text, document) = document(&case).await?;
    assert!(!text.contains(&password), "{text}");
    let consumer = document.consumers.first().ok_or("one consumer")?;
    assert_eq!(Source::Ferroehr, consumer.source);
    assert_eq!("ferrobridge", consumer.reader);
    assert_eq!(State::Stalled, consumer.state);
    assert_eq!(1, consumer.published);
    let position = consumer.positions.first().ok_or("a position")?;
    assert_eq!(
        ("clinical", "42"),
        (position.stream.as_str(), position.at.as_str())
    );
    assert_eq!(
        Some("the AMQP broker could not be reached"),
        consumer
            .last_error
            .as_ref()
            .map(|failure| failure.cause.as_str())
    );
    let topology = document.topology.ok_or("the topology")?;
    assert_eq!("ferroehr.events", topology.exchange);
    assert_eq!("<kind>.<change_type>.<template_id>", topology.routing_key);
    assert_eq!(1, topology.queues.len());
    Ok(())
}

#[tokio::test]
async fn every_consumer_is_listed_with_its_own_reader_and_parked_events()
-> Result<(), Box<dyn Error>> {
    let (_dir, report, id) = report(&uuid::Uuid::new_v4().to_string())?;
    let case = Case::reporting(report).await?;
    let (text, document) = document(&case).await?;
    assert!(!text.contains(ENVELOPE_MARKER), "{text}");
    let readers: Vec<&str> = document
        .consumers
        .iter()
        .map(|consumer| consumer.reader.as_str())
        .collect();
    assert_eq!(vec!["ferrobridge", "ferrobridge-second"], readers);
    let [events, second] = document.consumers.as_slice() else {
        return Err("two consumers".into());
    };
    assert_eq!(
        Vec::<ferrobridge_operator_api::events::Parked>::new(),
        second.parked
    );
    let parked = events.parked.first().ok_or("one parked event")?;
    assert_eq!(id, parked.id);
    assert_eq!(
        ("clinical", "7"),
        (parked.position.stream.as_str(), parked.position.at.as_str())
    );
    assert_eq!("the event names no template", parked.cause);
    assert_eq!(5, parked.attempts);
    assert!(!parked.replay_requested);
    Ok(())
}

#[tokio::test]
async fn a_replay_marks_the_parked_event() -> Result<(), Box<dyn Error>> {
    let (_dir, report, id) = report(&uuid::Uuid::new_v4().to_string())?;
    let case = Case::reporting(report).await?;
    let response = case
        .post(
            &format!("/operator/events/events/parked/{id}/replay"),
            OPERATOR,
        )
        .await?;
    assert_eq!(StatusCode::ACCEPTED, response.status());
    let (_text, document) = document(&case).await?;
    let parked = document
        .consumers
        .first()
        .and_then(|consumer| consumer.parked.first())
        .ok_or("the parked event")?;
    assert!(parked.replay_requested);
    Ok(())
}

#[tokio::test]
async fn a_replay_of_an_unknown_consumer_or_event_is_not_found() -> Result<(), Box<dyn Error>> {
    let (_dir, report, id) = report(&uuid::Uuid::new_v4().to_string())?;
    let case = Case::reporting(report).await?;
    for path in [
        format!("/operator/events/nobody/parked/{id}/replay"),
        format!("/operator/events/change-feed-second/parked/{id}/replay"),
        String::from("/operator/events/events/parked/999/replay"),
        String::from("/operator/events/events/parked/seven/replay"),
    ] {
        let response = case.post(&path, OPERATOR).await?;
        assert_eq!(StatusCode::NOT_FOUND, response.status(), "{path}");
    }
    Ok(())
}

#[tokio::test]
async fn a_replay_needs_the_operator_scope() -> Result<(), Box<dyn Error>> {
    let (_dir, report, id) = report(&uuid::Uuid::new_v4().to_string())?;
    let case = Case::reporting(report).await?;
    let response = case
        .post(
            &format!("/operator/events/events/parked/{id}/replay"),
            "patient/*.read",
        )
        .await?;
    assert_eq!(StatusCode::FORBIDDEN, response.status());
    let (_text, document) = document(&case).await?;
    let parked = document
        .consumers
        .first()
        .and_then(|consumer| consumer.parked.first())
        .ok_or("the parked event")?;
    assert!(!parked.replay_requested);
    Ok(())
}
