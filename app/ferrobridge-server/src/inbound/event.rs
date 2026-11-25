// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! What a verified event says: its tenant, its entity, the change it asks
//! for and the key that recognizes it when it arrives again.
//!
//! A source's event map selects each part by JSON Pointer (RFC 6901) and
//! picks the action by the first rule whose every pointer holds its value.
//! The de-duplication key is the SHA-256 of the parts the source declares:
//! its event id where it has one (Standard Webhooks `webhook-id`, `CloudEvents`
//! 1.0.2 `source` and `id`, `MessageHeader.id`, MSH-10), or the fields that
//! together tell one event from another where it has none. A refusal names
//! the part that failed, never a value of the event.

use std::collections::BTreeMap;

use http::HeaderMap;
use serde::Deserialize;
use serde_json::Value;
use sha2::{Digest as _, Sha256};

/// How a source's events are read, as its document declares it.
#[derive(Debug, Clone, PartialEq, Eq, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct EventMap {
    /// The pointer to the tenant, absent when the source serves one tenant.
    pub tenant: Option<String>,
    /// The pointer to the entity the event is about.
    pub entity: String,
    /// The rules that pick the action, the first match winning.
    pub rules: Vec<Rule>,
    /// The action of an event no rule matches.
    pub otherwise: Action,
    /// The parts the de-duplication key is taken over.
    pub dedupe: Dedupe,
    /// The pointer to the source's own stamp of the change, which orders two
    /// changes of one entity.
    pub stamp: Option<String>,
}

/// One rule of an event map.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Rule {
    /// The value each pointer must hold.
    pub when: BTreeMap<String, Value>,
    /// The action of an event that matches.
    pub action: Action,
}

/// What an event asks of the engine.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Action {
    /// The entity was created or changed.
    Upsert,
    /// The entity was removed at the source.
    Remove,
    /// The event concerns nothing the engine serves.
    #[default]
    Ignore,
    /// The source tests the endpoint.
    Probe,
}

/// The parts of a request the de-duplication key is taken over.
#[derive(Debug, Clone, PartialEq, Eq, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Dedupe {
    /// Headers, in order.
    pub headers: Vec<String>,
    /// Pointers into the body, in order, after the headers.
    pub pointers: Vec<String>,
}

/// A change an event asks the engine to make.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Change {
    /// Create or update the entity.
    Upsert,
    /// The entity was removed at the source.
    Remove,
}

/// The SHA-256 key that recognizes an event when it arrives again.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DedupeKey([u8; 32]);

impl DedupeKey {
    /// Returns the key's bytes.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

impl std::fmt::Debug for DedupeKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("DedupeKey(")?;
        for byte in self.0 {
            write!(f, "{byte:02x}")?;
        }
        f.write_str(")")
    }
}

/// An event the engine stores.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Event {
    /// The change it asks for.
    pub change: Change,
    /// The tenant, `None` for a source that serves one.
    pub tenant: Option<String>,
    /// The entity as the source names it.
    pub entity: String,
    /// The key that recognizes it again.
    pub dedupe_key: DedupeKey,
    /// The source's stamp of the change.
    pub stamp: Option<String>,
}

/// What reading an event decides.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Reading {
    /// Store the event; the engine acts on it.
    Store(Event),
    /// Answer the source and store nothing.
    Answer(Answered),
}

/// Why an event is answered without being stored.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Answered {
    /// The source tested the endpoint.
    Probe,
    /// The event concerns nothing the engine serves.
    Ignored,
}

/// The part of an event that could not be read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Part {
    /// The tenant.
    Tenant,
    /// The entity.
    Entity,
    /// A part of the de-duplication key.
    Dedupe,
    /// The source's stamp.
    Stamp,
}

impl std::fmt::Display for Part {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Tenant => "tenant",
            Self::Entity => "entity",
            Self::Dedupe => "de-duplication key",
            Self::Stamp => "stamp",
        })
    }
}

/// Why an event could not be read, naming no value of it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum Unreadable {
    /// A part the event map selects is absent.
    #[error("the event carries no {0}")]
    Missing(Part),
    /// A part is neither a string nor a number, or is empty.
    #[error("the event's {0} is not a string or a number")]
    NotScalar(Part),
}

impl EventMap {
    /// Reads the event `body` and `headers` carry.
    ///
    /// # Errors
    /// Returns [`Unreadable`] when an event to be stored lacks a part the
    /// map selects, or holds one that is no string or number.
    pub fn read(&self, headers: &HeaderMap, body: &Value) -> Result<Reading, Unreadable> {
        let action = self
            .rules
            .iter()
            .find(|rule| {
                rule.when
                    .iter()
                    .all(|(pointer, value)| body.pointer(pointer) == Some(value))
            })
            .map_or(self.otherwise, |rule| rule.action);
        let change = match action {
            Action::Probe => return Ok(Reading::Answer(Answered::Probe)),
            Action::Ignore => return Ok(Reading::Answer(Answered::Ignored)),
            Action::Upsert => Change::Upsert,
            Action::Remove => Change::Remove,
        };
        let tenant = self
            .tenant
            .as_deref()
            .map(|pointer| scalar(body, pointer, Part::Tenant))
            .transpose()?;
        let entity = scalar(body, &self.entity, Part::Entity)?;
        let stamp = self
            .stamp
            .as_deref()
            .map(|pointer| scalar(body, pointer, Part::Stamp))
            .transpose()?;
        Ok(Reading::Store(Event {
            change,
            tenant,
            entity,
            dedupe_key: self.dedupe.key(headers, body)?,
            stamp,
        }))
    }
}

impl Dedupe {
    /// Returns the SHA-256 over the JSON array of the declared parts, each
    /// header as a string and each body part as the JSON value it holds.
    fn key(&self, headers: &HeaderMap, body: &Value) -> Result<DedupeKey, Unreadable> {
        let mut parts = Vec::with_capacity(self.headers.len() + self.pointers.len());
        for name in &self.headers {
            let value = headers
                .get(name.as_str())
                .and_then(|value| value.to_str().ok())
                .filter(|value| !value.is_empty())
                .ok_or(Unreadable::Missing(Part::Dedupe))?;
            parts.push(Value::String(value.to_owned()));
        }
        for pointer in &self.pointers {
            let value = body
                .pointer(pointer)
                .ok_or(Unreadable::Missing(Part::Dedupe))?;
            parts.push(value.clone());
        }
        if parts.is_empty() {
            return Err(Unreadable::Missing(Part::Dedupe));
        }
        Ok(DedupeKey(
            Sha256::digest(Value::Array(parts).to_string().as_bytes()).into(),
        ))
    }
}

/// Returns the string or number at `pointer` as text.
fn scalar(body: &Value, pointer: &str, part: Part) -> Result<String, Unreadable> {
    match body.pointer(pointer) {
        None | Some(Value::Null) => Err(Unreadable::Missing(part)),
        Some(Value::String(text)) if !text.is_empty() => Ok(text.clone()),
        Some(Value::Number(number)) => Ok(number.to_string()),
        Some(_) => Err(Unreadable::NotScalar(part)),
    }
}
