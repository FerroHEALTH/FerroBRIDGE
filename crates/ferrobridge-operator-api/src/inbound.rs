// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The inbound intake's parked work of one source, how an operator settles
//! it, and what a replay runs again.
//!
//! A parked item carries a fixed reason that names no value of the record.
//! A settlement records who decided and why.

use serde::{Deserialize, Serialize};

/// `GET /operator/sources/{id}/parked`: the parked items, oldest first.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ParkedItems {
    /// The items.
    pub items: Vec<ParkedItem>,
}

/// One parked item.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ParkedItem {
    /// The tenant as the source document names it.
    pub tenant: String,
    /// The entity as the source names it.
    pub entity: String,
    /// The fixed reason it parked.
    pub reason: String,
    /// The attempts made.
    pub attempts: u32,
    /// When it parked, RFC 3339 UTC.
    pub since: String,
}

/// How a parked item is settled.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Settlement {
    /// Run it again.
    Retry,
    /// Leave it unapplied for good.
    Discard,
}

/// `POST /operator/sources/{id}/settle`: the body.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Settle {
    /// The tenant of the parked item.
    pub tenant: String,
    /// Its entity.
    pub entity: String,
    /// How it is settled.
    pub settlement: Settlement,
    /// Why, recorded with the operator who decided.
    pub reason: String,
}

/// `POST /operator/sources/{id}/replay`: the body, one entity or a time
/// range of received events.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged, deny_unknown_fields)]
pub enum Replay {
    /// One entity.
    Entity {
        /// The tenant.
        tenant: String,
        /// The entity.
        entity: String,
    },
    /// Every entity an event received in `[from, to)` named.
    Received {
        /// The first instant, RFC 3339, inclusive.
        from: String,
        /// The last instant, RFC 3339, exclusive.
        to: String,
    },
}

/// The answer to a replay.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Replayed {
    /// The work items that run again.
    pub items: u64,
}
