// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The dead-letter store of the change feed, a `redb` file beside the
//! watermarks.
//!
//! It holds the events a consumer failed with a poisoned failure on the
//! bound of passes, parked with their cause, and the count of poisoned
//! passes of each event still in the feed.
//!
//! A parked event is never dropped: it stays in the store, keyed by reader
//! and a number the store assigns, until a replay finishes it. A replay
//! request marks it, and the follower offers it to its consumer again before
//! its next page. Each write is one ACID transaction
//! (<https://docs.rs/redb/4.3.0/redb/>). The rows hold an event's
//! identifiers and provenance as the feed served them, never clinical
//! content. No specification governs this: our own design.

use std::collections::BTreeSet;
use std::path::Path;

use redb::Database;
use redb::ReadableDatabase;
use redb::ReadableTable;
use redb::TableDefinition;
use serde::Deserialize;
use serde::Serialize;

use crate::cdr::ids::ContributionUid;
use crate::cdr::ids::EhrId;
use crate::change_feed::ChangedVersion;
use crate::change_feed::Committed;
use crate::change_feed::Domain;
use crate::change_feed::Event;
use crate::change_feed::Position;
use crate::change_feed::ReaderName;
use crate::change_feed::Seq;

/// Reader and event position to the poisoned passes the event failed.
const ATTEMPTS: TableDefinition<'static, (&str, &str), u32> =
    TableDefinition::new("poisoned_attempts");

/// Reader and parked number to the parked event, as JSON.
const PARKED: TableDefinition<'static, (&str, u64), &str> = TableDefinition::new("parked_event");

/// The one row holding the next parked number.
const NEXT: TableDefinition<'static, &str, u64> = TableDefinition::new("parked_next");

/// The key of the next parked number.
const NEXT_KEY: &str = "next";

/// Why the dead-letter store refused a call.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum DeadLetterError {
    /// The store refused a read or a write.
    #[error("the dead-letter store refused the call")]
    Store {
        /// What `redb` reported.
        #[source]
        source: Box<redb::Error>,
    },
    /// A stored row does not read, or an event cannot be written as one.
    #[error("a row of the dead-letter store does not read")]
    Row {
        /// The JSON error behind it.
        #[source]
        source: serde_json::Error,
    },
    /// A stored row reads, and one of its values does not.
    #[error("a row of the dead-letter store holds {what} that does not read")]
    Value {
        /// What does not read.
        what: &'static str,
    },
    /// The store has assigned every parked number.
    #[error("the dead-letter store has no parked number left")]
    Exhausted,
}

/// Returns the store error `source` is.
fn store(source: impl Into<redb::Error>) -> DeadLetterError {
    DeadLetterError::Store {
        source: Box::new(source.into()),
    }
}

/// Returns the row error `source` is.
const fn row(source: serde_json::Error) -> DeadLetterError {
    DeadLetterError::Row { source }
}

/// One parked event.
#[derive(Debug, Clone)]
pub struct Parked {
    /// The number the store assigned it, unique per reader.
    pub id: u64,
    /// The event as the feed served it.
    pub event: Event,
    /// The last failure and its causes, in one line with no event body.
    pub cause: String,
    /// How many poisoned passes it failed, replays included.
    pub attempts: u32,
    /// When it was parked, or parked again after a replay failed.
    pub at: jiff::Timestamp,
    /// Whether a replay is requested and has not run yet.
    pub replay: bool,
}

/// The parked events and the poisoned-pass counts of every reader, on disk.
#[derive(Debug)]
pub struct DeadLetters {
    /// The open database.
    database: Database,
}

impl DeadLetters {
    /// Opens, or creates, the store at `path`.
    ///
    /// # Errors
    /// Returns [`DeadLetterError::Store`] when the file cannot be opened or
    /// created or its tables cannot be created.
    pub fn open(path: &Path) -> Result<Self, DeadLetterError> {
        let database = Database::create(path).map_err(store)?;
        let write = database.begin_write().map_err(store)?;
        write.open_table(ATTEMPTS).map_err(store).map(drop)?;
        write.open_table(PARKED).map_err(store).map(drop)?;
        write.open_table(NEXT).map_err(store).map(drop)?;
        write.commit().map_err(store)?;
        Ok(Self { database })
    }

    /// Counts one more poisoned pass of the event at `position` for
    /// `reader`, and returns the count.
    ///
    /// # Errors
    /// Returns [`DeadLetterError`] when the write does not commit.
    pub fn charge(&self, reader: &ReaderName, position: &Position) -> Result<u32, DeadLetterError> {
        let key = key(position)?;
        let write = self.database.begin_write().map_err(store)?;
        let count = {
            let mut table = write.open_table(ATTEMPTS).map_err(store)?;
            let before = table
                .get((reader.as_str(), key.as_str()))
                .map_err(store)?
                .map_or(0, |count| count.value());
            let count = before.saturating_add(1);
            table
                .insert((reader.as_str(), key.as_str()), count)
                .map_err(store)?;
            count
        };
        write.commit().map_err(store)?;
        Ok(count)
    }

    /// Forgets the poisoned passes of the event at `position` for `reader`,
    /// once it finished.
    ///
    /// # Errors
    /// Returns [`DeadLetterError`] when the write does not commit.
    pub fn settle(&self, reader: &ReaderName, position: &Position) -> Result<(), DeadLetterError> {
        let key = key(position)?;
        let write = self.database.begin_write().map_err(store)?;
        {
            let mut table = write.open_table(ATTEMPTS).map_err(store)?;
            table
                .remove((reader.as_str(), key.as_str()))
                .map_err(store)?;
        }
        write.commit().map_err(store)
    }

    /// Parks `event` for `reader` after `attempts` poisoned passes, the last
    /// for `cause`, and forgets its count, in one transaction; returns the
    /// number it is parked under.
    ///
    /// # Errors
    /// Returns [`DeadLetterError`] when the write does not commit or the
    /// event cannot be written.
    pub fn park(
        &self,
        reader: &ReaderName,
        event: &Event,
        cause: &str,
        attempts: u32,
    ) -> Result<u64, DeadLetterError> {
        let key = key(&event.position)?;
        let text = serde_json::to_string(&Stored {
            event: StoredEvent::from(event),
            cause: cause.to_owned(),
            attempts,
            parked_at: jiff::Timestamp::now().to_string(),
            replay: false,
        })
        .map_err(row)?;
        let write = self.database.begin_write().map_err(store)?;
        let id = {
            let mut next = write.open_table(NEXT).map_err(store)?;
            let id = next
                .get(NEXT_KEY)
                .map_err(store)?
                .map_or(1, |value| value.value());
            let after = id.checked_add(1).ok_or(DeadLetterError::Exhausted)?;
            next.insert(NEXT_KEY, after).map_err(store)?;
            let mut parked = write.open_table(PARKED).map_err(store)?;
            parked
                .insert((reader.as_str(), id), text.as_str())
                .map_err(store)?;
            let mut counts = write.open_table(ATTEMPTS).map_err(store)?;
            counts
                .remove((reader.as_str(), key.as_str()))
                .map_err(store)?;
            id
        };
        write.commit().map_err(store)?;
        Ok(id)
    }

    /// Returns the events parked for `reader`, in the order they were first
    /// parked.
    ///
    /// # Errors
    /// Returns [`DeadLetterError`] when the read fails or a row does not
    /// read.
    pub fn parked(&self, reader: &ReaderName) -> Result<Vec<Parked>, DeadLetterError> {
        let read = self.database.begin_read().map_err(store)?;
        let table = read.open_table(PARKED).map_err(store)?;
        let mut parked = Vec::new();
        for entry in table
            .range((reader.as_str(), 0)..=(reader.as_str(), u64::MAX))
            .map_err(store)?
        {
            let (key, value) = entry.map_err(store)?;
            parked.push(decode(key.value().1, value.value())?);
        }
        Ok(parked)
    }

    /// Marks the event `id` parked for `reader` for a replay; returns
    /// whether the store holds it.
    ///
    /// # Errors
    /// Returns [`DeadLetterError`] when the write does not commit or the row
    /// does not read.
    pub fn request_replay(&self, reader: &ReaderName, id: u64) -> Result<bool, DeadLetterError> {
        self.rewrite(reader, id, |stored| stored.replay = true)
    }

    /// Removes the event `id` parked for `reader`, which a replay finished.
    ///
    /// # Errors
    /// Returns [`DeadLetterError`] when the write does not commit.
    pub fn replayed(&self, reader: &ReaderName, id: u64) -> Result<(), DeadLetterError> {
        let write = self.database.begin_write().map_err(store)?;
        {
            let mut table = write.open_table(PARKED).map_err(store)?;
            table.remove((reader.as_str(), id)).map_err(store)?;
        }
        write.commit().map_err(store)
    }

    /// Parks the event `id` of `reader` again after its replay failed for
    /// `cause`: one more attempt, and no replay requested.
    ///
    /// # Errors
    /// Returns [`DeadLetterError`] when the write does not commit or the row
    /// does not read.
    pub fn repark(&self, reader: &ReaderName, id: u64, cause: &str) -> Result<(), DeadLetterError> {
        self.rewrite(reader, id, |stored| {
            stored.attempts = stored.attempts.saturating_add(1);
            cause.clone_into(&mut stored.cause);
            stored.parked_at = jiff::Timestamp::now().to_string();
            stored.replay = false;
        })
        .map(|_held| ())
    }

    /// Changes the row of `id` for `reader` with `change`; returns whether
    /// the store holds it.
    fn rewrite(
        &self,
        reader: &ReaderName,
        id: u64,
        change: impl FnOnce(&mut Stored),
    ) -> Result<bool, DeadLetterError> {
        let write = self.database.begin_write().map_err(store)?;
        let held = {
            let mut table = write.open_table(PARKED).map_err(store)?;
            let stored = match table.get((reader.as_str(), id)).map_err(store)? {
                Some(value) => Some(serde_json::from_str::<Stored>(value.value()).map_err(row)?),
                None => None,
            };
            match stored {
                Some(mut stored) => {
                    change(&mut stored);
                    let text = serde_json::to_string(&stored).map_err(row)?;
                    table
                        .insert((reader.as_str(), id), text.as_str())
                        .map_err(store)?;
                    true
                }
                None => false,
            }
        };
        write.commit().map_err(store)?;
        Ok(held)
    }
}

/// Returns the key an event's position is counted under.
fn key(position: &Position) -> Result<String, DeadLetterError> {
    serde_json::to_string(&StoredPosition::from(position)).map_err(row)
}

/// Returns the parked event `text` holds under `id`.
fn decode(id: u64, text: &str) -> Result<Parked, DeadLetterError> {
    let stored: Stored = serde_json::from_str(text).map_err(row)?;
    let parked_at = stored
        .parked_at
        .parse()
        .map_err(|_refused| DeadLetterError::Value {
            what: "a parking instant",
        })?;
    Ok(Parked {
        id,
        event: stored.event.into_event()?,
        cause: stored.cause,
        attempts: stored.attempts,
        at: parked_at,
        replay: stored.replay,
    })
}

/// One parked event as it is stored.
#[derive(Debug, Serialize, Deserialize)]
struct Stored {
    /// The event.
    event: StoredEvent,
    /// The last failure.
    cause: String,
    /// The poisoned passes.
    attempts: u32,
    /// When it was parked, RFC 3339.
    parked_at: String,
    /// Whether a replay is requested.
    replay: bool,
}

/// An event as it is stored.
#[derive(Debug, Serialize, Deserialize)]
struct StoredEvent {
    /// Where it sits in its source.
    position: StoredPosition,
    /// Whether it is an erasure.
    erasure: bool,
    /// The contribution.
    contribution_id: Option<String>,
    /// The EHR.
    ehr_id: Option<String>,
    /// The commit instant, RFC 3339.
    committed_at: Option<String>,
    /// The changed versions.
    versions: Vec<StoredVersion>,
    /// The event as the source served it.
    envelope: serde_json::Map<String, serde_json::Value>,
}

/// A position as it is stored.
#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "stream", rename_all = "snake_case")]
enum StoredPosition {
    /// A sequence number in one outbox.
    Outbox {
        /// The outbox.
        domain: String,
        /// The sequence number.
        seq: u64,
    },
    /// A commit instant and the versions at it.
    Committed {
        /// The instant as the CDR wrote it.
        at: String,
        /// The versions finished at it.
        versions: BTreeSet<String>,
    },
}

/// One changed version as it is stored.
#[derive(Debug, Serialize, Deserialize)]
struct StoredVersion {
    /// The versioned object.
    vo_id: Option<uuid::Uuid>,
    /// The RM type name.
    kind: String,
    /// The version ordinal.
    sys_version: Option<u64>,
    /// The version tree id.
    version_tree_id: Option<String>,
    /// The audit change type.
    change_type: String,
    /// The template.
    template_id: Option<String>,
}

impl From<&Position> for StoredPosition {
    fn from(position: &Position) -> Self {
        match position {
            Position::Outbox { domain, seq } => Self::Outbox {
                domain: domain.as_str().to_owned(),
                seq: seq.get(),
            },
            Position::Committed(committed) => Self::Committed {
                at: committed.text().to_owned(),
                versions: committed.versions().clone(),
            },
        }
    }
}

impl From<&Event> for StoredEvent {
    fn from(event: &Event) -> Self {
        Self {
            position: StoredPosition::from(&event.position),
            erasure: event.erasure,
            contribution_id: event
                .contribution_id
                .as_ref()
                .map(|id| id.as_str().to_owned()),
            ehr_id: event.ehr_id.as_ref().map(|id| id.as_str().to_owned()),
            committed_at: event.committed_at.map(|at| at.to_string()),
            versions: event
                .versions
                .iter()
                .map(|version| StoredVersion {
                    vo_id: version.vo_id,
                    kind: version.kind.clone(),
                    sys_version: version.sys_version,
                    version_tree_id: version.version_tree_id.clone(),
                    change_type: version.change_type.clone(),
                    template_id: version.template_id.clone(),
                })
                .collect(),
            envelope: event.envelope.clone(),
        }
    }
}

impl StoredEvent {
    /// Returns the event this row stores.
    fn into_event(self) -> Result<Event, DeadLetterError> {
        let position = match self.position {
            StoredPosition::Outbox { domain, seq } => Position::Outbox {
                domain: match domain.as_str() {
                    "clinical" => Domain::Clinical,
                    "party" => Domain::Party,
                    _ => return Err(DeadLetterError::Value { what: "an outbox" }),
                },
                seq: Seq::new(seq),
            },
            StoredPosition::Committed { at, versions } => {
                let instant = at.parse().map_err(|_refused| DeadLetterError::Value {
                    what: "a commit instant",
                })?;
                Position::Committed(Committed::new(instant, at, versions))
            }
        };
        let contribution_id = self
            .contribution_id
            .as_deref()
            .map(ContributionUid::new)
            .transpose()
            .map_err(|_refused| DeadLetterError::Value {
                what: "a contribution id",
            })?;
        let ehr_id = self
            .ehr_id
            .as_deref()
            .map(EhrId::new)
            .transpose()
            .map_err(|_refused| DeadLetterError::Value { what: "an EHR id" })?;
        let committed_at = self
            .committed_at
            .as_deref()
            .map(str::parse::<jiff::Timestamp>)
            .transpose()
            .map_err(|_refused| DeadLetterError::Value {
                what: "a commit instant",
            })?;
        Ok(Event {
            position,
            erasure: self.erasure,
            contribution_id,
            ehr_id,
            committed_at,
            versions: self
                .versions
                .into_iter()
                .map(|version| ChangedVersion {
                    vo_id: version.vo_id,
                    kind: version.kind,
                    sys_version: version.sys_version,
                    version_tree_id: version.version_tree_id,
                    change_type: version.change_type,
                    template_id: version.template_id,
                })
                .collect(),
            envelope: self.envelope,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::DeadLetters;
    use crate::change_feed::Domain;
    use crate::change_feed::Event;
    use crate::change_feed::Position;
    use crate::change_feed::ReaderName;
    use crate::change_feed::Seq;

    fn event(seq: u64) -> Event {
        Event {
            position: Position::Outbox {
                domain: Domain::Clinical,
                seq: Seq::new(seq),
            },
            erasure: false,
            contribution_id: None,
            ehr_id: None,
            committed_at: None,
            versions: Vec::new(),
            envelope: serde_json::Map::new(),
        }
    }

    #[test]
    fn a_parked_event_survives_a_reopen_and_leaves_on_its_replay() {
        let dir = tempfile::tempdir().expect("a scratch directory");
        let path = dir.path().join("dead-letters.redb");
        let reader = ReaderName::new("a").expect("a reader name");
        let other = ReaderName::new("b").expect("a reader name");
        let id = {
            let store = DeadLetters::open(&path).expect("the store opens");
            assert_eq!(
                1,
                store.charge(&reader, &event(7).position).expect("a charge")
            );
            assert_eq!(
                2,
                store.charge(&reader, &event(7).position).expect("a charge")
            );
            store
                .park(&reader, &event(7), "the mapping refused it", 2)
                .expect("parked")
        };
        let store = DeadLetters::open(&path).expect("the store opens again");
        assert!(store.parked(&other).expect("a read").is_empty());
        let parked = store.parked(&reader).expect("a read");
        assert_eq!(1, parked.len());
        assert_eq!(event(7).position, parked[0].event.position);
        assert_eq!("the mapping refused it", parked[0].cause);
        assert!(!parked[0].replay);
        assert_eq!(
            1,
            store.charge(&reader, &event(7).position).expect("a charge"),
            "parking forgot the count"
        );
        assert!(store.request_replay(&reader, id).expect("a replay"));
        assert!(!store.request_replay(&other, id).expect("a replay"));
        store
            .repark(&reader, id, "still refused")
            .expect("parked again");
        let again = store.parked(&reader).expect("a read");
        assert_eq!(3, again[0].attempts);
        assert!(!again[0].replay);
        store.replayed(&reader, id).expect("replayed");
        assert!(store.parked(&reader).expect("a read").is_empty());
    }
}
