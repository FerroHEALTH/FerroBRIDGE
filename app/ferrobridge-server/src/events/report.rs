// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The document `GET /operator/events` answers: the consumers of the change
//! feed and the AMQP topology of the events lane.
//!
//! No specification governs the operator surface: our own design.

use ferrobridge_operator_api::events as document;

use crate::change_feed::Position;
use crate::change_feed::Source;
use crate::change_feed::health::Following;
use crate::change_feed::health::State;
use crate::events::Settings;

/// The routing-key grammar every message of the lane carries.
pub const ROUTING_KEY: &str = "<kind>.<change_type>.<template_id>";

/// The running events lane, as the operator API reports it.
#[derive(Debug, Clone)]
pub struct Lane {
    /// The lane's settings.
    pub settings: Settings,
    /// The change-feed source it follows.
    pub source: Source,
    /// Where it stands.
    pub following: Following,
}

/// Returns the document of `lane`, an empty one when no lane runs.
#[must_use]
pub fn document(lane: Option<&Lane>) -> document::Events {
    let Some(lane) = lane else {
        return document::Events::default();
    };
    let status = lane.following.status();
    let consumer = document::Consumer {
        name: String::from(crate::events::NAME),
        source: match lane.source {
            Source::Aql { .. } => document::Source::Aql,
            Source::FerroEhr { .. } => document::Source::Ferroehr,
        },
        reader: lane.settings.reader.as_str().to_owned(),
        positions: status.positions.values().map(position).collect(),
        state: match status.state {
            State::Starting => document::State::Starting,
            State::Following => document::State::Following,
            State::Stalled(_) => document::State::Stalled,
        },
        published: status.delivered,
        last_published: status.last_delivered.map(|at| at.to_string()),
        last_error: status.last_failure.map(|failure| document::Failure {
            at: failure.at.to_string(),
            cause: failure.cause,
        }),
    };
    document::Events {
        consumers: vec![consumer],
        topology: Some(document::Topology {
            exchange: lane.settings.exchange.clone(),
            routing_key: String::from(ROUTING_KEY),
            queues: lane
                .settings
                .queues
                .iter()
                .map(|queue| document::Queue {
                    name: queue.name.clone(),
                    binding: queue.binding.clone(),
                })
                .collect(),
        }),
    }
}

/// Returns `position` as the document writes it.
fn position(position: &Position) -> document::Position {
    match position {
        Position::Outbox { domain, seq } => document::Position {
            stream: domain.as_str().to_owned(),
            at: seq.to_string(),
            versions_at_instant: None,
        },
        Position::Committed(committed) => document::Position {
            stream: String::from("committed"),
            at: committed.text().to_owned(),
            // NOTE: no specification governs this: our own design; a usize fits a u64 on
            // every target the server builds for, so the clamp never bites.
            versions_at_instant: Some(
                u64::try_from(committed.versions().len()).unwrap_or(u64::MAX),
            ),
        },
    }
}
