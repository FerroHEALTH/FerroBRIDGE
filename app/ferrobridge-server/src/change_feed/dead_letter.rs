// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The dead letters of the change feed, in `dead_letter` and
//! `poisoned_attempt` of the state store.
//!
//! A consumer that fails an event with a poisoned failure on its bound of
//! passes parks it with its cause; below the bound each pass is counted. A
//! parked event is never dropped: it stays open until an operator settles it,
//! by a retry, which the follower offers to its consumer again before its
//! next page, or by a discard, and the row keeps who decided and why. Every
//! write a follower makes is fenced by its lane lease. The rows hold an
//! event's identifiers and provenance as the feed served them, never
//! clinical content. No specification governs this: our own design.

use std::collections::BTreeSet;

use crate::retry::{Decided, Decision};
use serde::Deserialize;
use serde::Serialize;
use sqlx::postgres::PgPool;

use crate::cdr::ids::ContributionUid;
use crate::cdr::ids::EhrId;
use crate::change_feed::ChangedVersion;
use crate::change_feed::Committed;
use crate::change_feed::Domain;
use crate::change_feed::Event;
use crate::change_feed::Position;
use crate::change_feed::ReaderName;
use crate::change_feed::Seq;
use crate::store::lease::LeaseError;
use crate::store::lease::Term;

/// Why the dead-letter store refused a call.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum DeadLetterError {
    /// The store refused a read or a write.
    #[error("the dead-letter store refused the call")]
    Store(#[from] sqlx::Error),
    /// The follower's lease is no longer the lane's.
    #[error("the follower no longer leads its lane")]
    Lease(#[from] LeaseError),
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
}

/// Returns the row error `source` is.
const fn row(source: serde_json::Error) -> DeadLetterError {
    DeadLetterError::Row { source }
}

/// One parked event.
#[derive(Debug, Clone)]
pub struct Parked {
    /// The number the store assigned it.
    pub id: u64,
    /// The event as the feed served it.
    pub event: Event,
    /// The last failure and its causes, in one line with no event body.
    pub cause: String,
    /// How many poisoned passes it failed, replays included.
    pub attempts: u32,
    /// When it was parked, or parked again after a replay failed.
    pub at: jiff::Timestamp,
    /// Whether a retry is decided and has not run yet.
    pub replay: bool,
    /// The last settlement an operator decided, when there is one.
    pub decided: Option<Decided>,
}

/// The parked events and the poisoned-pass counts of every reader.
#[derive(Debug, Clone)]
pub struct DeadLetters {
    /// The state store's pool.
    pool: PgPool,
}

impl DeadLetters {
    /// Returns the dead letters the state store behind `pool` keeps.
    #[must_use]
    pub const fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Counts one more poisoned pass of the event at `position` for
    /// `reader` under `term`, and returns the count.
    ///
    /// # Errors
    /// Returns [`DeadLetterError`] when the write does not commit or `term`
    /// is no longer the lane's.
    pub async fn charge(
        &self,
        term: &Term,
        reader: &ReaderName,
        position: &Position,
    ) -> Result<u32, DeadLetterError> {
        let key = key(position)?;
        let mut tx = self.pool.begin().await?;
        term.fence(&mut tx).await?;
        let count: i32 = sqlx::query_scalar(
            "INSERT INTO poisoned_attempt (reader, position, attempts) VALUES ($1, $2, 1) \
             ON CONFLICT (reader, position) DO UPDATE SET attempts = poisoned_attempt.attempts + 1 \
             RETURNING attempts",
        )
        .bind(reader.as_str())
        .bind(&key)
        .fetch_one(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(u32::try_from(count).unwrap_or(u32::MAX))
    }

    /// Forgets the poisoned passes of the event at `position` for `reader`
    /// under `term`, once it finished.
    ///
    /// # Errors
    /// Returns [`DeadLetterError`] when the write does not commit or `term`
    /// is no longer the lane's.
    pub async fn forget(
        &self,
        term: &Term,
        reader: &ReaderName,
        position: &Position,
    ) -> Result<(), DeadLetterError> {
        let key = key(position)?;
        let mut tx = self.pool.begin().await?;
        term.fence(&mut tx).await?;
        uncount(&mut tx, reader, &key).await?;
        tx.commit().await?;
        Ok(())
    }

    /// Parks `event` for `reader` after `attempts` poisoned passes, the last
    /// for `cause`, and forgets its count, in one transaction under `term`;
    /// returns the number it is parked under.
    ///
    /// # Errors
    /// Returns [`DeadLetterError`] when the write does not commit, `term` is
    /// no longer the lane's, or the event cannot be written.
    pub async fn park(
        &self,
        term: &Term,
        reader: &ReaderName,
        event: &Event,
        cause: &str,
        attempts: u32,
    ) -> Result<u64, DeadLetterError> {
        let key = key(&event.position)?;
        let text = serde_json::to_string(&StoredEvent::from(event)).map_err(row)?;
        let mut tx = self.pool.begin().await?;
        term.fence(&mut tx).await?;
        // NOTE: no specification governs this: our own design; an event parked
        // again after a crash before its acknowledgement keeps its first row.
        let id: i64 = sqlx::query_scalar(
            "INSERT INTO dead_letter (reader, position, event, reason, attempts) \
             VALUES ($1, $2, $3::jsonb, $4, $5) \
             ON CONFLICT (reader, position) DO UPDATE SET reason = excluded.reason, \
             attempts = excluded.attempts, parked_at = now() RETURNING id",
        )
        .bind(reader.as_str())
        .bind(&key)
        .bind(&text)
        .bind(cause)
        .bind(i32::try_from(attempts).unwrap_or(i32::MAX))
        .fetch_one(&mut *tx)
        .await?;
        uncount(&mut tx, reader, &key).await?;
        tx.commit().await?;
        Ok(id.cast_unsigned())
    }

    /// Returns the open events parked for `reader`, in the order they were
    /// first parked.
    ///
    /// # Errors
    /// Returns [`DeadLetterError`] when the read fails or a row does not
    /// read.
    pub async fn parked(&self, reader: &ReaderName) -> Result<Vec<Parked>, DeadLetterError> {
        let rows: Vec<Row> = sqlx::query_as(
            "SELECT id, event::text, reason, attempts, \
             to_char(parked_at AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS.US\"Z\"'), \
             replay_requested_at IS NOT NULL, decision, decided_by, decision_reason \
             FROM dead_letter WHERE reader = $1 AND settled_at IS NULL ORDER BY id",
        )
        .bind(reader.as_str())
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter().map(decode).collect()
    }

    /// Settles the open event `id` parked for `reader` as `decided` says;
    /// returns whether an open event has the number.
    ///
    /// # Errors
    /// Returns [`DeadLetterError::Store`] when the write does not commit.
    pub async fn decide(
        &self,
        reader: &ReaderName,
        id: u64,
        decided: &Decided,
    ) -> Result<bool, DeadLetterError> {
        let Ok(id) = i64::try_from(id) else {
            return Ok(false);
        };
        let done = sqlx::query(
            "UPDATE dead_letter SET decision = $3, decided_by = $4, decision_reason = $5, \
             replay_requested_at = CASE WHEN $3 = 'retry' THEN now() END, \
             settled_at = CASE WHEN $3 = 'discard' THEN now() END \
             WHERE reader = $1 AND id = $2 AND settled_at IS NULL",
        )
        .bind(reader.as_str())
        .bind(id)
        .bind(decided.decision.as_str())
        .bind(&decided.by)
        .bind(&decided.reason)
        .execute(&self.pool)
        .await?;
        Ok(done.rows_affected() == 1)
    }

    /// Settles the events `ids` parked for `reader`, whose retry finished,
    /// inside the follower's transaction on `connection`.
    ///
    /// # Errors
    /// Returns [`DeadLetterError::Store`] when the statement fails.
    pub async fn replayed(
        &self,
        connection: &mut sqlx::PgConnection,
        reader: &ReaderName,
        ids: &[u64],
    ) -> Result<(), DeadLetterError> {
        let ids: Vec<i64> = ids.iter().map(|id| id.cast_signed()).collect();
        sqlx::query(
            "UPDATE dead_letter SET settled_at = now(), replay_requested_at = NULL \
             WHERE reader = $1 AND id = ANY($2)",
        )
        .bind(reader.as_str())
        .bind(ids)
        .execute(connection)
        .await?;
        Ok(())
    }

    /// Parks the event `id` of `reader` again after its retry failed for
    /// `cause`, under `term`: one more attempt, and no retry pending.
    ///
    /// # Errors
    /// Returns [`DeadLetterError`] when the write does not commit or `term`
    /// is no longer the lane's.
    pub async fn repark(
        &self,
        term: &Term,
        reader: &ReaderName,
        id: u64,
        cause: &str,
    ) -> Result<(), DeadLetterError> {
        let mut tx = self.pool.begin().await?;
        term.fence(&mut tx).await?;
        sqlx::query(
            "UPDATE dead_letter SET attempts = attempts + 1, reason = $3, parked_at = now(), \
             replay_requested_at = NULL WHERE reader = $1 AND id = $2",
        )
        .bind(reader.as_str())
        .bind(id.cast_signed())
        .bind(cause)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(())
    }
}

/// Forgets the poisoned passes counted for `reader` at the position `key`,
/// inside the caller's transaction.
async fn uncount(
    connection: &mut sqlx::PgConnection,
    reader: &ReaderName,
    key: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query("DELETE FROM poisoned_attempt WHERE reader = $1 AND position = $2")
        .bind(reader.as_str())
        .bind(key)
        .execute(connection)
        .await?;
    Ok(())
}

/// One open row, as [`DeadLetters::parked`] reads it.
type Row = (
    i64,
    String,
    String,
    i32,
    String,
    bool,
    Option<String>,
    Option<String>,
    Option<String>,
);

/// Returns the key an event's position is counted under.
fn key(position: &Position) -> Result<String, DeadLetterError> {
    serde_json::to_string(&StoredPosition::from(position)).map_err(row)
}

/// Returns the parked event `row` holds.
fn decode(row_read: Row) -> Result<Parked, DeadLetterError> {
    let (id, event, reason, attempts, parked_at, replay, decision, by, why) = row_read;
    let stored: StoredEvent = serde_json::from_str(&event).map_err(row)?;
    let at = parked_at
        .parse()
        .map_err(|_refused| DeadLetterError::Value {
            what: "a parking instant",
        })?;
    let decided = match (decision, by, why) {
        (Some(decision), Some(by), Some(reason)) => Some(Decided {
            decision: Decision::parse(&decision)
                .ok_or(DeadLetterError::Value { what: "a decision" })?,
            by,
            reason,
        }),
        _ => None,
    };
    Ok(Parked {
        id: id.cast_unsigned(),
        event: stored.into_event()?,
        cause: reason,
        attempts: u32::try_from(attempts).unwrap_or(0),
        at,
        replay,
        decided,
    })
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
