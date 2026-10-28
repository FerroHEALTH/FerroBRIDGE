// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The schema of the store file, and the rows an earlier build left out.
//!
//! [`RedbStore::open`](crate::facade::identity::redb_store::RedbStore::open)
//! runs both before it hands the store out, in the one write transaction that
//! creates the tables (no specification governs the store: our own design).

use std::path::Path;

use redb::ReadableTable;

use crate::facade::identity::PersonId;
use crate::facade::identity::redb_store::OLDEST_SCHEMA;
use crate::facade::identity::redb_store::SCHEMA;
use crate::facade::identity::redb_store::SCHEMA_KEY;
use crate::facade::identity::redb_store::rendered;
use crate::facade::identity::redb_store::subject_record;
use crate::facade::identity::redb_store::transaction;
use crate::facade::identity::store::StoreError;

/// Checks the schema `meta` records and stamps it with [`SCHEMA`].
///
/// `fresh` says the file held no table before this open. A file that held
/// tables and no schema row was written by a build up to 0.0.6, schema 1. A
/// schema this build does not read is refused before anything else is
/// written, and the refusal names the file and its schema.
pub(super) fn stamp(
    meta: &mut redb::Table<'_, &str, &str>,
    path: &Path,
    fresh: bool,
) -> Result<(), StoreError> {
    let held = meta
        .get(SCHEMA_KEY)
        .map_err(transaction)?
        .map(|value| value.value().to_owned());
    let found = match held {
        Some(text) => Some(text),
        None if fresh => None,
        None => Some(String::from("1")),
    };
    if let Some(text) = found {
        let readable = text
            .parse::<u32>()
            .is_ok_and(|schema| (OLDEST_SCHEMA..=SCHEMA).contains(&schema));
        if !readable {
            return Err(StoreError::Schema {
                path: path.display().to_string(),
                found: text,
                oldest: OLDEST_SCHEMA,
                newest: SCHEMA,
            });
        }
    }
    meta.insert(SCHEMA_KEY, SCHEMA.to_string().as_str())
        .map_err(transaction)?;
    Ok(())
}

/// Writes the reverse row of every patient row that has none.
///
/// A patient key is [`PersonId`]'s `namespace|id` rendering, split at its
/// first `|`, as the R4 token search splits `system|code`
/// (<https://hl7.org/fhir/R4/search.html#token>).
pub(super) fn backfill(
    patients: &redb::Table<'_, &str, &str>,
    subjects: &mut redb::Table<'_, &str, &str>,
) -> Result<(), StoreError> {
    let mut rows: Vec<(String, String)> = Vec::new();
    for row in patients.iter().map_err(transaction)? {
        let (key, value) = row.map_err(transaction)?;
        rows.push((key.value().to_owned(), value.value().to_owned()));
    }
    for (key, ehr) in rows {
        if subjects.get(ehr.as_str()).map_err(transaction)?.is_some() {
            continue;
        }
        let (namespace, id) = key.split_once('|').unwrap_or((key.as_str(), ""));
        let person = PersonId::new(namespace, id).map_err(|source| StoreError::Identifier {
            key: key.clone(),
            kind: "person key",
            value: key.clone(),
            source: Box::new(source),
        })?;
        let offered = rendered(&ehr, &subject_record(&person))?;
        subjects
            .insert(ehr.as_str(), offered.as_str())
            .map_err(transaction)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::cdr::ids::EhrId;
    use crate::facade::identity::redb_store::META;
    use crate::facade::identity::redb_store::PATIENTS;
    use crate::facade::identity::redb_store::RedbStore;
    use crate::facade::identity::redb_store::SCHEMA;
    use crate::facade::identity::redb_store::SCHEMA_KEY;
    use crate::facade::identity::store::Store;
    use crate::facade::identity::store::StoreError;
    use crate::facade::identity::{FhirResourceId, PersonId};

    #[test]
    fn a_store_without_reverse_rows_gains_them_when_it_opens() {
        let directory = tempfile::tempdir().expect("a temporary directory");
        let path = directory.path().join("identity.redb");
        {
            let database = redb::Database::create(&path).expect("the file");
            let write = database.begin_write().expect("a write");
            {
                let mut patients = write.open_table(PATIENTS).expect("the table");
                patients
                    .insert(
                        "http://example.org/ns|p-1",
                        "bd6b1e5a-3b9b-4a4a-9e0b-9f4b3a0c9f11",
                    )
                    .expect("the row");
                let mut meta = write.open_table(META).expect("the table");
                meta.insert(SCHEMA_KEY, SCHEMA.to_string().as_str())
                    .expect("the row");
            }
            write.commit().expect("the commit");
        }
        let store = RedbStore::open(&path).expect("the open");
        let ehr = EhrId::new("bd6b1e5a-3b9b-4a4a-9e0b-9f4b3a0c9f11").expect("a legal ehr_id");
        assert_eq!(
            Some(PersonId::new("http://example.org/ns", "p-1").expect("a legal person id")),
            store.person_of(&ehr).expect("the read")
        );
    }
    /// Returns the schema row of the store file at `path`, or `None` when
    /// the file holds no schema table or no row in it.
    fn schema_of(path: &std::path::Path) -> Option<String> {
        use redb::ReadableDatabase;
        let database = redb::Database::open(path).expect("the file");
        let read = database.begin_read().expect("a read");
        let meta = read.open_table(META).ok()?;
        meta.get(SCHEMA_KEY)
            .expect("the row reads")
            .map(|value| value.value().to_owned())
    }

    /// Writes a store file at `path` whose schema row holds `schema`, or
    /// carries no schema row when `schema` is `None`.
    fn file_with_schema(path: &std::path::Path, schema: Option<&str>) {
        let database = redb::Database::create(path).expect("the file");
        let write = database.begin_write().expect("a write");
        {
            let mut patients = write.open_table(PATIENTS).expect("the table");
            patients
                .insert(
                    "http://example.org/ns|p-1",
                    "bd6b1e5a-3b9b-4a4a-9e0b-9f4b3a0c9f11",
                )
                .expect("the row");
            if let Some(schema) = schema {
                let mut meta = write.open_table(META).expect("the table");
                meta.insert(SCHEMA_KEY, schema).expect("the row");
            }
        }
        write.commit().expect("the commit");
    }

    #[test]
    fn a_new_store_is_stamped_with_the_schema_this_build_writes() {
        let directory = tempfile::tempdir().expect("a temporary directory");
        let path = directory.path().join("identity.redb");
        drop(RedbStore::open(&path).expect("the open"));
        assert_eq!(Some(SCHEMA.to_string()), schema_of(&path));
    }

    #[test]
    fn a_store_an_earlier_build_wrote_without_a_schema_is_refused_as_schema_1() {
        let directory = tempfile::tempdir().expect("a temporary directory");
        let path = directory.path().join("identity.redb");
        file_with_schema(&path, None);
        let refused = RedbStore::open(&path).expect_err("schema 1 is refused");
        assert!(
            matches!(refused, StoreError::Schema { ref found, .. } if found == "1"),
            "{refused}"
        );
        assert_eq!(None, schema_of(&path), "a refused file is left as it was");
    }

    #[test]
    fn a_store_in_a_schema_this_build_does_not_read_is_refused_by_name() {
        for found in ["0", "1", "2", "3", "99", "two"] {
            let directory = tempfile::tempdir().expect("a temporary directory");
            let path = directory.path().join("identity.redb");
            file_with_schema(&path, Some(found));
            let refused = RedbStore::open(&path).expect_err("the schema is refused");
            assert!(
                matches!(
                    refused,
                    StoreError::Schema { found: ref held, oldest: 4, newest: 4, .. } if held == found
                ),
                "{refused}"
            );
            assert!(
                refused.to_string().contains("removing the file"),
                "the refusal names the reset: {refused}"
            );
            assert_eq!(
                Some(String::from(found)),
                schema_of(&path),
                "a refused file is left as it was"
            );
        }
    }

    #[test]
    fn a_store_of_the_schema_this_build_writes_opens() {
        let directory = tempfile::tempdir().expect("a temporary directory");
        let path = directory.path().join("identity.redb");
        file_with_schema(&path, Some("4"));
        drop(RedbStore::open(&path).expect("the open"));
        assert_eq!(Some(SCHEMA.to_string()), schema_of(&path));
    }

    #[test]
    fn a_created_binding_reads_back_with_its_parent() {
        let directory = tempfile::tempdir().expect("a temporary directory");
        let path = directory.path().join("identity.redb");
        let store = RedbStore::open(&path).expect("the open");
        let internal = FhirResourceId::new("created-1").expect("a legal FHIR id");
        let binding = crate::facade::identity::record::CompositionBinding {
            ehr_id: String::from("bd6b1e5a-3b9b-4a4a-9e0b-9f4b3a0c9f11"),
            versioned_object_uid: String::from("uid-1"),
            template_id: String::from("ferrobridge.diagnose.v1"),
            resource_type: String::from("Observation"),
            entry_path: String::from("/content[openEHR-EHR-EVALUATION.problem_diagnosis.v1]"),
            split: 0,
            context: String::from("ferrobridge_diagnosis.context"),
            derivation: Some(crate::facade::identity::derive::DERIVATION),
            created: Some(crate::facade::identity::record::CreatedBy {
                parent_type: String::from("Condition"),
                parent_id: String::from("parent-1"),
                mapping: String::from("evidence"),
            }),
        };
        store
            .record_binding("Observation", &internal, &binding)
            .expect("the write");
        drop(store);
        let reopened = RedbStore::open(&path).expect("the reopen");
        assert_eq!(
            Some(binding),
            reopened
                .binding_of("Observation", &internal)
                .expect("the read")
        );
    }
}
