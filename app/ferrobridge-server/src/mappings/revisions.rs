// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The mapping revisions in the state store: every set of console mapping
//! files the bridge recorded, and the one every replica runs.
//!
//! A revision names each console file by its wire path and the SHA-256 of
//! its bytes, kept once per digest. A change holds the `mapping_running` row
//! for its whole transaction, so changes from every replica run one at a
//! time, and one that moves the running revision notifies [`CHANNEL`] before
//! it commits; each replica then compiles that revision
//! (`crate::operator::watch`). No specification governs the store: our own
//! design.

use std::collections::BTreeMap;
use std::sync::Arc;

use ferrobridge_operator_api::revisions::{Origin, Revision};
use sqlx::postgres::{PgPool, PgRow};
use sqlx::{PgConnection, Postgres, Row as _, Transaction};

use crate::mappings::files;

/// The channel a change of the running revision is notified on.
pub const CHANNEL: &str = "ferrobridge_mappings";

/// The console files of one revision, by wire path.
pub type Files = BTreeMap<String, Arc<[u8]>>;

/// The columns a revision is read with, as one literal for `concat!`.
macro_rules! columns {
    () => {
        "id, parent, origin, author, reason, \
         to_char(created_at AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"') AS time, \
         applied_at IS NOT NULL AS applied, source, files::text AS files"
    };
}

/// Why the revision store could not answer.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum RevisionError {
    /// The state store refused or failed a statement.
    #[error("a mapping revision statement failed")]
    Database(#[source] Box<sqlx::Error>),
    /// A stored revision is not the shape this version writes.
    #[error("the mapping revision {id} holds a record this version cannot read")]
    Record {
        /// The revision.
        id: i64,
    },
    /// A revision names a digest the store holds no bytes for, or bytes with
    /// another digest.
    #[error("the mapping blob {digest} is missing or does not hold the bytes of its digest")]
    Corrupt {
        /// The digest.
        digest: String,
    },
}

/// Returns the error a database failure is.
fn database(source: sqlx::Error) -> RevisionError {
    RevisionError::Database(Box::new(source))
}

/// The console files the bridge runs, and the revision they came from.
#[derive(Debug, Clone, Default)]
pub struct Console {
    /// The running revision, `None` before the first one.
    revision: Option<String>,
    /// Its files.
    files: Files,
}

impl Console {
    /// Returns the console files `files` of the revision `revision`.
    #[must_use]
    pub const fn new(revision: Option<String>, files: Files) -> Self {
        Self { revision, files }
    }

    /// Returns the revision the files came from.
    #[must_use]
    pub fn revision(&self) -> Option<&str> {
        self.revision.as_deref()
    }

    /// Returns the files, by wire path.
    #[must_use]
    pub const fn files(&self) -> &Files {
        &self.files
    }
}

/// Who records a revision, and why.
#[derive(Debug, Clone)]
pub struct Entry {
    /// What records it.
    pub origin: Origin,
    /// The subject of the operator.
    pub author: String,
    /// The reason given.
    pub reason: String,
    /// The revision an apply restored.
    pub source: Option<String>,
}

/// The mapping revisions of one state store.
#[derive(Debug, Clone)]
pub struct Revisions {
    /// The store's pool.
    pool: PgPool,
}

impl Revisions {
    /// Returns the revisions the state store `store` keeps.
    #[must_use]
    pub fn new(store: &crate::store::Store) -> Self {
        Self {
            pool: store.pool().clone(),
        }
    }

    /// Returns the running revision's id and every revision, oldest first.
    ///
    /// # Errors
    ///
    /// Returns [`RevisionError::Database`] when a statement fails and
    /// [`RevisionError::Record`] for a revision this version cannot read.
    pub async fn list(&self) -> Result<(Option<String>, Vec<Revision>), RevisionError> {
        let mut connection = self.pool.acquire().await.map_err(database)?;
        let running = running_id(&mut connection).await?;
        let rows = sqlx::query(concat!(
            "SELECT ",
            columns!(),
            " FROM mapping_revision ORDER BY id"
        ))
        .fetch_all(&mut *connection)
        .await
        .map_err(database)?;
        let revisions = rows.iter().map(revision).collect::<Result<_, _>>()?;
        Ok((running.map(|id| id.to_string()), revisions))
    }

    /// Returns the revision `id` names, or `None` when there is none.
    ///
    /// # Errors
    ///
    /// Returns what [`Self::list`] returns.
    pub async fn get(&self, id: &str) -> Result<Option<Revision>, RevisionError> {
        let mut connection = self.pool.acquire().await.map_err(database)?;
        get(&mut connection, id).await
    }

    /// Returns the running revision, `None` before the first one.
    ///
    /// # Errors
    ///
    /// Returns what [`Self::list`] returns.
    pub async fn running(&self) -> Result<Option<Revision>, RevisionError> {
        let mut connection = self.pool.acquire().await.map_err(database)?;
        match running_id(&mut connection).await? {
            Some(id) => get(&mut connection, &id.to_string()).await,
            None => Ok(None),
        }
    }

    /// Returns the bytes of every file of `revision`, by wire path.
    ///
    /// # Errors
    ///
    /// Returns [`RevisionError::Database`] when a statement fails and
    /// [`RevisionError::Corrupt`] for a file whose bytes are missing or have
    /// another digest.
    pub async fn contents(&self, revision: &Revision) -> Result<Files, RevisionError> {
        let mut connection = self.pool.acquire().await.map_err(database)?;
        contents(&mut connection, revision).await
    }

    /// Returns the running revision's console files.
    ///
    /// # Errors
    ///
    /// Returns what [`Self::running`] and [`Self::contents`] return.
    pub async fn console(&self) -> Result<Console, RevisionError> {
        let mut connection = self.pool.acquire().await.map_err(database)?;
        let Some(id) = running_id(&mut connection).await? else {
            return Ok(Console::default());
        };
        let Some(running) = get(&mut connection, &id.to_string()).await? else {
            return Ok(Console::default());
        };
        let files = contents(&mut connection, &running).await?;
        Ok(Console::new(Some(running.id), files))
    }

    /// Begins a change: a transaction holding the `mapping_running` row
    /// until it commits or is dropped.
    ///
    /// # Errors
    ///
    /// Returns what [`Self::list`] returns.
    pub async fn change(&self) -> Result<Change, RevisionError> {
        let mut transaction = self.pool.begin().await.map_err(database)?;
        let running: Option<i64> =
            sqlx::query_scalar("SELECT revision FROM mapping_running WHERE singleton FOR UPDATE")
                .fetch_one(&mut *transaction)
                .await
                .map_err(database)?;
        let running = match running {
            Some(id) => get(&mut transaction, &id.to_string()).await?,
            None => None,
        };
        Ok(Change {
            transaction,
            running,
        })
    }
}

/// Whether a recorded revision runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Recording {
    /// It becomes the running revision.
    Run,
    /// It waits for an apply.
    Keep,
}

/// One change of the revisions, holding the `mapping_running` row; dropped
/// uncommitted, it changes nothing.
#[derive(Debug)]
pub struct Change {
    /// The transaction.
    transaction: Transaction<'static, Postgres>,
    /// The revision running when the change began.
    running: Option<Revision>,
}

impl Change {
    /// Returns the revision running when the change began.
    #[must_use]
    pub const fn running(&self) -> Option<&Revision> {
        self.running.as_ref()
    }

    /// Returns the revision `id` names, or `None` when there is none.
    ///
    /// # Errors
    ///
    /// Returns what [`Revisions::list`] returns.
    pub async fn get(&mut self, id: &str) -> Result<Option<Revision>, RevisionError> {
        get(&mut self.transaction, id).await
    }

    /// Returns the bytes of every file of `revision`, by wire path.
    ///
    /// # Errors
    ///
    /// Returns what [`Revisions::contents`] returns.
    pub async fn contents(&mut self, revision: &Revision) -> Result<Files, RevisionError> {
        contents(&mut self.transaction, revision).await
    }

    /// Returns the running revision's files, none before the first one.
    ///
    /// # Errors
    ///
    /// Returns what [`Revisions::contents`] returns.
    pub async fn running_files(&mut self) -> Result<Files, RevisionError> {
        match self.running.clone() {
            Some(running) => self.contents(&running).await,
            None => Ok(Files::new()),
        }
    }

    /// Records `files` under `entry` and commits; under [`Recording::Run`]
    /// the revision runs and every replica is notified. Returns the
    /// revision.
    ///
    /// # Errors
    ///
    /// Returns [`RevisionError::Database`] when a statement or the commit
    /// fails, and nothing is recorded.
    pub async fn record(
        mut self,
        entry: Entry,
        files: &Files,
        recording: Recording,
    ) -> Result<Revision, RevisionError> {
        let applied = recording == Recording::Run;
        let mut digests = BTreeMap::new();
        for (wire, bytes) in files {
            let digest = files::sha256(bytes);
            sqlx::query(
                "INSERT INTO mapping_blob (sha256, bytes) VALUES (decode($1, 'hex'), $2) \
                 ON CONFLICT (sha256) DO NOTHING",
            )
            .bind(&digest)
            .bind(bytes.as_ref())
            .execute(&mut *self.transaction)
            .await
            .map_err(database)?;
            digests.insert(wire.clone(), digest);
        }
        let manifest = serde_json::Value::Object(
            digests
                .into_iter()
                .map(|(wire, digest)| (wire, serde_json::Value::String(digest)))
                .collect(),
        )
        .to_string();
        let parent = self
            .running
            .as_ref()
            .and_then(|running| number(&running.id));
        let source = entry.source.as_deref().and_then(number);
        let row = sqlx::query(concat!(
            "INSERT INTO mapping_revision (parent, origin, author, reason, applied_at, source, files) \
             VALUES ($1, $2, $3, $4, CASE WHEN $5 THEN now() END, $6, $7::jsonb) RETURNING ",
            columns!()
        ))
        .bind(parent)
        .bind(origin_text(entry.origin))
        .bind(&entry.author)
        .bind(&entry.reason)
        .bind(applied)
        .bind(source)
        .bind(&manifest)
        .fetch_one(&mut *self.transaction)
        .await
        .map_err(database)?;
        let recorded = revision(&row)?;
        if applied {
            let id = number(&recorded.id).unwrap_or_default();
            sqlx::query("UPDATE mapping_running SET revision = $1 WHERE singleton")
                .bind(id)
                .execute(&mut *self.transaction)
                .await
                .map_err(database)?;
            sqlx::query("SELECT pg_notify($1, $2)")
                .bind(CHANNEL)
                .bind(&recorded.id)
                .execute(&mut *self.transaction)
                .await
                .map_err(database)?;
        }
        self.transaction.commit().await.map_err(database)?;
        Ok(recorded)
    }
}

/// Returns the running revision's id, `None` before the first one.
async fn running_id(connection: &mut PgConnection) -> Result<Option<i64>, RevisionError> {
    sqlx::query_scalar("SELECT revision FROM mapping_running WHERE singleton")
        .fetch_one(connection)
        .await
        .map_err(database)
}

/// Returns the revision `id` names on `connection`.
async fn get(connection: &mut PgConnection, id: &str) -> Result<Option<Revision>, RevisionError> {
    let Some(id) = number(id) else {
        return Ok(None);
    };
    sqlx::query(concat!(
        "SELECT ",
        columns!(),
        " FROM mapping_revision WHERE id = $1"
    ))
    .bind(id)
    .fetch_optional(connection)
    .await
    .map_err(database)?
    .as_ref()
    .map(revision)
    .transpose()
}

/// Returns the bytes of every file of `revision` on `connection`, each
/// checked against its digest.
async fn contents(
    connection: &mut PgConnection,
    revision: &Revision,
) -> Result<Files, RevisionError> {
    let digests: Vec<&str> = revision.files.values().map(String::as_str).collect();
    let rows = sqlx::query(
        "SELECT encode(sha256, 'hex') AS digest, bytes FROM mapping_blob \
         WHERE sha256 = ANY (SELECT decode(d, 'hex') FROM unnest($1::text[]) AS d)",
    )
    .bind(&digests)
    .fetch_all(connection)
    .await
    .map_err(database)?;
    let mut blobs: BTreeMap<String, Arc<[u8]>> = BTreeMap::new();
    for row in &rows {
        let digest: String = row.try_get("digest").map_err(database)?;
        let bytes: Vec<u8> = row.try_get("bytes").map_err(database)?;
        if files::sha256(&bytes) != digest {
            return Err(RevisionError::Corrupt { digest });
        }
        blobs.insert(digest, Arc::from(bytes));
    }
    revision
        .files
        .iter()
        .map(|(wire, digest)| {
            blobs
                .get(digest)
                .map(|bytes| (wire.clone(), Arc::clone(bytes)))
                .ok_or_else(|| RevisionError::Corrupt {
                    digest: digest.clone(),
                })
        })
        .collect()
}

/// Returns the revision a `mapping_revision` row holds.
fn revision(row: &PgRow) -> Result<Revision, RevisionError> {
    let id: i64 = row.try_get("id").map_err(database)?;
    let unreadable = || RevisionError::Record { id };
    let origin = match row
        .try_get::<String, _>("origin")
        .map_err(database)?
        .as_str()
    {
        "save" => Origin::Save,
        "apply" => Origin::Apply,
        "import" => Origin::Import,
        _ => return Err(unreadable()),
    };
    let files: String = row.try_get("files").map_err(database)?;
    let files: BTreeMap<String, String> =
        serde_json::from_str(&files).map_err(|_unreadable| unreadable())?;
    Ok(Revision {
        id: id.to_string(),
        parent: row
            .try_get::<Option<i64>, _>("parent")
            .map_err(database)?
            .map(|parent| parent.to_string()),
        author: row.try_get("author").map_err(database)?,
        time: row.try_get("time").map_err(database)?,
        reason: row.try_get("reason").map_err(database)?,
        origin,
        applied: row.try_get("applied").map_err(database)?,
        source: row
            .try_get::<Option<i64>, _>("source")
            .map_err(database)?
            .map(|source| source.to_string()),
        files,
    })
}

/// Returns the text an origin is stored as.
const fn origin_text(origin: Origin) -> &'static str {
    match origin {
        Origin::Save => "save",
        Origin::Apply => "apply",
        Origin::Import => "import",
    }
}

/// Returns the number a revision id names, or `None` for any other text.
fn number(id: &str) -> Option<i64> {
    if id.is_empty() || !id.chars().all(|character| character.is_ascii_digit()) {
        return None;
    }
    id.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::number;

    #[test]
    fn only_a_decimal_number_names_a_revision() {
        assert_eq!(Some(12), number("12"));
        for id in ["", "-1", "1a", "+1", "99999999999999999999"] {
            assert_eq!(None, number(id), "{id}");
        }
    }
}
