// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! What the outbound lane has delivered, in `outbound_delivered` of the state
//! store: for each composition, the EHR and the `[type]/[id]` of every
//! resource its latest delivery carried.
//!
//! A deletion or an erasure takes the content away, so the resources to
//! retract are read from here, and an update retracts the resources its
//! earlier delivery carried and this one does not. What an accepted delivery
//! stands at is staged in memory until the follower commits its pass, and is
//! then written in the transaction that moves the lane's watermark, so a
//! crash between the endpoint's answer and the commit redelivers (the
//! `PUT` and `DELETE` entries are idempotent) and never forgets. A read sees
//! the staged state first. Identifiers only. No specification governs this:
//! our own design.

use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::sync::Mutex;
use std::sync::PoisonError;

use sqlx::PgConnection;
use sqlx::postgres::PgPool;

/// Why the record could not be read or written.
#[derive(Debug, thiserror::Error)]
#[error("the outbound lane's delivery record could not be read or written")]
pub struct DeliveredError(#[from] sqlx::Error);

/// What one composition stands at: its EHR and the references it was last
/// delivered as, none once it is retracted.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Standing {
    /// The EHR.
    ehr_id: String,
    /// The references.
    refs: BTreeSet<String>,
}

/// The delivery record, with what the current pass staged.
#[derive(Debug)]
pub struct Delivered {
    /// The state store's pool.
    pool: PgPool,
    /// Versioned object id to what its accepted delivery of this pass stands
    /// at.
    staged: Mutex<BTreeMap<String, Standing>>,
}

impl Delivered {
    /// Returns the record the state store behind `pool` keeps.
    #[must_use]
    pub fn new(pool: PgPool) -> Self {
        Self {
            pool,
            staged: Mutex::new(BTreeMap::new()),
        }
    }

    /// Returns the staged standing of `vo_id`, when this pass staged one.
    fn staged(&self, vo_id: &str) -> Option<Standing> {
        self.staged
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(vo_id)
            .cloned()
    }

    /// Returns the references the latest delivery of the composition `vo_id`
    /// carried.
    ///
    /// # Errors
    /// Returns [`DeliveredError`] when the read fails.
    pub async fn delivered(&self, vo_id: &str) -> Result<BTreeSet<String>, DeliveredError> {
        if let Some(standing) = self.staged(vo_id) {
            return Ok(standing.refs);
        }
        let refs: Option<Vec<String>> = sqlx::query_scalar(
            "SELECT refs FROM outbound_delivered WHERE versioned_object_id = $1",
        )
        .bind(vo_id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(refs.unwrap_or_default().into_iter().collect())
    }

    /// Returns the compositions delivered for the EHR `ehr_id`.
    ///
    /// # Errors
    /// Returns [`DeliveredError`] when the read fails.
    pub async fn compositions(&self, ehr_id: &str) -> Result<Vec<String>, DeliveredError> {
        let stored: Vec<String> = sqlx::query_scalar(
            "SELECT versioned_object_id FROM outbound_delivered WHERE ehr_id = $1 \
             ORDER BY versioned_object_id",
        )
        .bind(ehr_id)
        .fetch_all(&self.pool)
        .await?;
        let mut found: BTreeSet<String> = stored.into_iter().collect();
        for (vo_id, standing) in self
            .staged
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .iter()
        {
            if standing.refs.is_empty() {
                found.remove(vo_id);
            } else if standing.ehr_id == ehr_id {
                found.insert(vo_id.clone());
            }
        }
        Ok(found.into_iter().collect())
    }

    /// Stages `refs` as what the composition `vo_id` of the EHR `ehr_id`
    /// stands at once the endpoint accepted its delivery: none retracts it.
    pub fn stage(&self, ehr_id: &str, vo_id: &str, refs: BTreeSet<String>) {
        self.staged
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(
                vo_id.to_owned(),
                Standing {
                    ehr_id: ehr_id.to_owned(),
                    refs,
                },
            );
    }

    /// Forgets what this pass staged, which the follower did not commit.
    pub fn discard(&self) {
        self.staged
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clear();
    }

    /// Writes what this pass staged inside the follower's transaction on
    /// `connection`, and forgets it.
    ///
    /// # Errors
    /// Returns [`DeliveredError`] when a statement fails.
    pub async fn commit(&self, connection: &mut PgConnection) -> Result<(), DeliveredError> {
        let staged =
            std::mem::take(&mut *self.staged.lock().unwrap_or_else(PoisonError::into_inner));
        for (vo_id, standing) in staged {
            if standing.refs.is_empty() {
                sqlx::query("DELETE FROM outbound_delivered WHERE versioned_object_id = $1")
                    .bind(&vo_id)
                    .execute(&mut *connection)
                    .await?;
            } else {
                sqlx::query(
                    "INSERT INTO outbound_delivered (versioned_object_id, ehr_id, refs) \
                     VALUES ($1, $2, $3) ON CONFLICT (versioned_object_id) DO UPDATE SET \
                     ehr_id = excluded.ehr_id, refs = excluded.refs, delivered_at = now()",
                )
                .bind(&vo_id)
                .bind(&standing.ehr_id)
                .bind(standing.refs.into_iter().collect::<Vec<_>>())
                .execute(&mut *connection)
                .await?;
            }
        }
        Ok(())
    }
}
