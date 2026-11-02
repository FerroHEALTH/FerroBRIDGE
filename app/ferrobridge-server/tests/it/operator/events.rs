// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! `GET /operator/events`: the change-feed consumers and the AMQP topology,
//! empty when the events lane is off, and never the broker URL or a body.

use std::error::Error;
use std::time::Duration;

use ferrobridge_operator_api::events::{Events, Source, State};
use ferrobridge_server::change_feed::health::Following;
use ferrobridge_server::change_feed::{Domain, Position, ReaderName, Seq};
use ferrobridge_server::events::report::Lane;
use ferrobridge_server::events::{Queue, Settings};
use http::StatusCode;
use secrecy::SecretString;

use super::{Case, OPERATOR, bytes};

/// Returns a lane over the reference CDR's feed, stalled after one
/// acknowledged event and one published message, whose broker URL carries
/// `password`.
fn lane(password: &str) -> Result<Lane, Box<dyn Error>> {
    let following = Following::new(ferrobridge_server::events::NAME);
    following.acknowledged(&[Position::Outbox {
        domain: Domain::Clinical,
        seq: Seq::new(42),
    }]);
    following.delivered();
    following.stalled("the AMQP broker could not be reached");
    Ok(Lane {
        settings: Settings {
            reader: ReaderName::new("ferrobridge")?,
            url: SecretString::from(format!("amqp://bridge:{password}@broker.invalid/%2f")),
            exchange: String::from("ferroehr.events"),
            queues: vec![Queue {
                name: String::from("ferroehr.events.analytics"),
                binding: String::from("COMPOSITION.*.*"),
            }],
            confirm_timeout: Duration::from_secs(30),
        },
        source: ferrobridge_server::change_feed::Source::FerroEhr {
            root: "https://ehr.invalid/ferroehr/rest/extensions/v1".parse()?,
        },
        following,
    })
}

#[tokio::test]
async fn a_bridge_with_no_events_lane_answers_an_empty_document() -> Result<(), Box<dyn Error>> {
    let case = Case::new().await?;
    let response = case.get("/operator/events", Some(OPERATOR)).await?;
    assert_eq!(StatusCode::OK, response.status());
    let document: Events = serde_json::from_slice(&bytes(response).await?)?;
    assert_eq!(Events::default(), document);
    Ok(())
}

#[tokio::test]
async fn a_running_lane_reports_its_consumer_and_topology() -> Result<(), Box<dyn Error>> {
    let password = uuid::Uuid::new_v4().to_string();
    let case = Case::reporting(lane(&password)?).await?;
    let response = case.get("/operator/events", Some(OPERATOR)).await?;
    assert_eq!(StatusCode::OK, response.status());
    let bytes = bytes(response).await?;
    let text = String::from_utf8(bytes.clone())?;
    assert!(!text.contains(&password), "{text}");
    let document: Events = serde_json::from_slice(&bytes)?;
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
