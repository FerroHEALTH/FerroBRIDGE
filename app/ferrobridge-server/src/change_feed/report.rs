// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The running change-feed consumers, as the operator API reports them.
//!
//! The document `GET /operator/events` answers holds every consumer with its
//! parked events and the AMQP topology of the events lane, and `POST
//! /operator/events/{consumer}/parked/{id}/replay` requests a replay.
//!
//! No specification governs the operator surface: our own design.

use std::sync::Arc;

use ferrobridge_operator_api::events as document;

use crate::change_feed::Position;
use crate::change_feed::ReaderName;
use crate::change_feed::Source;
use crate::change_feed::dead_letter::DeadLetterError;
use crate::change_feed::dead_letter::DeadLetters;
use crate::change_feed::dead_letter::Parked;
use crate::change_feed::health::Following;
use crate::change_feed::health::State;

/// One running consumer.
#[derive(Debug, Clone)]
pub struct Follower {
    /// The consumer's name, which its indicator carries.
    pub name: &'static str,
    /// The reader it follows the source under.
    pub reader: ReaderName,
    /// Where it stands.
    pub following: Following,
}

/// The running consumers of one change feed.
#[derive(Debug, Clone)]
pub struct Report {
    /// The source they follow.
    source: Source,
    /// The store their parked events are in.
    dead_letters: Arc<DeadLetters>,
    /// The consumers, in the order they started.
    followers: Vec<Follower>,
    /// The events lane, when it runs.
    events: Option<crate::events::Settings>,
}

/// What a replay request found.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Replay {
    /// The event is marked; its follower offers it again before its next
    /// page.
    Requested,
    /// No running consumer has the name.
    UnknownConsumer,
    /// The consumer has no parked event with the number.
    UnknownEvent,
}

impl Report {
    /// Returns the report of no consumer yet over `source`, whose parked
    /// events `dead_letters` holds.
    #[must_use]
    pub const fn new(source: Source, dead_letters: Arc<DeadLetters>) -> Self {
        Self {
            source,
            dead_letters,
            followers: Vec::new(),
            events: None,
        }
    }

    /// Returns this report with `follower` among its consumers.
    #[must_use]
    pub fn with_follower(mut self, follower: Follower) -> Self {
        self.followers.push(follower);
        self
    }

    /// Returns this report with the AMQP topology of the events lane
    /// `settings`.
    #[must_use]
    pub fn with_events(mut self, settings: crate::events::Settings) -> Self {
        self.events = Some(settings);
        self
    }

    /// Returns the document of every consumer.
    ///
    /// # Errors
    /// Returns [`DeadLetterError`] when the parked events cannot be read.
    pub fn document(&self) -> Result<document::Events, DeadLetterError> {
        let consumers = self
            .followers
            .iter()
            .map(|follower| self.consumer(follower))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(document::Events {
            consumers,
            topology: self.events.as_ref().map(|settings| document::Topology {
                exchange: settings.exchange.clone(),
                routing_key: String::from(crate::events::ROUTING_KEY),
                queues: settings
                    .queues
                    .iter()
                    .map(|queue| document::Queue {
                        name: queue.name.clone(),
                        binding: queue.binding.clone(),
                    })
                    .collect(),
            }),
        })
    }

    /// Requests the replay of the event `id` parked for the consumer `name`.
    ///
    /// # Errors
    /// Returns [`DeadLetterError`] when the store refuses the write.
    pub fn replay(&self, name: &str, id: u64) -> Result<Replay, DeadLetterError> {
        let Some(follower) = self.followers.iter().find(|follower| follower.name == name) else {
            return Ok(Replay::UnknownConsumer);
        };
        Ok(if self.dead_letters.request_replay(&follower.reader, id)? {
            Replay::Requested
        } else {
            Replay::UnknownEvent
        })
    }

    /// Returns the document entry of `follower`.
    fn consumer(&self, follower: &Follower) -> Result<document::Consumer, DeadLetterError> {
        let status = follower.following.status();
        Ok(document::Consumer {
            name: String::from(follower.name),
            source: match self.source {
                Source::Aql { .. } => document::Source::Aql,
                Source::FerroEhr { .. } => document::Source::Ferroehr,
            },
            reader: follower.reader.as_str().to_owned(),
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
            parked: self
                .dead_letters
                .parked(&follower.reader)?
                .iter()
                .map(parked)
                .collect(),
        })
    }
}

/// Returns `entry` as the document writes it: identifiers and the cause,
/// never the envelope.
fn parked(entry: &Parked) -> document::Parked {
    document::Parked {
        id: entry.id,
        position: position(&entry.event.position),
        contribution_id: entry
            .event
            .contribution_id
            .as_ref()
            .map(|id| id.as_str().to_owned()),
        ehr_id: entry.event.ehr_id.as_ref().map(|id| id.as_str().to_owned()),
        erasure: entry.event.erasure,
        versions: entry
            .event
            .versions
            .iter()
            .map(|version| document::ParkedVersion {
                kind: version.kind.clone(),
                change_type: version.change_type.clone(),
                template_id: version.template_id.clone(),
            })
            .collect(),
        cause: entry.cause.clone(),
        attempts: entry.attempts,
        at: entry.at.to_string(),
        replay_requested: entry.replay,
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
