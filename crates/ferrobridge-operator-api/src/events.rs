// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The change-feed consumers and the AMQP topology `GET /operator/events`
//! answers.
//!
//! Each consumer names the source it follows and the reader it follows it
//! under, how far it has finished, its health, what it published since the
//! bridge started, the last failure with its cause and the events it parked
//! in the dead-letter store. The topology is the one `[events]` declares. The
//! document carries identifiers, instants and counts, never an event body.
//! No specification governs this document: our own design.

use serde::{Deserialize, Serialize};

/// What `GET /operator/events` answers with.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Events {
    /// The consumers that follow the change feed; empty when none runs.
    pub consumers: Vec<Consumer>,
    /// The AMQP topology of the events lane, when it runs.
    pub topology: Option<Topology>,
}

/// One consumer of the change feed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Consumer {
    /// The consumer's name, which its readiness indicator carries.
    pub name: String,
    /// The source it follows.
    pub source: Source,
    /// The reader name it follows the source under.
    pub reader: String,
    /// How far it has finished, one entry per stream of the source it has
    /// a position in.
    pub positions: Vec<Position>,
    /// Where it stands.
    pub state: State,
    /// The messages it published since the bridge started.
    pub published: u64,
    /// When it last published, RFC 3339 UTC.
    pub last_published: Option<String>,
    /// The last pass that failed, kept after the consumer recovers.
    pub last_error: Option<Failure>,
    /// The events it parked in the dead-letter store, in the order they were
    /// first parked.
    pub parked: Vec<Parked>,
}

/// One event a consumer parked after its bound of poisoned passes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Parked {
    /// The number the store parked it under, unique per consumer.
    pub id: u64,
    /// Where it sits in the source.
    pub position: Position,
    /// The contribution it names, absent on an erasure.
    pub contribution_id: Option<String>,
    /// The EHR it names, absent for a demographic contribution.
    pub ehr_id: Option<String>,
    /// Whether it is an erasure.
    pub erasure: bool,
    /// One entry per changed versioned object.
    pub versions: Vec<ParkedVersion>,
    /// The last failure and every cause behind it, in one line with no event
    /// body.
    pub cause: String,
    /// How many poisoned passes it failed, replays included.
    pub attempts: u32,
    /// When it was parked, or parked again after a replay failed, RFC 3339
    /// UTC.
    pub at: String,
    /// Whether a replay is requested and has not run yet.
    pub replay_requested: bool,
}

/// One changed versioned object of a parked event.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ParkedVersion {
    /// The RM type name, such as `COMPOSITION`.
    pub kind: String,
    /// The audit change type, as the source writes it.
    pub change_type: String,
    /// The operational template of a composition, when there is one.
    pub template_id: Option<String>,
}

/// The source a consumer follows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Source {
    /// The vendor-neutral poll of ITS-REST `/query/aql`, with a watermark the
    /// bridge keeps.
    Aql,
    /// The reference CDR's outbox cursor feed, whose reader cursor is the
    /// watermark.
    Ferroehr,
}

/// How far a consumer has finished in one stream of its source.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Position {
    /// The stream: `clinical` or `party` for an outbox of the reference CDR,
    /// `committed` for the AQL source.
    pub stream: String,
    /// The position: the last acknowledged `seq`, or the watermark instant
    /// as the CDR wrote it.
    pub at: String,
    /// On the AQL source, how many versions committed at the watermark
    /// instant are finished.
    pub versions_at_instant: Option<u64>,
}

/// Where a consumer stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum State {
    /// No pass has completed since the bridge started.
    Starting,
    /// The last pass completed.
    Following,
    /// The last pass failed; the events wait at the source.
    Stalled,
}

/// One failed pass.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Failure {
    /// When it failed, RFC 3339 UTC.
    pub at: String,
    /// The failure and every cause behind it, in one line with no event
    /// body.
    pub cause: String,
}

/// The AMQP topology of the events lane.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Topology {
    /// The durable topic exchange.
    pub exchange: String,
    /// The grammar of the routing key each message carries.
    pub routing_key: String,
    /// The durable queues bound to the exchange.
    pub queues: Vec<Queue>,
}

/// One durable queue bound to the exchange.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Queue {
    /// The queue name.
    pub name: String,
    /// The topic binding key.
    pub binding: String,
}
