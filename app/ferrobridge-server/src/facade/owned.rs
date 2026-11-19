// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The resources the facade keeps itself, because the CDR has no home for
//! them: SMART App State (`Basic`) and SMART launch `Task`s.
//!
//! One `redb` file holds one kind of resource, each under `[type]/[id]` with
//! its version, as the JSON the client sent plus the `id` and `meta` the
//! server writes. A deleted resource leaves a tombstone, so a later request
//! about its id is told it is gone. No specification governs the store: our
//! own design. A conditional write reads and writes in one `redb` write
//! transaction (<https://docs.rs/redb/4.3.0/redb/>), so two writers racing
//! on one version cannot both win.

use std::path::Path;
use std::path::PathBuf;

use fhir_types::codec::Object;
use redb::Database;
use redb::ReadableDatabase;
use redb::ReadableTable;
use redb::TableDefinition;
use serde::Deserialize;
use serde::Serialize;

/// `[type]/[id]` to the stored record, as JSON.
const RESOURCES: TableDefinition<'static, &str, &str> = TableDefinition::new("resources");

/// Why the store could not answer.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum OwnedError {
    /// The store file could not be opened or created.
    #[error("the resource store at {path} could not be opened")]
    Open {
        /// The path that was tried.
        path: String,
        /// What the storage layer reported.
        #[source]
        source: Box<redb::DatabaseError>,
    },
    /// A transaction could not be started or committed.
    #[error("a resource store transaction did not complete")]
    Transaction {
        /// What the storage layer reported.
        #[source]
        source: Box<redb::Error>,
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

/// Returns the error a `redb` failure is.
fn transaction<E: Into<redb::Error>>(source: E) -> OwnedError {
    OwnedError::Transaction {
        source: Box::new(source.into()),
    }
}

/// One record as it is stored.
#[derive(Debug, Clone, Serialize, Deserialize)]
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

/// The resources of one kind, on disk.
#[derive(Debug)]
pub struct OwnedStore {
    /// The open database.
    database: Database,
    /// The path it was opened at.
    path: PathBuf,
}

impl OwnedStore {
    /// Opens, or creates, the store at `path`.
    ///
    /// # Errors
    ///
    /// Returns [`OwnedError::Open`] when the path cannot be opened or
    /// created, and [`OwnedError::Transaction`] when the table cannot be
    /// created.
    pub fn open(path: &Path) -> Result<Self, OwnedError> {
        let database = Database::create(path).map_err(|source| OwnedError::Open {
            path: path.display().to_string(),
            source: Box::new(source),
        })?;
        let write = database.begin_write().map_err(transaction)?;
        {
            let _resources = write.open_table(RESOURCES).map_err(transaction)?;
        }
        write.commit().map_err(transaction)?;
        Ok(Self {
            database,
            path: path.to_path_buf(),
        })
    }

    /// Returns the path the store was opened at.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Stores `resource` as version 1 of a new `resource_type` under a fresh
    /// id, and returns it with its `id` and `meta` written.
    ///
    /// # Errors
    ///
    /// Returns [`OwnedError`] when the write does not commit.
    pub fn create(&self, resource_type: &str, resource: Object) -> Result<Object, OwnedError> {
        let id = uuid::Uuid::new_v4().to_string();
        let key = key_of(resource_type, &id);
        let stamped = stamp(resource, &id, 1);
        let record = encode(
            &key,
            &Record {
                version: 1,
                resource: Some(stamped.clone()),
            },
        )?;
        let write = self.database.begin_write().map_err(transaction)?;
        {
            let mut table = write.open_table(RESOURCES).map_err(transaction)?;
            table
                .insert(key.as_str(), record.as_str())
                .map_err(transaction)?;
        }
        write.commit().map_err(transaction)?;
        Ok(stamped)
    }

    /// Returns what the store holds for `resource_type`/`id`.
    ///
    /// # Errors
    ///
    /// Returns [`OwnedError`] when the read fails or the record cannot be
    /// read.
    pub fn get(&self, resource_type: &str, id: &str) -> Result<Option<Held>, OwnedError> {
        let key = key_of(resource_type, id);
        let read = self.database.begin_read().map_err(transaction)?;
        let table = read.open_table(RESOURCES).map_err(transaction)?;
        let found = table.get(key.as_str()).map_err(transaction)?;
        found
            .map(|value| decode(&key, value.value()).map(Held::from))
            .transpose()
    }

    /// Applies the change `decide` returns for what the store holds under
    /// `resource_type`/`id`, in one write transaction, and returns what the
    /// store then holds; a refusal of `decide` writes nothing.
    ///
    /// # Errors
    ///
    /// Returns [`OwnedError`] when the transaction fails; the inner
    /// `Err` is `decide`'s refusal.
    pub fn change<R>(
        &self,
        resource_type: &str,
        id: &str,
        decide: impl FnOnce(Option<&Held>) -> Result<Change, R>,
    ) -> Result<Result<Held, R>, OwnedError> {
        let key = key_of(resource_type, id);
        let write = self.database.begin_write().map_err(transaction)?;
        let outcome = {
            let mut table = write.open_table(RESOURCES).map_err(transaction)?;
            let held = table
                .get(key.as_str())
                .map_err(transaction)?
                .map(|value| decode(&key, value.value()).map(Held::from))
                .transpose()?;
            match decide(held.as_ref()) {
                Err(refusal) => Err(refusal),
                Ok(change) => {
                    let version = held
                        .as_ref()
                        .map_or(0, Held::version)
                        .checked_add(1)
                        .ok_or_else(|| exhausted(&key))?;
                    let record = match change {
                        Change::Put(resource) => Record {
                            version,
                            resource: Some(stamp(resource, id, version)),
                        },
                        Change::Delete => Record {
                            version,
                            resource: None,
                        },
                    };
                    let encoded = encode(&key, &record)?;
                    table
                        .insert(key.as_str(), encoded.as_str())
                        .map_err(transaction)?;
                    Ok(Held::from(record))
                }
            }
        };
        if outcome.is_ok() {
            write.commit().map_err(transaction)?;
        } else {
            write.abort().map_err(transaction)?;
        }
        Ok(outcome)
    }

    /// Returns every live resource of `resource_type`, in id order.
    ///
    /// # Errors
    ///
    /// Returns [`OwnedError`] when the read fails or a record cannot be read.
    pub fn scan(&self, resource_type: &str) -> Result<Vec<Object>, OwnedError> {
        let start = format!("{resource_type}/");
        let end = format!("{resource_type}0");
        let read = self.database.begin_read().map_err(transaction)?;
        let table = read.open_table(RESOURCES).map_err(transaction)?;
        let rows = table
            .range(start.as_str()..end.as_str())
            .map_err(transaction)?;
        let mut live = Vec::new();
        for row in rows {
            let (key, value) = row.map_err(transaction)?;
            if let Some(resource) = decode(key.value(), value.value())?.resource {
                live.push(resource);
            }
        }
        Ok(live)
    }
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

/// Returns `record` as the JSON it is stored as.
fn encode(key: &str, record: &Record) -> Result<String, OwnedError> {
    serde_json::to_string(record).map_err(|source| OwnedError::Record {
        key: key.to_owned(),
        source: Box::new(source),
    })
}

/// Returns the record `text` stores under `key`.
fn decode(key: &str, text: &str) -> Result<Record, OwnedError> {
    serde_json::from_str(text).map_err(|source| OwnedError::Record {
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
    use super::Change;
    use super::Held;
    use super::OwnedStore;
    use fhir_types::codec::Object;
    use fhir_types::codec::Value;

    fn store() -> (tempfile::TempDir, OwnedStore) {
        let directory = tempfile::tempdir().expect("a directory");
        let store = OwnedStore::open(&directory.path().join("owned.redb")).expect("the store");
        (directory, store)
    }

    fn basic() -> Object {
        serde_json::from_str(r#"{"resourceType":"Basic","code":{"text":"x"}}"#).expect("JSON")
    }

    #[test]
    fn a_created_resource_carries_its_id_and_first_version() {
        let (_directory, store) = store();
        let created = store.create("Basic", basic()).expect("created");
        let Some(Value::String(id)) = created.get("id") else {
            panic!("an id");
        };
        let Some(Held::Live { version, resource }) = store.get("Basic", id).expect("read") else {
            panic!("live");
        };
        assert_eq!(1, version);
        assert_eq!(created, resource);
        let serialized = serde_json::to_value(&resource).expect("JSON");
        assert_eq!(Some("1"), serialized["meta"]["versionId"].as_str());
    }

    #[test]
    fn a_change_moves_the_version_and_a_refusal_writes_nothing() {
        let (_directory, store) = store();
        let created = store.create("Basic", basic()).expect("created");
        let Some(Value::String(id)) = created.get("id") else {
            panic!("an id");
        };
        let refused = store
            .change("Basic", id, |_held| Err::<Change, &str>("no"))
            .expect("ran");
        assert_eq!(Err("no"), refused);
        let updated = store
            .change("Basic", id, |_held| Ok::<_, ()>(Change::Put(basic())))
            .expect("ran")
            .expect("changed");
        assert_eq!(2, updated.version());
        let deleted = store
            .change("Basic", id, |_held| Ok::<_, ()>(Change::Delete))
            .expect("ran")
            .expect("changed");
        assert_eq!(Held::Deleted { version: 3 }, deleted);
        assert_eq!(
            Some(Held::Deleted { version: 3 }),
            store.get("Basic", id).expect("read")
        );
        assert_eq!(Vec::<Object>::new(), store.scan("Basic").expect("scanned"));
    }

    #[test]
    fn a_scan_reads_one_type_only() {
        let (_directory, store) = store();
        store.create("Basic", basic()).expect("created");
        store.create("BasicX", basic()).expect("created");
        store.create("Task", basic()).expect("created");
        assert_eq!(1, store.scan("Basic").expect("scanned").len());
    }
}
