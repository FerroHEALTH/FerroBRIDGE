// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! What the outbound lane has delivered, in a `redb` file: for each
//! composition, the `[type]/[id]` of every resource its latest delivery
//! carried, and for each EHR, its compositions.
//!
//! A deletion or an erasure takes the content away, so the resources to
//! retract are read from here, and an update retracts the resources its
//! earlier delivery carried and this one does not. A write is one ACID
//! transaction (<https://docs.rs/redb/4.3.0/redb/>). The file holds
//! identifiers and nothing else. No specification governs this: our own
//! design.

use std::collections::BTreeSet;
use std::path::Path;

use redb::Database;
use redb::MultimapTableDefinition;
use redb::ReadableDatabase;
use redb::TableDefinition;

/// Versioned object id to the references its latest delivery carried, as
/// JSON.
const DELIVERED: TableDefinition<'static, &str, &str> = TableDefinition::new("delivered");

/// EHR id to the versioned object ids delivered for it.
const COMPOSITIONS: MultimapTableDefinition<'static, &str, &str> =
    MultimapTableDefinition::new("ehr_compositions");

/// Why the store could not be read or written.
#[derive(Debug, thiserror::Error)]
#[error("the outbound lane's delivery record could not be read or written")]
pub struct EmittedError(#[source] Box<dyn std::error::Error + Send + Sync>);

/// Returns the store error `source` is.
fn store(source: impl Into<redb::Error>) -> EmittedError {
    EmittedError(Box::new(source.into()))
}

/// The delivery record, on disk.
#[derive(Debug)]
pub struct Emitted {
    /// The open database.
    database: Database,
}

impl Emitted {
    /// Opens, or creates, the record at `path`.
    ///
    /// # Errors
    /// Returns [`EmittedError`] when the file cannot be opened or created or
    /// its tables cannot be created.
    pub fn open(path: &Path) -> Result<Self, EmittedError> {
        let database = Database::create(path).map_err(store)?;
        let write = database.begin_write().map_err(store)?;
        {
            let _delivered = write.open_table(DELIVERED).map_err(store)?;
            let _compositions = write.open_multimap_table(COMPOSITIONS).map_err(store)?;
        }
        write.commit().map_err(store)?;
        Ok(Self { database })
    }

    /// Returns the references the latest delivery of the composition
    /// `vo_id` carried.
    ///
    /// # Errors
    /// Returns [`EmittedError`] when the read fails or the row does not read.
    pub fn delivered(&self, vo_id: &str) -> Result<BTreeSet<String>, EmittedError> {
        let read = self.database.begin_read().map_err(store)?;
        let table = read.open_table(DELIVERED).map_err(store)?;
        let Some(row) = table.get(vo_id).map_err(store)? else {
            return Ok(BTreeSet::new());
        };
        serde_json::from_str(row.value()).map_err(|error| EmittedError(Box::new(error)))
    }

    /// Returns the compositions delivered for the EHR `ehr_id`.
    ///
    /// # Errors
    /// Returns [`EmittedError`] when the read fails.
    pub fn compositions(&self, ehr_id: &str) -> Result<Vec<String>, EmittedError> {
        let read = self.database.begin_read().map_err(store)?;
        let table = read.open_multimap_table(COMPOSITIONS).map_err(store)?;
        let mut found = Vec::new();
        for value in table.get(ehr_id).map_err(store)? {
            found.push(value.map_err(store)?.value().to_owned());
        }
        Ok(found)
    }

    /// Records `delivered` as what the composition `vo_id` of the EHR
    /// `ehr_id` stands at: none removes the composition from the record.
    ///
    /// # Errors
    /// Returns [`EmittedError`] when the write does not commit.
    pub fn record(
        &self,
        ehr_id: &str,
        vo_id: &str,
        delivered: &BTreeSet<String>,
    ) -> Result<(), EmittedError> {
        let write = self.database.begin_write().map_err(store)?;
        {
            let mut table = write.open_table(DELIVERED).map_err(store)?;
            let mut compositions = write.open_multimap_table(COMPOSITIONS).map_err(store)?;
            if delivered.is_empty() {
                table.remove(vo_id).map_err(store)?;
                compositions.remove(ehr_id, vo_id).map_err(store)?;
            } else {
                let row = serde_json::to_string(delivered)
                    .map_err(|error| EmittedError(Box::new(error)))?;
                table.insert(vo_id, row.as_str()).map_err(store)?;
                compositions.insert(ehr_id, vo_id).map_err(store)?;
            }
        }
        write.commit().map_err(store)
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::Emitted;

    #[test]
    fn a_delivery_is_recorded_and_an_empty_one_removes_it() {
        let directory = tempfile::tempdir().expect("a scratch directory");
        let store =
            Emitted::open(&directory.path().join("emitted.redb")).expect("the record opens");
        let one = BTreeSet::from([String::from("Condition/c-1")]);
        store
            .record("ehr-1", "vo-1", &one)
            .expect("the write commits");
        assert_eq!(one, store.delivered("vo-1").expect("the read"));
        assert_eq!(
            vec![String::from("vo-1")],
            store.compositions("ehr-1").expect("the read")
        );
        store
            .record("ehr-1", "vo-1", &BTreeSet::new())
            .expect("the write commits");
        assert_eq!(BTreeSet::new(), store.delivered("vo-1").expect("the read"));
        assert_eq!(
            Vec::<String>::new(),
            store.compositions("ehr-1").expect("the read")
        );
    }
}
