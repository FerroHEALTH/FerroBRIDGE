// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The identity map on disk, over `redb`.
//!
//! Seven `redb` tables hold the identity map: a patient identifier to its
//! `ehr_id`, each `ehr_id` back to the person it was recorded for, an
//! external resource id to the internal one, an internal id to its
//! composition binding, a source to what consumed it, a source to the
//! contribution that committed it, and each `Resource.identifier` of a
//! committed resource to its logical id. An eighth holds the store's schema
//! (no specification governs the store: our own design). The file
//! holds identifiers, archetype paths and mapping names and nothing else: no
//! clinical value ever reaches it, which
//! `the_store_never_holds_clinical_content` asserts by reading the file back
//! as bytes.
//!
//! `redb` is an ACID embedded store (<https://docs.rs/redb/4.3.0/redb/>), so a
//! record-once write is one write transaction that reads the key first and
//! commits only when the key was free.

mod map;
mod schema;

use std::path::Path;
use std::path::PathBuf;

use redb::Database;
use redb::ReadableDatabase;
use redb::ReadableTable;
use redb::TableDefinition;

use crate::facade::identity::PersonId;
use crate::facade::identity::record::EhrSubject;
use crate::facade::identity::store::StoreError;

/// Patient identifier to `ehr_id`.
const PATIENTS: TableDefinition<'static, &str, &str> = TableDefinition::new("patient_ehr");

/// `ehr_id` to the person it was first recorded for, as JSON.
const SUBJECTS: TableDefinition<'static, &str, &str> = TableDefinition::new("ehr_patient");

/// External resource id to internal resource id.
const EXTERNAL: TableDefinition<'static, &str, &str> = TableDefinition::new("external_internal");

/// Internal resource id to the composition binding, as JSON.
const BINDINGS: TableDefinition<'static, &str, &str> = TableDefinition::new("internal_composition");

/// Source resource `id` and `meta.versionId` to what consumed them, as JSON.
const SOURCES: TableDefinition<'static, &str, &str> = TableDefinition::new("source_mapping");

/// Source resource `id` and `meta.versionId` to the contribution that
/// committed them, as JSON.
const CONTRIBUTIONS: TableDefinition<'static, &str, &str> =
    TableDefinition::new("source_contribution");

/// Resource type, `Resource.identifier` and logical id to that logical id.
const IDENTIFIERS: TableDefinition<'static, &str, &str> =
    TableDefinition::new("identifier_internal");

/// The store's own facts, such as the schema it was written in.
const META: TableDefinition<'static, &str, &str> = TableDefinition::new("store_meta");

/// The key [`META`] holds the schema under.
const SCHEMA_KEY: &str = "schema";

/// The schema this build writes.
///
/// Schema 4 holds the bindings of identity derivation 3, whose ids are the
/// deterministic ids the engine's `$tofhir` answers. An id an earlier
/// schema recorded was derived another way, so a file of schema 1 to 3 is
/// refused rather than read, and its reset is to remove the file, which the
/// bridge then creates anew (no specification governs the store: our own
/// design).
pub const SCHEMA: u32 = 4;

/// The oldest schema this build reads.
pub const OLDEST_SCHEMA: u32 = 4;

/// The identity map of one configured CDR, on disk.
///
/// One store per configured CDR: a multi-tenant deployment runs one bridge per
/// tenant (no specification governs this: our own design).
#[derive(Debug)]
pub struct RedbStore {
    /// The open database.
    database: Database,
    /// The path it was opened at, for a diagnostic that names the file.
    path: PathBuf,
}

impl RedbStore {
    /// Opens, or creates, the store at `path`.
    ///
    /// The seven tables are created in one transaction, so every later read
    /// finds a table rather than a missing one. A store an earlier version
    /// wrote has no reverse row for its EHRs, so the same transaction writes
    /// one for every patient row that lacks it. The same transaction checks
    /// the schema the file was written in and stamps it with [`SCHEMA`].
    ///
    /// # Errors
    ///
    /// Returns [`StoreError::Open`] when the path cannot be opened or created,
    /// [`StoreError::Schema`] when the file carries a schema outside
    /// [`OLDEST_SCHEMA`] to [`SCHEMA`], and [`StoreError::Transaction`] when
    /// the table creation does not commit.
    pub fn open(path: &Path) -> Result<Self, StoreError> {
        let database = Database::create(path).map_err(|source| StoreError::Open {
            path: path.display().to_string(),
            source: Box::new(source),
        })?;
        let store = Self {
            database,
            path: path.to_path_buf(),
        };
        let write = store.database.begin_write().map_err(transaction)?;
        {
            let fresh = write.list_tables().map_err(transaction)?.next().is_none();
            let mut meta = write.open_table(META).map_err(transaction)?;
            schema::stamp(&mut meta, path, fresh)?;
            let patients = write.open_table(PATIENTS).map_err(transaction)?;
            let mut subjects = write.open_table(SUBJECTS).map_err(transaction)?;
            schema::backfill(&patients, &mut subjects)?;
            let _external = write.open_table(EXTERNAL).map_err(transaction)?;
            let _bindings = write.open_table(BINDINGS).map_err(transaction)?;
            let _sources = write.open_table(SOURCES).map_err(transaction)?;
            let _contributions = write.open_table(CONTRIBUTIONS).map_err(transaction)?;
            let _identifiers = write.open_table(IDENTIFIERS).map_err(transaction)?;
        }
        write.commit().map_err(transaction)?;
        Ok(store)
    }

    /// Returns the path the store was opened at.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Returns the value `key` holds in `table`.
    fn get(
        &self,
        table: TableDefinition<'static, &str, &str>,
        key: &str,
    ) -> Result<Option<String>, StoreError> {
        let read = self.database.begin_read().map_err(transaction)?;
        let opened = read.open_table(table).map_err(transaction)?;
        let found = opened.get(key).map_err(transaction)?;
        Ok(found.map(|value| value.value().to_owned()))
    }

    /// Returns every key and value `table` holds in `range`, in key order.
    fn scan(
        &self,
        table: TableDefinition<'static, &str, &str>,
        range: &core::ops::Range<String>,
    ) -> Result<Vec<(String, String)>, StoreError> {
        let read = self.database.begin_read().map_err(transaction)?;
        let opened = read.open_table(table).map_err(transaction)?;
        let rows = opened
            .range(range.start.as_str()..range.end.as_str())
            .map_err(transaction)?;
        rows.map(|row| {
            let (key, value) = row.map_err(transaction)?;
            Ok((key.value().to_owned(), value.value().to_owned()))
        })
        .collect()
    }

    /// Writes `value` under `key` in `table` unless the key is already taken,
    /// and returns the value that stands.
    fn record(
        &self,
        table: TableDefinition<'static, &str, &str>,
        key: &str,
        value: &str,
    ) -> Result<String, StoreError> {
        let write = self.database.begin_write().map_err(transaction)?;
        let stood = {
            let mut opened = write.open_table(table).map_err(transaction)?;
            let existing = opened
                .get(key)
                .map_err(transaction)?
                .map(|held| held.value().to_owned());
            if let Some(held) = existing {
                held
            } else {
                opened.insert(key, value).map_err(transaction)?;
                value.to_owned()
            }
        };
        write.commit().map_err(transaction)?;
        Ok(stood)
    }
}

/// Returns the reverse row `person` is stored as.
fn subject_record(person: &PersonId) -> EhrSubject {
    EhrSubject {
        namespace: person.namespace().to_owned(),
        id: person.id().to_owned(),
    }
}

/// Returns the identifier this store read back, or a refusal naming the key.
fn identifier<T, E>(
    key: &str,
    kind: &'static str,
    value: Option<String>,
    parse: impl Fn(&str) -> Result<T, E>,
) -> Result<Option<T>, StoreError>
where
    E: std::error::Error + Send + Sync + 'static,
{
    match value {
        None => Ok(None),
        Some(text) => match parse(&text) {
            Ok(parsed) => Ok(Some(parsed)),
            Err(source) => Err(StoreError::Identifier {
                key: key.to_owned(),
                kind,
                value: text,
                source: Box::new(source),
            }),
        },
    }
}

/// Returns the record `text` holds, or a refusal naming the key.
fn record_of<T: serde::de::DeserializeOwned>(
    key: &str,
    text: Option<String>,
) -> Result<Option<T>, StoreError> {
    match text {
        None => Ok(None),
        Some(text) => serde_json::from_str(&text)
            .map(Some)
            .map_err(|source| StoreError::Record {
                key: key.to_owned(),
                source: Box::new(source),
            }),
    }
}

/// Returns the record `value` writes, or a refusal naming the key.
fn rendered<T: serde::Serialize>(key: &str, value: &T) -> Result<String, StoreError> {
    serde_json::to_string(value).map_err(|source| StoreError::Record {
        key: key.to_owned(),
        source: Box::new(source),
    })
}

/// Wraps a `redb` refusal as a store refusal that carries it.
fn transaction<E: Into<redb::Error>>(source: E) -> StoreError {
    StoreError::Transaction {
        source: Box::new(source.into()),
    }
}
