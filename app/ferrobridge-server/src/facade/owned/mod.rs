// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The resources the facade keeps itself, because the CDR has no home for
//! them: SMART App State (`Basic`) and SMART launch `Task`s.
//!
//! The state store holds each resource under its type and id with its
//! version, as the JSON the client sent plus the `id` and `meta` the server
//! writes, so every replica serves the same resources. A deleted resource
//! leaves a tombstone, so a later request about its id is told it is gone. A
//! conditional write holds the row (`SELECT … FOR UPDATE`) from its read to
//! its write, so two writers racing on one version cannot both win. No
//! specification governs the store: our own design.

pub mod answer;

use fhir_types::codec::Object;
use sqlx::Row as _;
use sqlx::postgres::PgPool;

/// Why the store could not answer.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum OwnedError {
    /// The state store refused or failed a statement.
    #[error("a resource store statement failed")]
    Database {
        /// What the database reported.
        #[source]
        source: Box<sqlx::Error>,
    },
    /// A stored record is not the shape this version writes, or a resource
    /// could not be written as one.
    #[error("the resource store holds a record under {key} that this version cannot read")]
    Record {
        /// The key the record sits under.
        key: String,
        /// What the codec reported.
        #[source]
        source: Box<serde_json::Error>,
    },
}

impl From<sqlx::Error> for OwnedError {
    fn from(source: sqlx::Error) -> Self {
        Self::Database {
            source: Box::new(source),
        }
    }
}

/// One record as it is stored.
#[derive(Debug, Clone)]
struct Record {
    /// The version, from 1.
    version: u64,
    /// The resource, `None` once deleted.
    resource: Option<Object>,
}

/// What the store holds under one id.
#[derive(Debug, Clone, PartialEq)]
pub enum Held {
    /// The current version of a live resource.
    Live {
        /// Its version.
        version: u64,
        /// The resource, its `id` and `meta.versionId` written.
        resource: Object,
    },
    /// A resource that was deleted at `version`.
    Deleted {
        /// The version of the deletion.
        version: u64,
    },
}

/// What a conditional write does once its precondition holds.
#[derive(Debug, Clone, PartialEq)]
pub enum Change {
    /// Store `resource` as the next version.
    Put(Object),
    /// Delete the resource.
    Delete,
}

/// The resources the facade keeps, in the state store.
#[derive(Debug, Clone)]
pub struct OwnedStore {
    /// The pool.
    pool: PgPool,
}

impl OwnedStore {
    /// Returns the resources of `store`.
    #[must_use]
    pub fn open(store: &crate::store::Store) -> Self {
        Self {
            pool: store.pool().clone(),
        }
    }

    /// Stores `resource` as version 1 of a new `resource_type` under a fresh
    /// id, and returns it with its `id` and `meta` written.
    ///
    /// # Errors
    ///
    /// Returns [`OwnedError`] when the write does not commit.
    pub async fn create(
        &self,
        resource_type: &str,
        resource: Object,
    ) -> Result<Object, OwnedError> {
        let id = uuid::Uuid::new_v4().to_string();
        let key = key_of(resource_type, &id);
        let stamped = stamp(resource, &id, 1);
        let body = encode(&key, &stamped)?;
        sqlx::query(
            "INSERT INTO owned_resource (resource_type, id, version, body) VALUES ($1, $2, 1, $3)",
        )
        .bind(resource_type)
        .bind(&id)
        .bind(&body)
        .execute(&self.pool)
        .await?;
        Ok(stamped)
    }

    /// Returns what the store holds for `resource_type`/`id`.
    ///
    /// # Errors
    ///
    /// Returns [`OwnedError`] when the read fails or the record cannot be
    /// read.
    pub async fn get(&self, resource_type: &str, id: &str) -> Result<Option<Held>, OwnedError> {
        let row = sqlx::query(
            "SELECT version, body FROM owned_resource WHERE resource_type = $1 AND id = $2 \
             AND version > 0",
        )
        .bind(resource_type)
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;
        row.map(|row| held_of(&key_of(resource_type, id), &row))
            .transpose()
    }

    /// Applies the change `decide` returns for what the store holds under
    /// `resource_type`/`id`, in one transaction that holds the row, and
    /// returns what the store then holds; a refusal of `decide` writes
    /// nothing.
    ///
    /// # Errors
    ///
    /// Returns [`OwnedError`] when the transaction fails; the inner
    /// `Err` is `decide`'s refusal.
    pub async fn change<R>(
        &self,
        resource_type: &str,
        id: &str,
        decide: impl FnOnce(Option<&Held>) -> Result<Change, R> + Send,
    ) -> Result<Result<Held, R>, OwnedError> {
        let key = key_of(resource_type, id);
        let mut tx = self.pool.begin().await?;
        // NOTE: no specification governs this: our own design; a version 0 row
        // stands for an absent resource, so a create by id has a row to hold.
        sqlx::query(
            "INSERT INTO owned_resource (resource_type, id, version) VALUES ($1, $2, 0) \
             ON CONFLICT (resource_type, id) DO NOTHING",
        )
        .bind(resource_type)
        .bind(id)
        .execute(&mut *tx)
        .await?;
        let row = sqlx::query(
            "SELECT version, body FROM owned_resource WHERE resource_type = $1 AND id = $2 \
             FOR UPDATE",
        )
        .bind(resource_type)
        .bind(id)
        .fetch_one(&mut *tx)
        .await?;
        let version: i64 = row.try_get("version")?;
        let held = if version == 0 {
            None
        } else {
            Some(held_of(&key, &row)?)
        };
        let change = match decide(held.as_ref()) {
            Err(refusal) => {
                tx.rollback().await?;
                return Ok(Err(refusal));
            }
            Ok(change) => change,
        };
        let next = held
            .as_ref()
            .map_or(0, Held::version)
            .checked_add(1)
            .ok_or_else(|| exhausted(&key))?;
        let stored = i64::try_from(next).map_err(|_overflow| exhausted(&key))?;
        let record = match change {
            Change::Put(resource) => Record {
                version: next,
                resource: Some(stamp(resource, id, next)),
            },
            Change::Delete => Record {
                version: next,
                resource: None,
            },
        };
        let body = record
            .resource
            .as_ref()
            .map(|resource| encode(&key, resource))
            .transpose()?;
        sqlx::query(
            "UPDATE owned_resource SET version = $3, body = $4 WHERE resource_type = $1 AND id = $2",
        )
        .bind(resource_type)
        .bind(id)
        .bind(stored)
        .bind(&body)
        .execute(&mut *tx)
        .await
        ?;
        tx.commit().await?;
        Ok(Ok(Held::from(record)))
    }

    /// Returns every live resource of `resource_type`, in id order.
    ///
    /// # Errors
    ///
    /// Returns [`OwnedError`] when the read fails or a record cannot be read.
    pub async fn scan(&self, resource_type: &str) -> Result<Vec<Object>, OwnedError> {
        let rows = sqlx::query(
            "SELECT id, body FROM owned_resource WHERE resource_type = $1 AND body IS NOT NULL \
             ORDER BY id COLLATE \"C\"",
        )
        .bind(resource_type)
        .fetch_all(&self.pool)
        .await?;
        rows.iter()
            .map(|row| {
                let id: String = row.try_get("id")?;
                let body: serde_json::Value = row.try_get("body")?;
                decode(&key_of(resource_type, &id), body)
            })
            .collect()
    }
}

/// Returns what a row holds under `key`.
fn held_of(key: &str, row: &sqlx::postgres::PgRow) -> Result<Held, OwnedError> {
    let version: i64 = row.try_get("version")?;
    let version = u64::try_from(version).map_err(|_negative| exhausted(key))?;
    let body: Option<serde_json::Value> = row.try_get("body")?;
    Ok(Held::from(Record {
        version,
        resource: body.map(|body| decode(key, body)).transpose()?,
    }))
}

impl Held {
    /// Returns the version.
    #[must_use]
    pub const fn version(&self) -> u64 {
        match self {
            Self::Live { version, .. } | Self::Deleted { version } => *version,
        }
    }
}

impl From<Record> for Held {
    fn from(record: Record) -> Self {
        match record.resource {
            Some(resource) => Self::Live {
                version: record.version,
                resource,
            },
            None => Self::Deleted {
                version: record.version,
            },
        }
    }
}

/// Returns the key `resource_type`/`id` is stored under.
fn key_of(resource_type: &str, id: &str) -> String {
    format!("{resource_type}/{id}")
}

/// Returns `resource` with `id`, `meta.versionId` and `meta.lastUpdated`
/// written, the rest of `meta` kept.
fn stamp(mut resource: Object, id: &str, version: u64) -> Object {
    crate::facade::identity::stamp(
        &mut resource,
        id,
        &version.to_string(),
        Some(&jiff::Timestamp::now().to_string()),
    );
    resource
}

/// Returns `resource` as the JSON it is stored as.
fn encode(key: &str, resource: &Object) -> Result<serde_json::Value, OwnedError> {
    serde_json::to_value(resource).map_err(|source| OwnedError::Record {
        key: key.to_owned(),
        source: Box::new(source),
    })
}

/// Returns the resource `body` stores under `key`.
fn decode(key: &str, body: serde_json::Value) -> Result<Object, OwnedError> {
    serde_json::from_value(body).map_err(|source| OwnedError::Record {
        key: key.to_owned(),
        source: Box::new(source),
    })
}

/// Returns the error of a version counter that cannot move on.
fn exhausted(key: &str) -> OwnedError {
    OwnedError::Record {
        key: key.to_owned(),
        source: Box::new(serde::de::Error::custom(
            "the version counter is at its largest value",
        )),
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::{Change, OwnedError, OwnedStore};

    /// A store over a database no server answers.
    fn unreachable() -> OwnedStore {
        OwnedStore {
            pool: sqlx::postgres::PgPoolOptions::new()
                .acquire_timeout(Duration::from_millis(300))
                .connect_lazy("postgres://bridge@127.0.0.1:1/state?sslmode=disable")
                .expect("a lazy pool"),
        }
    }

    #[tokio::test]
    async fn an_unreachable_store_refuses_every_call() {
        let store = unreachable();
        let basic = serde_json::from_str(r#"{"resourceType":"Basic"}"#).expect("JSON");
        assert!(matches!(
            store.create("Basic", basic).await,
            Err(OwnedError::Database { .. })
        ));
        assert!(matches!(
            store.get("Basic", "x").await,
            Err(OwnedError::Database { .. })
        ));
        assert!(matches!(
            store
                .change("Basic", "x", |_held| Ok::<_, ()>(Change::Delete))
                .await,
            Err(OwnedError::Database { .. })
        ));
        assert!(matches!(
            store.scan("Basic").await,
            Err(OwnedError::Database { .. })
        ));
    }
}
