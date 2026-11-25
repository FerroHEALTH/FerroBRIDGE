// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The watermark of each reader of the AQL source, in `reader_watermark` of
//! the state store.
//!
//! One row per reader holds the commit instant the reader has finished up
//! to, as the CDR wrote it, and the version uids finished at that instant. A
//! write runs in one transaction fenced by the reader's lane lease and
//! refuses to move a watermark backwards. No specification governs this: our
//! own design.

use std::collections::BTreeSet;

use sqlx::PgConnection;
use sqlx::postgres::PgPool;

use crate::change_feed::Committed;
use crate::change_feed::FeedError;
use crate::change_feed::Position;
use crate::change_feed::ReaderName;
use crate::store::lease::Term;

/// The watermarks of every reader.
#[derive(Debug, Clone)]
pub struct Watermarks {
    /// The state store's pool.
    pool: PgPool,
}

impl Watermarks {
    /// Returns the watermarks the state store behind `pool` keeps.
    #[must_use]
    pub const fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Returns the watermark of `reader`, or `None` before its first one.
    ///
    /// # Errors
    /// Returns [`FeedError::Store`] when the read fails and
    /// [`FeedError::Malformed`] when the stored instant does not read.
    pub async fn get(&self, reader: &ReaderName) -> Result<Option<Committed>, FeedError> {
        let row: Option<(String, Vec<String>)> =
            sqlx::query_as("SELECT committed_at, finished FROM reader_watermark WHERE reader = $1")
                .bind(reader.as_str())
                .fetch_optional(&self.pool)
                .await?;
        row.map(|(at, finished)| decode(reader, at, finished))
            .transpose()
    }

    /// Records `start` as the watermark of `reader` unless it has one, under
    /// `term`; returns the watermark that stands.
    ///
    /// # Errors
    /// Returns [`FeedError::Lease`] when `term` is no longer the lane's and
    /// [`FeedError::Store`] when the write does not commit.
    pub async fn begin(
        &self,
        reader: &ReaderName,
        start: &Committed,
        term: &Term,
    ) -> Result<Committed, FeedError> {
        let mut tx = self.pool.begin().await?;
        term.fence(&mut tx).await?;
        sqlx::query(
            "INSERT INTO reader_watermark (reader, committed_at, finished, generation) \
             VALUES ($1, $2, $3, $4) ON CONFLICT (reader) DO NOTHING",
        )
        .bind(reader.as_str())
        .bind(start.text())
        .bind(start.versions().iter().cloned().collect::<Vec<_>>())
        .bind(term.generation().get())
        .execute(&mut *tx)
        .await?;
        let stood = read(&mut tx, reader)
            .await?
            .ok_or_else(|| FeedError::Malformed {
                reason: format!("the watermark of reader {reader} was not recorded"),
                source: None,
            })?;
        tx.commit().await?;
        Ok(stood)
    }

    /// Moves the watermark of `reader` to `watermark` inside the caller's
    /// transaction on `connection`, which `term` has fenced.
    ///
    /// # Errors
    /// Returns [`FeedError::Behind`] when `watermark` does not lie after the
    /// stored one and [`FeedError::Store`] when a statement fails.
    pub async fn set(
        connection: &mut PgConnection,
        reader: &ReaderName,
        watermark: &Committed,
        term: &Term,
    ) -> Result<(), FeedError> {
        if let Some(stored) = read(connection, reader).await?
            && stored != *watermark
            && !watermark.follows(&stored)
        {
            return Err(FeedError::Behind {
                reader: reader.clone(),
                position: Position::Committed(watermark.clone()),
            });
        }
        sqlx::query(
            "INSERT INTO reader_watermark (reader, committed_at, finished, generation) \
             VALUES ($1, $2, $3, $4) ON CONFLICT (reader) DO UPDATE SET \
             committed_at = excluded.committed_at, finished = excluded.finished, \
             generation = excluded.generation",
        )
        .bind(reader.as_str())
        .bind(watermark.text())
        .bind(watermark.versions().iter().cloned().collect::<Vec<_>>())
        .bind(term.generation().get())
        .execute(connection)
        .await?;
        Ok(())
    }
}

/// Returns the watermark of `reader` inside the caller's transaction, its
/// row locked until the transaction ends.
async fn read(
    connection: &mut PgConnection,
    reader: &ReaderName,
) -> Result<Option<Committed>, FeedError> {
    let row: Option<(String, Vec<String>)> = sqlx::query_as(
        "SELECT committed_at, finished FROM reader_watermark WHERE reader = $1 FOR UPDATE",
    )
    .bind(reader.as_str())
    .fetch_optional(connection)
    .await?;
    row.map(|(at, finished)| decode(reader, at, finished))
        .transpose()
}

/// Returns the watermark the row of `reader` holds.
fn decode(reader: &ReaderName, at: String, finished: Vec<String>) -> Result<Committed, FeedError> {
    let instant = at
        .parse::<jiff::Timestamp>()
        .map_err(|_refused| FeedError::Malformed {
            reason: format!("the stored watermark of reader {reader} names no instant"),
            source: None,
        })?;
    Ok(Committed::new(
        instant,
        at,
        finished.into_iter().collect::<BTreeSet<_>>(),
    ))
}
