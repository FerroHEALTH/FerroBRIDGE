// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The links, their sealed files and their file locations, in one `redb`
//! file.
//!
//! The store holds a link's rules, the passcode's Argon2id hash and each
//! file as its compact JWE; never a key and never a plaintext file. A link
//! leaves the store whole when it expires, is revoked, is locked by its
//! passcode attempts, or has given its last answer and its last location has
//! been used or has lapsed. Every change of one link is one `redb` write
//! transaction (<https://docs.rs/redb/4.3.0/redb/>), so parallel requests
//! count its answers and passcode attempts exactly. No specification governs
//! the store: our own design, on one replica.

use std::path::Path;

use redb::Database;
use redb::MultimapTableDefinition;
use redb::ReadableDatabase;
use redb::ReadableTable;
use redb::TableDefinition;
use redb::WriteTransaction;
use serde::Deserialize;
use serde::Serialize;

use super::seal;

/// The manifest id to the link, as JSON.
const LINKS: TableDefinition<'static, &str, &str> = TableDefinition::new("links");

/// The handle the person manages a link by to its manifest id.
const HANDLES: TableDefinition<'static, &str, &str> = TableDefinition::new("handles");

/// The person to the handles of their links.
const OWNED: MultimapTableDefinition<'static, &str, &str> = MultimapTableDefinition::new("owned");

/// A link's manifest id and file position to the file's compact JWE.
const FILES: TableDefinition<'static, (&str, u32), &str> = TableDefinition::new("files");

/// A file location's token to the location, as JSON.
const LOCATIONS: TableDefinition<'static, &str, &str> = TableDefinition::new("locations");

/// Why the store could not answer.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum StoreError {
    /// The store file could not be opened or created.
    #[error("the link store at {} could not be opened", path.display())]
    Open {
        /// The path that was tried.
        path: std::path::PathBuf,
        /// What the storage layer reported.
        #[source]
        source: Box<redb::DatabaseError>,
    },
    /// A transaction could not be started or committed.
    #[error("a link store transaction did not complete")]
    Transaction(#[source] Box<redb::Error>),
    /// A stored record is not the shape this version writes.
    #[error("the link store holds a record this version cannot read")]
    Record(#[source] serde_json::Error),
    /// A fresh token could not be drawn.
    #[error("a file location could not be drawn")]
    Seal(#[source] seal::SealError),
    /// A link names a file the store does not hold.
    #[error("a link names a file the store does not hold")]
    Missing,
    /// A passcode could not be checked against its hash.
    #[error("a passcode could not be checked")]
    Passcode(#[source] super::passcode::PasscodeError),
}

/// Returns the error a `redb` failure is.
fn transaction<E: Into<redb::Error>>(source: E) -> StoreError {
    StoreError::Transaction(Box::new(source.into()))
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

/// A file location as the store holds it.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct Location {
    /// The manifest id of the link.
    link: String,
    /// The file's position.
    position: u32,
    /// When the location lapses, in epoch seconds.
    lapses: i64,
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

/// The links of this server, on disk.
#[derive(Debug)]
pub struct LinkStore {
    /// The open database.
    database: Database,
}

impl LinkStore {
    /// Opens, or creates, the store at `path`.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError`] when the file cannot be opened or its tables
    /// created.
    pub fn open(path: &Path) -> Result<Self, StoreError> {
        let database = Database::create(path).map_err(|source| StoreError::Open {
            source: Box::new(source),
            path: path.to_path_buf(),
        })?;
        let write = database.begin_write().map_err(transaction)?;
        {
            write.open_table(LINKS).map_err(transaction)?;
            write.open_table(HANDLES).map_err(transaction)?;
            write.open_multimap_table(OWNED).map_err(transaction)?;
            write.open_table(FILES).map_err(transaction)?;
            write.open_table(LOCATIONS).map_err(transaction)?;
        }
        write.commit().map_err(transaction)?;
        Ok(Self { database })
    }

    /// Stores `link` under the manifest id `id` with its sealed `files`.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError`] when the write does not commit.
    pub fn create(&self, id: &str, link: &Link, files: &[String]) -> Result<(), StoreError> {
        let write = self.database.begin_write().map_err(transaction)?;
        {
            put_link(&write, id, link)?;
            write
                .open_table(HANDLES)
                .map_err(transaction)?
                .insert(link.handle.as_str(), id)
                .map_err(transaction)?;
            write
                .open_multimap_table(OWNED)
                .map_err(transaction)?
                .insert(link.owner.as_str(), link.handle.as_str())
                .map_err(transaction)?;
            let mut table = write.open_table(FILES).map_err(transaction)?;
            for (position, jwe) in (0_u32..).zip(files) {
                table
                    .insert((id, position), jwe.as_str())
                    .map_err(transaction)?;
            }
        }
        write.commit().map_err(transaction)
    }

    /// Returns the active links of `owner` at `now`, oldest first.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError`] when the read fails.
    pub fn list(&self, owner: &str, now: i64) -> Result<Vec<Listed>, StoreError> {
        let read = self.database.begin_read().map_err(transaction)?;
        let by_owner = read.open_multimap_table(OWNED).map_err(transaction)?;
        let handles = read.open_table(HANDLES).map_err(transaction)?;
        let links = read.open_table(LINKS).map_err(transaction)?;
        let mut listed = Vec::new();
        for handle in by_owner.get(owner).map_err(transaction)? {
            let handle = handle.map_err(transaction)?;
            let Some(id) = handles.get(handle.value()).map_err(transaction)? else {
                continue;
            };
            let Some(text) = links.get(id.value()).map_err(transaction)? else {
                continue;
            };
            let link: Link = serde_json::from_str(text.value()).map_err(StoreError::Record)?;
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
    pub fn revoke(&self, owner: &str, handle: &str) -> Result<Option<Gone>, StoreError> {
        let write = self.database.begin_write().map_err(transaction)?;
        let id = write
            .open_table(HANDLES)
            .map_err(transaction)?
            .get(handle)
            .map_err(transaction)?
            .map(|id| id.value().to_owned());
        let gone = match id {
            Some(id) => match get_link(&write, &id)? {
                Some(link) if link.owner == owner => {
                    Some(remove(&write, &id, &link, Ending::Revoked)?)
                }
                _ => None,
            },
            None => None,
        };
        write.commit().map_err(transaction)?;
        Ok(gone)
    }

    /// Answers a manifest request for `id` at `now` with `passcode`, writing
    /// a location that lapses after `lifetime` seconds for each file longer
    /// than `embedded_max`.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError`] when the store fails.
    pub fn answer(
        &self,
        id: &str,
        passcode: Option<&str>,
        embedded_max: Option<usize>,
        (now, lifetime): (i64, i64),
    ) -> Result<Answer, StoreError> {
        let write = self.database.begin_write().map_err(transaction)?;
        let answer = Self::answer_in(&write, id, passcode, embedded_max, (now, lifetime))?;
        write.commit().map_err(transaction)?;
        Ok(answer)
    }

    /// The transaction of [`LinkStore::answer`].
    fn answer_in(
        write: &WriteTransaction,
        id: &str,
        passcode: Option<&str>,
        embedded_max: Option<usize>,
        (now, lifetime): (i64, i64),
    ) -> Result<Answer, StoreError> {
        let Some(mut link) = get_link(write, id)? else {
            return Ok(Answer::Inactive { gone: None });
        };
        if link.expired(now) {
            return Ok(Answer::Inactive {
                gone: Some(remove(write, id, &link, Ending::Expired)?),
            });
        }
        if link.answers_left == 0 || link.resolution == Resolution::Direct {
            return Ok(Answer::Inactive { gone: None });
        }
        if let Some(stored) = link.passcode.as_deref() {
            let right = match passcode {
                Some(given) => {
                    super::passcode::matches(given, stored).map_err(StoreError::Passcode)?
                }
                None => false,
            };
            if !right {
                link.attempts_left = link.attempts_left.saturating_sub(1);
                let (handle, owner, remaining) =
                    (link.handle.clone(), link.owner.clone(), link.attempts_left);
                let locked = remaining == 0;
                if locked {
                    remove(write, id, &link, Ending::Locked)?;
                } else {
                    put_link(write, id, &link)?;
                }
                return Ok(Answer::Refused {
                    handle,
                    owner,
                    remaining,
                    locked,
                });
            }
        }
        link.answers_left = link.answers_left.saturating_sub(1);
        let mut files = Vec::with_capacity(link.files.len());
        {
            let mut locations = write.open_table(LOCATIONS).map_err(transaction)?;
            for (position, entry) in (0_u32..).zip(&link.files) {
                let jwe = file(write, id, position)?;
                if embedded_max.is_some_and(|max| jwe.len() <= max) {
                    files.push(Served {
                        entry: entry.clone(),
                        embedded: Some(jwe),
                        location: None,
                    });
                    continue;
                }
                let token = seal::random_token().map_err(StoreError::Seal)?;
                let location = serde_json::to_string(&Location {
                    link: id.to_owned(),
                    position,
                    lapses: now.saturating_add(lifetime),
                })
                .map_err(StoreError::Record)?;
                locations
                    .insert(token.as_str(), location.as_str())
                    .map_err(transaction)?;
                link.outstanding = link.outstanding.saturating_add(1);
                files.push(Served {
                    entry: entry.clone(),
                    embedded: None,
                    location: Some(token),
                });
            }
        }
        let used = finish(write, id, &link)?;
        Ok(Answer::Manifest {
            handle: link.handle,
            owner: link.owner,
            files,
            used,
        })
    }

    /// Fetches, once, the file the location `token` names at `now`.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError`] when the store fails.
    pub fn fetch(&self, token: &str, now: i64) -> Result<Fetched, StoreError> {
        let write = self.database.begin_write().map_err(transaction)?;
        let location = write
            .open_table(LOCATIONS)
            .map_err(transaction)?
            .remove(token)
            .map_err(transaction)?
            .map(|text| serde_json::from_str::<Location>(text.value()))
            .transpose()
            .map_err(StoreError::Record)?;
        let fetched = match location {
            None => Fetched::Inactive { gone: None },
            Some(location) => match get_link(&write, &location.link)? {
                None => Fetched::Inactive { gone: None },
                Some(link) if link.expired(now) => Fetched::Inactive {
                    gone: Some(remove(&write, &location.link, &link, Ending::Expired)?),
                },
                Some(mut link) => {
                    link.outstanding = link.outstanding.saturating_sub(1);
                    if now >= location.lapses {
                        let used = finish(&write, &location.link, &link)?;
                        Fetched::Inactive {
                            gone: used.then(|| gone(&link, Ending::Used)),
                        }
                    } else {
                        let jwe = file(&write, &location.link, location.position)?;
                        let used = finish(&write, &location.link, &link)?;
                        Fetched::File {
                            handle: link.handle,
                            owner: link.owner,
                            jwe,
                            used,
                        }
                    }
                }
            },
        };
        write.commit().map_err(transaction)?;
        Ok(fetched)
    }

    /// Answers a direct file request for `id` at `now`.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError`] when the store fails.
    pub fn direct(&self, id: &str, now: i64) -> Result<Fetched, StoreError> {
        let write = self.database.begin_write().map_err(transaction)?;
        let fetched = match get_link(&write, id)? {
            None => Fetched::Inactive { gone: None },
            Some(link) if link.expired(now) => Fetched::Inactive {
                gone: Some(remove(&write, id, &link, Ending::Expired)?),
            },
            Some(link) if link.resolution != Resolution::Direct || link.answers_left == 0 => {
                Fetched::Inactive { gone: None }
            }
            Some(mut link) => {
                link.answers_left = link.answers_left.saturating_sub(1);
                let jwe = file(&write, id, 0)?;
                let used = finish(&write, id, &link)?;
                Fetched::File {
                    handle: link.handle,
                    owner: link.owner,
                    jwe,
                    used,
                }
            }
        };
        write.commit().map_err(transaction)?;
        Ok(fetched)
    }

    /// Removes every link past its expiry at `now` and every lapsed
    /// location, and returns the links that left the store.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError`] when the store fails.
    pub fn sweep(&self, now: i64) -> Result<Vec<Gone>, StoreError> {
        let write = self.database.begin_write().map_err(transaction)?;
        let mut ended = Vec::new();
        let expired: Vec<(String, Link)> = {
            let links = write.open_table(LINKS).map_err(transaction)?;
            let mut expired = Vec::new();
            for row in links.iter().map_err(transaction)? {
                let (id, text) = row.map_err(transaction)?;
                let link: Link = serde_json::from_str(text.value()).map_err(StoreError::Record)?;
                if link.expired(now) {
                    expired.push((id.value().to_owned(), link));
                }
            }
            expired
        };
        for (id, link) in &expired {
            ended.push(remove(&write, id, link, Ending::Expired)?);
        }
        let lapsed = locations_where(&write, |location| now >= location.lapses)?;
        for (token, location) in lapsed {
            write
                .open_table(LOCATIONS)
                .map_err(transaction)?
                .remove(token.as_str())
                .map_err(transaction)?;
            if let Some(mut link) = get_link(&write, &location.link)? {
                link.outstanding = link.outstanding.saturating_sub(1);
                if finish(&write, &location.link, &link)? {
                    ended.push(gone(&link, Ending::Used));
                }
            }
        }
        write.commit().map_err(transaction)?;
        Ok(ended)
    }
}

/// Returns the link stored under `id`.
fn get_link(write: &WriteTransaction, id: &str) -> Result<Option<Link>, StoreError> {
    write
        .open_table(LINKS)
        .map_err(transaction)?
        .get(id)
        .map_err(transaction)?
        .map(|text| serde_json::from_str(text.value()).map_err(StoreError::Record))
        .transpose()
}

/// Stores `link` under `id`.
fn put_link(write: &WriteTransaction, id: &str, link: &Link) -> Result<(), StoreError> {
    let text = serde_json::to_string(link).map_err(StoreError::Record)?;
    write
        .open_table(LINKS)
        .map_err(transaction)?
        .insert(id, text.as_str())
        .map_err(transaction)?;
    Ok(())
}

/// Returns the JWE of the file at `position` of the link `id`.
fn file(write: &WriteTransaction, id: &str, position: u32) -> Result<String, StoreError> {
    write
        .open_table(FILES)
        .map_err(transaction)?
        .get((id, position))
        .map_err(transaction)?
        .map(|jwe| jwe.value().to_owned())
        .ok_or(StoreError::Missing)
}

/// Stores `link`, or removes it when it gives no more answers and no
/// location is outstanding; returns whether it was removed.
fn finish(write: &WriteTransaction, id: &str, link: &Link) -> Result<bool, StoreError> {
    if link.answers_left == 0 && link.outstanding == 0 {
        remove(write, id, link, Ending::Used)?;
        Ok(true)
    } else {
        put_link(write, id, link)?;
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

/// Removes the link `id` with its files, its handle and its locations.
fn remove(
    write: &WriteTransaction,
    id: &str,
    link: &Link,
    ending: Ending,
) -> Result<Gone, StoreError> {
    write
        .open_table(LINKS)
        .map_err(transaction)?
        .remove(id)
        .map_err(transaction)?;
    write
        .open_table(HANDLES)
        .map_err(transaction)?
        .remove(link.handle.as_str())
        .map_err(transaction)?;
    write
        .open_multimap_table(OWNED)
        .map_err(transaction)?
        .remove(link.owner.as_str(), link.handle.as_str())
        .map_err(transaction)?;
    {
        let mut files = write.open_table(FILES).map_err(transaction)?;
        for (position, _entry) in (0_u32..).zip(&link.files) {
            files.remove((id, position)).map_err(transaction)?;
        }
    }
    if link.outstanding > 0 {
        let owned = locations_where(write, |location| location.link == id)?;
        let mut locations = write.open_table(LOCATIONS).map_err(transaction)?;
        for (token, _location) in owned {
            locations.remove(token.as_str()).map_err(transaction)?;
        }
    }
    Ok(gone(link, ending))
}

/// Returns every location `keep` selects, with its token.
fn locations_where(
    write: &WriteTransaction,
    keep: impl Fn(&Location) -> bool,
) -> Result<Vec<(String, Location)>, StoreError> {
    let locations = write.open_table(LOCATIONS).map_err(transaction)?;
    let mut selected = Vec::new();
    for row in locations.iter().map_err(transaction)? {
        let (token, text) = row.map_err(transaction)?;
        let location: Location = serde_json::from_str(text.value()).map_err(StoreError::Record)?;
        if keep(&location) {
            selected.push((token.value().to_owned(), location));
        }
    }
    Ok(selected)
}
