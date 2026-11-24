// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The links, their sealed files and their file locations, in the state
//! store every replica shares.
//!
//! The store holds a link's rules, the passcode's Argon2id hash and each
//! file as its compact JWE; never a key and never a plaintext file. A
//! manifest id, a handle and a location token are stored as their SHA-256,
//! and the handle is also sealed so its owner can list it, so none is stored
//! in clear. A link leaves the store whole when it expires, is revoked, is
//! locked by its passcode attempts, or has given its last answer and its
//! last location has been used or has lapsed. Every change of one link runs
//! in one transaction that holds the link's row (`SELECT … FOR UPDATE`), so
//! replicas count its answers and passcode attempts exactly and serve a
//! location once (SMART Health Cards and Links 1.0.0, `links-specification`).
//! The sweep runs under a transaction-level advisory lock, on one replica at
//! a time. No specification governs the storage: our own design.

use serde::Deserialize;
use serde::Serialize;
use sha2::{Digest as _, Sha256};
use sqlx::postgres::{PgPool, PgRow};
use sqlx::{PgConnection, Row as _};

use super::seal;
use crate::store::seal::{KeyId, SealKeys};

/// The advisory lock the sweep holds, so one replica sweeps at a time.
const SWEEP_LOCK: i64 = 0x6662_7368_6c73; // "fbshls"

/// Why the store could not answer.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum StoreError {
    /// The links run without `[state.seal]`, so a handle cannot be sealed.
    #[error("the links need [state.seal]: they seal each handle")]
    Unsealed,
    /// The state store refused or failed a statement.
    #[error("a link store statement failed")]
    Database(#[source] Box<sqlx::Error>),
    /// A stored record is not the shape this version writes.
    #[error("the link store holds a record this version cannot read")]
    Record(#[source] serde_json::Error),
    /// A fresh token could not be drawn.
    #[error("a file location could not be drawn")]
    Seal(#[source] seal::SealError),
    /// A handle could not be sealed or opened.
    #[error("a handle could not be sealed or opened")]
    Handle(#[source] crate::store::seal::SealError),
    /// A link names a file the store does not hold.
    #[error("a link names a file the store does not hold")]
    Missing,
    /// A passcode could not be checked against its hash.
    #[error("a passcode could not be checked")]
    Passcode(#[source] super::passcode::PasscodeError),
    /// The passcode check did not finish.
    #[error("the passcode check did not finish")]
    Joined(#[source] tokio::task::JoinError),
}

/// Returns the error a database failure is.
fn database(source: sqlx::Error) -> StoreError {
    StoreError::Database(Box::new(source))
}

/// Returns the SHA-256 a manifest id, a handle or a token is stored as.
fn hashed(text: &str) -> Vec<u8> {
    Sha256::digest(text.as_bytes()).to_vec()
}

/// Returns a count as the store holds it.
fn position_of(position: u32) -> i32 {
    i32::try_from(position).unwrap_or(i32::MAX)
}

/// How a link is resolved by its receiver.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Resolution {
    /// A manifest request with the passcode (`P`).
    Passcode,
    /// A direct file request (`U`).
    Direct,
}

/// One file of a link, as its manifest describes it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileEntry {
    /// The manifest's `contentType`.
    pub content_type: String,
    /// The manifest's `lastUpdated`, an RFC 3339 instant.
    pub last_updated: String,
}

/// A link as the store holds it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Link {
    /// The handle the person manages it by, which names it in the audit
    /// trail.
    pub handle: String,
    /// The person who shares it: the `patient` of their token.
    pub owner: String,
    /// When it was created, in epoch seconds.
    pub created: i64,
    /// When it expires, in epoch seconds.
    pub expires: i64,
    /// Its label.
    pub label: Option<String>,
    /// How it is resolved.
    pub resolution: Resolution,
    /// The Argon2id hash of its passcode, under [`Resolution::Passcode`].
    pub passcode: Option<String>,
    /// The wrong passcodes it still takes before it is disabled.
    pub attempts_left: u32,
    /// The answers it still gives.
    pub answers_left: u32,
    /// Its files, in order.
    pub files: Vec<FileEntry>,
    /// The file locations handed out and neither used nor lapsed.
    pub outstanding: u32,
}

impl Link {
    /// Returns whether the link is past its expiry at `now`.
    #[must_use]
    pub const fn expired(&self, now: i64) -> bool {
        now >= self.expires
    }
}

/// One file of a manifest answer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Served {
    /// The file's entry.
    pub entry: FileEntry,
    /// The JWE, when it is embedded.
    pub embedded: Option<String>,
    /// The location token, when the file is fetched by location.
    pub location: Option<String>,
}

/// Why a link left the store.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ending {
    /// It reached its expiry.
    Expired,
    /// Its owner revoked it.
    Revoked,
    /// Its passcode attempts ran out.
    Locked,
    /// It gave its last answer and its last file was fetched or lapsed.
    Used,
}

/// A link that left the store, for its record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Gone {
    /// The handle.
    pub handle: String,
    /// The owner.
    pub owner: String,
    /// Why it left.
    pub ending: Ending,
}

/// The answer to a manifest request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Answer {
    /// No active link has the id, or it resolves another way.
    Inactive {
        /// The link that left the store on this request, when one did.
        gone: Option<Gone>,
    },
    /// The passcode is missing or wrong.
    Refused {
        /// The handle.
        handle: String,
        /// The owner.
        owner: String,
        /// The wrong passcodes the link still takes.
        remaining: u32,
        /// Whether the link was disabled by this attempt.
        locked: bool,
    },
    /// The manifest's files.
    Manifest {
        /// The handle.
        handle: String,
        /// The owner.
        owner: String,
        /// The files.
        files: Vec<Served>,
        /// Whether the link left the store with this answer.
        used: bool,
    },
}

/// A file fetched by its location or by a direct request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Fetched {
    /// No live location or active link has the token or id.
    Inactive {
        /// The link that left the store on this request, when one did.
        gone: Option<Gone>,
    },
    /// The file's JWE.
    File {
        /// The handle.
        handle: String,
        /// The owner.
        owner: String,
        /// The JWE.
        jwe: String,
        /// Whether the link left the store with this fetch.
        used: bool,
    },
}

/// A link as its owner lists it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Listed {
    /// The handle.
    pub handle: String,
    /// The label.
    pub label: Option<String>,
    /// When it was created, in epoch seconds.
    pub created: i64,
    /// When it expires, in epoch seconds.
    pub expires: i64,
    /// Whether it needs a passcode.
    pub passcode: bool,
    /// The answers it still gives.
    pub answers_left: u32,
}

/// What a sweep did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Swept {
    /// Another replica holds the sweep, so this one did nothing.
    Busy,
    /// The links that left the store.
    Done(Vec<Gone>),
}

/// The links of this deployment, in the state store.
#[derive(Debug, Clone)]
pub struct LinkStore {
    /// The pool.
    pool: PgPool,
    /// The keys a handle is sealed under.
    keys: SealKeys,
}

impl LinkStore {
    /// Returns the links of `store`.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError::Unsealed`] when `[state.seal]` is absent.
    pub fn open(store: &crate::store::Store) -> Result<Self, StoreError> {
        Ok(Self {
            pool: store.pool().clone(),
            keys: store.seal().cloned().ok_or(StoreError::Unsealed)?,
        })
    }

    /// Stores `link` under the manifest id `id` with its sealed `files`.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError`] when the write does not commit.
    pub async fn create(&self, id: &str, link: &Link, files: &[String]) -> Result<(), StoreError> {
        let id_hash = hashed(id);
        let (key_id, sealed) = self
            .keys
            .seal(&handle_context(&id_hash), link.handle.as_bytes())
            .map_err(StoreError::Handle)?;
        let mut tx = self.pool.begin().await.map_err(database)?;
        sqlx::query(
            "INSERT INTO shl_link (id_hash, handle_hash, handle_key_id, handle_sealed, owner, \
             expires, record) VALUES ($1, $2, $3, $4, $5, $6, $7)",
        )
        .bind(&id_hash)
        .bind(hashed(&link.handle))
        .bind(key_id.as_str())
        .bind(&sealed)
        .bind(&link.owner)
        .bind(link.expires)
        .bind(record_of(link)?)
        .execute(&mut *tx)
        .await
        .map_err(database)?;
        for (position, jwe) in (0_u32..).zip(files) {
            sqlx::query("INSERT INTO shl_file (id_hash, position, jwe) VALUES ($1, $2, $3)")
                .bind(&id_hash)
                .bind(position_of(position))
                .bind(jwe)
                .execute(&mut *tx)
                .await
                .map_err(database)?;
        }
        tx.commit().await.map_err(database)
    }

    /// Returns the active links of `owner` at `now`, oldest first.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError`] when the read fails.
    pub async fn list(&self, owner: &str, now: i64) -> Result<Vec<Listed>, StoreError> {
        let rows = sqlx::query(
            "SELECT id_hash, handle_key_id, handle_sealed, record FROM shl_link WHERE owner = $1",
        )
        .bind(owner)
        .fetch_all(&self.pool)
        .await
        .map_err(database)?;
        let mut listed = Vec::new();
        for row in &rows {
            let (_, link) = self.link_of(row)?;
            if link.expired(now) || link.answers_left == 0 {
                continue;
            }
            listed.push(Listed {
                handle: link.handle,
                label: link.label,
                created: link.created,
                expires: link.expires,
                passcode: link.passcode.is_some(),
                answers_left: link.answers_left,
            });
        }
        listed.sort_by(|a, b| (a.created, &a.handle).cmp(&(b.created, &b.handle)));
        Ok(listed)
    }

    /// Removes the link `handle` names when `owner` owns it.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError`] when the write does not commit.
    pub async fn revoke(&self, owner: &str, handle: &str) -> Result<Option<Gone>, StoreError> {
        let mut tx = self.pool.begin().await.map_err(database)?;
        let row = sqlx::query("SELECT id_hash, handle_key_id, handle_sealed, record FROM shl_link WHERE handle_hash = $1 FOR UPDATE")
        .bind(hashed(handle))
        .fetch_optional(&mut *tx)
        .await
        .map_err(database)?;
        let gone = match row {
            Some(row) => {
                let (id_hash, link) = self.link_of(&row)?;
                if link.owner == owner {
                    Some(remove(&mut tx, &id_hash, &link, Ending::Revoked).await?)
                } else {
                    None
                }
            }
            None => None,
        };
        tx.commit().await.map_err(database)?;
        Ok(gone)
    }

    /// Answers a manifest request for `id` at `now` with `passcode`, writing
    /// a location that lapses after `lifetime` seconds for each file longer
    /// than `embedded_max`.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError`] when the store fails.
    pub async fn answer(
        &self,
        id: &str,
        passcode: Option<&str>,
        embedded_max: Option<usize>,
        (now, lifetime): (i64, i64),
    ) -> Result<Answer, StoreError> {
        let id_hash = hashed(id);
        let checked = self.check(&id_hash, passcode).await?;
        let mut tx = self.pool.begin().await.map_err(database)?;
        let Some(mut link) = self.locked(&mut tx, &id_hash).await? else {
            return Ok(Answer::Inactive { gone: None });
        };
        let answer = if link.expired(now) {
            Answer::Inactive {
                gone: Some(remove(&mut tx, &id_hash, &link, Ending::Expired).await?),
            }
        } else if link.answers_left == 0 || link.resolution == Resolution::Direct {
            Answer::Inactive { gone: None }
        } else if let Some(refused) = refusal(&mut link, passcode, checked.as_ref()).await? {
            if refused.locked {
                remove(&mut tx, &id_hash, &link, Ending::Locked).await?;
            } else {
                put_link(&mut tx, &id_hash, &link).await?;
            }
            Answer::Refused {
                handle: link.handle.clone(),
                owner: link.owner.clone(),
                remaining: refused.remaining,
                locked: refused.locked,
            }
        } else {
            link.answers_left = link.answers_left.saturating_sub(1);
            let files = served(&mut tx, &id_hash, &mut link, embedded_max, (now, lifetime)).await?;
            let used = finish(&mut tx, &id_hash, &link).await?;
            Answer::Manifest {
                handle: link.handle,
                owner: link.owner,
                files,
                used,
            }
        };
        tx.commit().await.map_err(database)?;
        Ok(answer)
    }

    /// Returns the passcode hash of the link `id_hash` names and whether
    /// `passcode` matches it, checked before the transaction so parallel
    /// guesses hash at once and only the attempt count waits on the lock.
    async fn check(
        &self,
        id_hash: &[u8],
        passcode: Option<&str>,
    ) -> Result<Option<Checked>, StoreError> {
        let Some(given) = passcode else {
            return Ok(None);
        };
        let row = sqlx::query(
            "SELECT id_hash, handle_key_id, handle_sealed, record FROM shl_link WHERE id_hash = $1",
        )
        .bind(id_hash)
        .fetch_optional(&self.pool)
        .await
        .map_err(database)?;
        let Some(row) = row else {
            return Ok(None);
        };
        let (_, link) = self.link_of(&row)?;
        let Some(hash) = link.passcode else {
            return Ok(None);
        };
        let right = matches(given, &hash).await?;
        Ok(Some(Checked { hash, right }))
    }

    /// Fetches, once, the file the location `token` names at `now`.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError`] when the store fails.
    pub async fn fetch(&self, token: &str, now: i64) -> Result<Fetched, StoreError> {
        let token_hash = hashed(token);
        let mut tx = self.pool.begin().await.map_err(database)?;
        let owner: Option<Vec<u8>> =
            sqlx::query_scalar("SELECT id_hash FROM shl_location WHERE token_hash = $1")
                .bind(&token_hash)
                .fetch_optional(&mut *tx)
                .await
                .map_err(database)?;
        let Some(id_hash) = owner else {
            return Ok(Fetched::Inactive { gone: None });
        };
        let Some(mut link) = self.locked(&mut tx, &id_hash).await? else {
            return Ok(Fetched::Inactive { gone: None });
        };
        let location = sqlx::query(
            "DELETE FROM shl_location WHERE token_hash = $1 RETURNING position, lapses",
        )
        .bind(&token_hash)
        .fetch_optional(&mut *tx)
        .await
        .map_err(database)?;
        let fetched = match location {
            None => Fetched::Inactive { gone: None },
            Some(_) if link.expired(now) => Fetched::Inactive {
                gone: Some(remove(&mut tx, &id_hash, &link, Ending::Expired).await?),
            },
            Some(location) => {
                let position: i32 = location.try_get("position").map_err(database)?;
                let lapses: i64 = location.try_get("lapses").map_err(database)?;
                link.outstanding = link.outstanding.saturating_sub(1);
                if now >= lapses {
                    let used = finish(&mut tx, &id_hash, &link).await?;
                    Fetched::Inactive {
                        gone: used.then(|| gone(&link, Ending::Used)),
                    }
                } else {
                    let jwe = file(&mut tx, &id_hash, position).await?;
                    let used = finish(&mut tx, &id_hash, &link).await?;
                    Fetched::File {
                        handle: link.handle,
                        owner: link.owner,
                        jwe,
                        used,
                    }
                }
            }
        };
        tx.commit().await.map_err(database)?;
        Ok(fetched)
    }

    /// Answers a direct file request for `id` at `now`.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError`] when the store fails.
    pub async fn direct(&self, id: &str, now: i64) -> Result<Fetched, StoreError> {
        let id_hash = hashed(id);
        let mut tx = self.pool.begin().await.map_err(database)?;
        let fetched = match self.locked(&mut tx, &id_hash).await? {
            None => Fetched::Inactive { gone: None },
            Some(link) if link.expired(now) => Fetched::Inactive {
                gone: Some(remove(&mut tx, &id_hash, &link, Ending::Expired).await?),
            },
            Some(link) if link.resolution != Resolution::Direct || link.answers_left == 0 => {
                Fetched::Inactive { gone: None }
            }
            Some(mut link) => {
                link.answers_left = link.answers_left.saturating_sub(1);
                let jwe = file(&mut tx, &id_hash, 0).await?;
                let used = finish(&mut tx, &id_hash, &link).await?;
                Fetched::File {
                    handle: link.handle,
                    owner: link.owner,
                    jwe,
                    used,
                }
            }
        };
        tx.commit().await.map_err(database)?;
        Ok(fetched)
    }

    /// Removes every link past its expiry at `now` and every lapsed
    /// location, on one replica at a time, and returns the links that left
    /// the store; [`Swept::Busy`] while another replica sweeps.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError`] when the store fails.
    pub async fn sweep(&self, now: i64) -> Result<Swept, StoreError> {
        let mut tx = self.pool.begin().await.map_err(database)?;
        let held: bool = sqlx::query_scalar("SELECT pg_try_advisory_xact_lock($1)")
            .bind(SWEEP_LOCK)
            .fetch_one(&mut *tx)
            .await
            .map_err(database)?;
        if !held {
            return Ok(Swept::Busy);
        }
        let mut ended = Vec::new();
        let expired = sqlx::query("SELECT id_hash, handle_key_id, handle_sealed, record FROM shl_link WHERE expires <= $1 FOR UPDATE SKIP LOCKED")
        .bind(now)
        .fetch_all(&mut *tx)
        .await
        .map_err(database)?;
        for row in &expired {
            let (id_hash, link) = self.link_of(row)?;
            ended.push(remove(&mut tx, &id_hash, &link, Ending::Expired).await?);
        }
        let lapsed = sqlx::query(
            "SELECT id_hash, handle_key_id, handle_sealed, record FROM shl_link WHERE id_hash IN \
             (SELECT id_hash FROM shl_location WHERE lapses <= $1) FOR UPDATE SKIP LOCKED",
        )
        .bind(now)
        .fetch_all(&mut *tx)
        .await
        .map_err(database)?;
        for row in &lapsed {
            let (id_hash, mut link) = self.link_of(row)?;
            let removed =
                sqlx::query("DELETE FROM shl_location WHERE id_hash = $1 AND lapses <= $2")
                    .bind(&id_hash)
                    .bind(now)
                    .execute(&mut *tx)
                    .await
                    .map_err(database)?
                    .rows_affected();
            let removed = u32::try_from(removed).unwrap_or(u32::MAX);
            link.outstanding = link.outstanding.saturating_sub(removed);
            if finish(&mut tx, &id_hash, &link).await? {
                ended.push(gone(&link, Ending::Used));
            }
        }
        tx.commit().await.map_err(database)?;
        Ok(Swept::Done(ended))
    }

    /// Returns the link `id_hash` names, holding its row until `tx` ends.
    async fn locked(
        &self,
        tx: &mut PgConnection,
        id_hash: &[u8],
    ) -> Result<Option<Link>, StoreError> {
        let row = sqlx::query("SELECT id_hash, handle_key_id, handle_sealed, record FROM shl_link WHERE id_hash = $1 FOR UPDATE")
        .bind(id_hash)
        .fetch_optional(&mut *tx)
        .await
        .map_err(database)?;
        row.map(|row| self.link_of(&row).map(|(_, link)| link))
            .transpose()
    }

    /// Returns the manifest id hash and the link a row holds, its handle
    /// opened.
    fn link_of(&self, row: &PgRow) -> Result<(Vec<u8>, Link), StoreError> {
        let id_hash: Vec<u8> = row.try_get("id_hash").map_err(database)?;
        let key_id: String = row.try_get("handle_key_id").map_err(database)?;
        let sealed: Vec<u8> = row.try_get("handle_sealed").map_err(database)?;
        let record: serde_json::Value = row.try_get("record").map_err(database)?;
        let key_id = KeyId::new(&key_id)
            .map_err(|_refused| StoreError::Handle(crate::store::seal::SealError::UnknownKey))?;
        let handle = self
            .keys
            .open(&key_id, &handle_context(&id_hash), &sealed)
            .map_err(StoreError::Handle)?;
        let handle = String::from_utf8(handle)
            .map_err(|_invalid| StoreError::Handle(crate::store::seal::SealError::Cipher))?;
        let mut link: Link = serde_json::from_value(record).map_err(StoreError::Record)?;
        link.handle = handle;
        Ok((id_hash, link))
    }
}

/// Returns the associated data the handle of the link `id_hash` is sealed
/// with.
fn handle_context(id_hash: &[u8]) -> Vec<u8> {
    let mut context = b"shl_link.handle\x1f".to_vec();
    context.extend_from_slice(id_hash);
    context
}

/// Returns `link` as its row's record: every field but the handle.
fn record_of(link: &Link) -> Result<serde_json::Value, StoreError> {
    let mut record = serde_json::to_value(link).map_err(StoreError::Record)?;
    if let Some(object) = record.as_object_mut() {
        object.insert(
            String::from("handle"),
            serde_json::Value::String(String::new()),
        );
    }
    Ok(record)
}

/// Returns whether `given` matches the Argon2id `hash`, off the runtime.
async fn matches(given: &str, hash: &str) -> Result<bool, StoreError> {
    let (given, hash) = (given.to_owned(), hash.to_owned());
    tokio::task::spawn_blocking(move || super::passcode::matches(&given, &hash))
        .await
        .map_err(StoreError::Joined)?
        .map_err(StoreError::Passcode)
}

/// A refused passcode: the attempts left, and whether the link is locked.
struct Refused {
    /// The wrong passcodes the link still takes.
    remaining: u32,
    /// Whether this attempt locked it.
    locked: bool,
}

/// Returns the refusal of `passcode` against `link`'s, spending one attempt
/// of `link`, or `None` when the link takes no passcode or it matches.
async fn refusal(
    link: &mut Link,
    passcode: Option<&str>,
    checked: Option<&Checked>,
) -> Result<Option<Refused>, StoreError> {
    let Some(stored) = link.passcode.clone() else {
        return Ok(None);
    };
    let right = match (passcode, checked) {
        (Some(_), Some(checked)) if checked.hash == stored => checked.right,
        (Some(given), _) => matches(given, &stored).await?,
        (None, _) => false,
    };
    if right {
        return Ok(None);
    }
    link.attempts_left = link.attempts_left.saturating_sub(1);
    Ok(Some(Refused {
        remaining: link.attempts_left,
        locked: link.attempts_left == 0,
    }))
}

/// Returns the files of a manifest answer, embedding each no longer than
/// `embedded_max` and writing a location for every other.
async fn served(
    tx: &mut PgConnection,
    id_hash: &[u8],
    link: &mut Link,
    embedded_max: Option<usize>,
    (now, lifetime): (i64, i64),
) -> Result<Vec<Served>, StoreError> {
    let mut files = Vec::with_capacity(link.files.len());
    for (position, entry) in (0_u32..).zip(link.files.clone()) {
        let jwe = file(tx, id_hash, position_of(position)).await?;
        if embedded_max.is_some_and(|max| jwe.len() <= max) {
            files.push(Served {
                entry,
                embedded: Some(jwe),
                location: None,
            });
            continue;
        }
        let token = seal::random_token().map_err(StoreError::Seal)?;
        sqlx::query(
            "INSERT INTO shl_location (token_hash, id_hash, position, lapses) VALUES ($1, $2, $3, $4)",
        )
        .bind(hashed(&token))
        .bind(id_hash)
        .bind(position_of(position))
        .bind(now.saturating_add(lifetime))
        .execute(&mut *tx)
        .await
        .map_err(database)?;
        link.outstanding = link.outstanding.saturating_add(1);
        files.push(Served {
            entry,
            embedded: None,
            location: Some(token),
        });
    }
    Ok(files)
}

/// A passcode checked against a link's hash outside the transaction.
struct Checked {
    /// The hash it was checked against.
    hash: String,
    /// Whether it matched.
    right: bool,
}

/// Stores `link` under `id_hash`.
async fn put_link(tx: &mut PgConnection, id_hash: &[u8], link: &Link) -> Result<(), StoreError> {
    sqlx::query("UPDATE shl_link SET record = $2, expires = $3 WHERE id_hash = $1")
        .bind(id_hash)
        .bind(record_of(link)?)
        .bind(link.expires)
        .execute(&mut *tx)
        .await
        .map_err(database)?;
    Ok(())
}

/// Returns the JWE of the file at `position` of the link `id_hash`.
async fn file(tx: &mut PgConnection, id_hash: &[u8], position: i32) -> Result<String, StoreError> {
    sqlx::query_scalar("SELECT jwe FROM shl_file WHERE id_hash = $1 AND position = $2")
        .bind(id_hash)
        .bind(position)
        .fetch_optional(&mut *tx)
        .await
        .map_err(database)?
        .ok_or(StoreError::Missing)
}

/// Stores `link`, or removes it when it gives no more answers and no
/// location is outstanding; returns whether it was removed.
async fn finish(tx: &mut PgConnection, id_hash: &[u8], link: &Link) -> Result<bool, StoreError> {
    if link.answers_left == 0 && link.outstanding == 0 {
        remove(tx, id_hash, link, Ending::Used).await?;
        Ok(true)
    } else {
        put_link(tx, id_hash, link).await?;
        Ok(false)
    }
}

/// Returns the record of `link` leaving the store for `ending`.
fn gone(link: &Link, ending: Ending) -> Gone {
    Gone {
        handle: link.handle.clone(),
        owner: link.owner.clone(),
        ending,
    }
}

/// Removes the link `id_hash` with its files and its locations.
async fn remove(
    tx: &mut PgConnection,
    id_hash: &[u8],
    link: &Link,
    ending: Ending,
) -> Result<Gone, StoreError> {
    sqlx::query("DELETE FROM shl_link WHERE id_hash = $1")
        .bind(id_hash)
        .execute(&mut *tx)
        .await
        .map_err(database)?;
    Ok(gone(link, ending))
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::{Ending, FileEntry, Link, LinkStore, Resolution, StoreError, record_of};
    use crate::store::seal::{KeyId, LookupKey, SealKeys, SealingKey};

    /// The base64 of 32 bytes of `A`, made for the tests.
    const KEY: &str = "QUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUE=";

    /// A link store over a database no server answers.
    fn unreachable() -> LinkStore {
        let key = SealingKey::from_base64(KEY).expect("a 32-byte key");
        LinkStore {
            pool: sqlx::postgres::PgPoolOptions::new()
                .acquire_timeout(Duration::from_millis(300))
                .connect_lazy("postgres://bridge@127.0.0.1:1/state?sslmode=disable")
                .expect("a lazy pool"),
            keys: SealKeys::new(
                (KeyId::new("k").expect("an id"), key.clone()),
                Vec::new(),
                LookupKey::new(&key),
            ),
        }
    }

    /// A link of one file.
    fn link() -> Link {
        Link {
            handle: String::from("h-1"),
            owner: String::from("Patient/p"),
            created: 0,
            expires: 100,
            label: None,
            resolution: Resolution::Direct,
            passcode: None,
            attempts_left: 1,
            answers_left: 1,
            files: vec![FileEntry {
                content_type: String::from("application/fhir+json"),
                last_updated: String::from("1970-01-01T00:00:00Z"),
            }],
            outstanding: 0,
        }
    }

    /// Whether `outcome` is the database's refusal.
    fn refused<T: std::fmt::Debug>(outcome: &Result<T, StoreError>) -> bool {
        matches!(outcome, Err(StoreError::Database(_)))
    }

    #[tokio::test]
    async fn an_unreachable_store_refuses_every_call() {
        let store = unreachable();
        assert!(refused(
            &store.create("m", &link(), &[String::from("jwe")]).await
        ));
        assert!(refused(&store.list("Patient/p", 0).await));
        assert!(refused(&store.revoke("Patient/p", "h-1").await));
        assert!(refused(&store.answer("m", None, None, (0, 60)).await));
        assert!(refused(
            &store.answer("m", Some("1234"), None, (0, 60)).await
        ));
        assert!(refused(&store.fetch("t", 0).await));
        assert!(refused(&store.direct("m", 0).await));
        assert!(refused(&store.sweep(0).await));
    }

    #[test]
    fn a_record_leaves_the_handle_out_and_an_ending_is_named() {
        let record = record_of(&link()).expect("a record");
        assert_eq!(Some(""), record["handle"].as_str());
        assert_eq!(Some("Patient/p"), record["owner"].as_str());
        assert!(link().expired(100));
        assert_ne!(Ending::Expired, Ending::Used);
    }
}
