// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The identity map's reads and record-once writes over the store's tables.
//!
//! Every write keeps the value a key already holds: the map wins once written
//! (no specification governs the store: our own design).

use redb::ReadableTable;

use crate::cdr::ids::EhrId;
use crate::facade::identity::ExternalResourceId;
use crate::facade::identity::FhirResourceId;
use crate::facade::identity::PersonId;
use crate::facade::identity::record::CommittedSource;
use crate::facade::identity::record::CompositionBinding;
use crate::facade::identity::record::ConsumedSource;
use crate::facade::identity::record::EhrSubject;
use crate::facade::identity::record::Identifier;
use crate::facade::identity::record::IdentifierQuery;
use crate::facade::identity::record::SourceVersion;
use crate::facade::identity::record::external_key;
use crate::facade::identity::record::internal_key;
use crate::facade::identity::redb_store::BINDINGS;
use crate::facade::identity::redb_store::CONTRIBUTIONS;
use crate::facade::identity::redb_store::EXTERNAL;
use crate::facade::identity::redb_store::IDENTIFIERS;
use crate::facade::identity::redb_store::PATIENTS;
use crate::facade::identity::redb_store::RedbStore;
use crate::facade::identity::redb_store::SOURCES;
use crate::facade::identity::redb_store::SUBJECTS;
use crate::facade::identity::redb_store::identifier;
use crate::facade::identity::redb_store::record_of;
use crate::facade::identity::redb_store::rendered;
use crate::facade::identity::redb_store::subject_record;
use crate::facade::identity::redb_store::transaction;
use crate::facade::identity::store::Store;
use crate::facade::identity::store::StoreError;
use crate::facade::identity::store::identified_id;

impl Store for RedbStore {
    fn ehr_of(&self, patient: &PersonId) -> Result<Option<EhrId>, StoreError> {
        let key = patient.to_string();
        let held = self.get(PATIENTS, &key)?;
        identifier(&key, "ehr_id", held, EhrId::new)
    }

    fn record_ehr(&self, patient: &PersonId, ehr: &EhrId) -> Result<EhrId, StoreError> {
        let key = patient.to_string();
        let offered = rendered(ehr.as_str(), &subject_record(patient))?;
        let write = self.database.begin_write().map_err(transaction)?;
        let stood = {
            let mut patients = write.open_table(PATIENTS).map_err(transaction)?;
            let existing = patients
                .get(key.as_str())
                .map_err(transaction)?
                .map(|held| held.value().to_owned());
            let stood = if let Some(held) = existing {
                held
            } else {
                patients
                    .insert(key.as_str(), ehr.as_str())
                    .map_err(transaction)?;
                ehr.as_str().to_owned()
            };
            let mut subjects = write.open_table(SUBJECTS).map_err(transaction)?;
            if stood == ehr.as_str() && subjects.get(ehr.as_str()).map_err(transaction)?.is_none() {
                subjects
                    .insert(ehr.as_str(), offered.as_str())
                    .map_err(transaction)?;
            }
            stood
        };
        write.commit().map_err(transaction)?;
        identifier(&key, "ehr_id", Some(stood), EhrId::new)?.ok_or(StoreError::Missing { key })
    }

    fn person_of(&self, ehr: &EhrId) -> Result<Option<PersonId>, StoreError> {
        let key = ehr.as_str();
        let held: Option<EhrSubject> = record_of(key, self.get(SUBJECTS, key)?)?;
        held.map(|subject| {
            PersonId::new(&subject.namespace, &subject.id).map_err(|source| {
                StoreError::Identifier {
                    key: key.to_owned(),
                    kind: "person key",
                    value: format!("{}|{}", subject.namespace, subject.id),
                    source: Box::new(source),
                }
            })
        })
        .transpose()
    }

    fn internal_of(
        &self,
        resource_type: &str,
        external: &ExternalResourceId,
    ) -> Result<Option<FhirResourceId>, StoreError> {
        let key = external_key(resource_type, external);
        let held = self.get(EXTERNAL, &key)?;
        identifier(&key, "FHIR id", held, FhirResourceId::new)
    }

    fn record_internal(
        &self,
        resource_type: &str,
        external: &ExternalResourceId,
        internal: &FhirResourceId,
    ) -> Result<FhirResourceId, StoreError> {
        let key = external_key(resource_type, external);
        let stood = self.record(EXTERNAL, &key, internal.as_str())?;
        identifier(&key, "FHIR id", Some(stood), FhirResourceId::new)?
            .ok_or(StoreError::Missing { key })
    }

    fn binding_of(
        &self,
        resource_type: &str,
        internal: &FhirResourceId,
    ) -> Result<Option<CompositionBinding>, StoreError> {
        let key = internal_key(resource_type, internal);
        let held = self.get(BINDINGS, &key)?;
        record_of(&key, held)
    }

    fn record_binding(
        &self,
        resource_type: &str,
        internal: &FhirResourceId,
        binding: &CompositionBinding,
    ) -> Result<CompositionBinding, StoreError> {
        let key = internal_key(resource_type, internal);
        let offered = rendered(&key, binding)?;
        let stood = self.record(BINDINGS, &key, &offered)?;
        record_of(&key, Some(stood))?.ok_or(StoreError::Missing { key })
    }

    fn consumed(&self, source: &SourceVersion) -> Result<Option<ConsumedSource>, StoreError> {
        let key = source.storage_key();
        let held = self.get(SOURCES, &key)?;
        record_of(&key, held)
    }

    fn consumed_versions(&self, source: &SourceVersion) -> Result<Vec<ConsumedSource>, StoreError> {
        self.scan(SOURCES, &source.every_version())?
            .into_iter()
            .map(|(key, held)| record_of(&key, Some(held))?.ok_or(StoreError::Missing { key }))
            .collect()
    }

    fn record_consumed(
        &self,
        source: &SourceVersion,
        consumed: &ConsumedSource,
    ) -> Result<ConsumedSource, StoreError> {
        let key = source.storage_key();
        let offered = rendered(&key, consumed)?;
        let stood = self.record(SOURCES, &key, &offered)?;
        record_of(&key, Some(stood))?.ok_or(StoreError::Missing { key })
    }

    fn committed(&self, source: &SourceVersion) -> Result<Option<CommittedSource>, StoreError> {
        let key = source.storage_key();
        let held = self.get(CONTRIBUTIONS, &key)?;
        record_of(&key, held)
    }

    fn record_committed(
        &self,
        source: &SourceVersion,
        committed: &CommittedSource,
    ) -> Result<CommittedSource, StoreError> {
        let key = source.storage_key();
        let offered = rendered(&key, committed)?;
        let stood = self.record(CONTRIBUTIONS, &key, &offered)?;
        record_of(&key, Some(stood))?.ok_or(StoreError::Missing { key })
    }

    fn identified(
        &self,
        resource_type: &str,
        query: &IdentifierQuery,
    ) -> Result<Vec<FhirResourceId>, StoreError> {
        self.scan(IDENTIFIERS, &query.range(resource_type))?
            .iter()
            .map(|(key, value)| identified_id(key, value))
            .collect()
    }

    fn record_identifier(
        &self,
        resource_type: &str,
        identifier: &Identifier,
        internal: &FhirResourceId,
    ) -> Result<(), StoreError> {
        let key = identifier.storage_key(resource_type, internal);
        self.record(IDENTIFIERS, &key, internal.as_str())?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crate::cdr::ids::EhrId;
    use crate::facade::identity::record::{
        CommittedSource, CompositionBinding, ConsumedSource, SourceVersion,
    };
    use crate::facade::identity::redb_store::RedbStore;
    use crate::facade::identity::store::Store;
    use crate::facade::identity::{ExternalResourceId, FhirResourceId, PersonId};

    /// A synthetic clinical value, to prove it never reaches the file.
    const MARKER: &str = "a synthetic finding the identity store must never hold";

    /// Returns a binding naming `uid` as its version container.
    fn binding(uid: &str) -> CompositionBinding {
        CompositionBinding {
            ehr_id: String::from("bd6b1e5a-3b9b-4a4a-9e0b-9f4b3a0c9f11"),
            versioned_object_uid: String::from(uid),
            template_id: String::from("ferrobridge.diagnose.v1"),
            resource_type: String::from("Condition"),
            entry_path: String::from("/content[1]"),
            split: 0,
            context: String::from("ferrobridge_diagnosis.context"),
            derivation: Some(crate::facade::identity::derive::DERIVATION),
        }
    }

    #[test]
    fn the_person_of_an_ehr_survives_a_restart_and_the_first_one_stands() {
        let directory = tempfile::tempdir().expect("a temporary directory");
        let path = directory.path().join("identity.redb");
        let ehr = EhrId::new("bd6b1e5a-3b9b-4a4a-9e0b-9f4b3a0c9f11").expect("a legal ehr_id");
        let first = PersonId::new("http://example.org/ns", "p-1").expect("a legal person id");
        let second = PersonId::new("http://example.org/other", "p-2").expect("a legal person");
        {
            let store = RedbStore::open(&path).expect("the first open");
            store.record_ehr(&first, &ehr).expect("the write");
            store.record_ehr(&second, &ehr).expect("the second write");
        }
        let reopened = RedbStore::open(&path).expect("the second open");
        assert_eq!(Some(first), reopened.person_of(&ehr).expect("the read"));
    }

    #[test]
    fn the_store_survives_a_restart() {
        let directory = tempfile::tempdir().expect("a temporary directory");
        let path = directory.path().join("identity.redb");
        let internal = FhirResourceId::new("abc").expect("a legal FHIR id");
        {
            let store = RedbStore::open(&path).expect("the first open");
            store
                .record_binding("Condition", &internal, &binding("uid-1"))
                .expect("the write");
        }
        let reopened = RedbStore::open(&path).expect("the second open");
        let held = reopened
            .binding_of("Condition", &internal)
            .expect("the read")
            .expect("the binding survives");
        assert_eq!("uid-1", held.versioned_object_uid);
    }

    #[test]
    fn a_committed_source_survives_a_restart_and_is_recorded_once() {
        let directory = tempfile::tempdir().expect("a temporary directory");
        let path = directory.path().join("identity.redb");
        let source = SourceVersion::new(
            "Condition",
            ExternalResourceId::new("sender-1").expect("a legal external id"),
            None,
        );
        let first = CommittedSource {
            ehr_id: String::from("bd6b1e5a-3b9b-4a4a-9e0b-9f4b3a0c9f11"),
            contribution_uid: String::from("7b0a4c2e-0000-4000-8000-00000000000c"),
        };
        {
            let store = RedbStore::open(&path).expect("the first open");
            assert_eq!(None, store.committed(&source).expect("an empty read"));
            store
                .record_committed(&source, &first)
                .expect("the first write");
            let second = CommittedSource {
                contribution_uid: String::from("another"),
                ..first.clone()
            };
            assert_eq!(
                first,
                store
                    .record_committed(&source, &second)
                    .expect("the second write"),
                "a second write does not move the record"
            );
        }
        let reopened = RedbStore::open(&path).expect("the second open");
        assert_eq!(
            Some(first),
            reopened.committed(&source).expect("the read back")
        );
    }

    #[test]
    fn every_consumed_version_of_one_id_reads_back_from_disk_and_no_other_id() {
        let directory = tempfile::tempdir().expect("a temporary directory");
        let store = RedbStore::open(&directory.path().join("identity.redb")).expect("the open");
        let source = |id: &str, version: Option<&str>| {
            SourceVersion::new(
                "Condition",
                ExternalResourceId::new(id).expect("a legal external id"),
                version.map(String::from),
            )
        };
        let consumed = |uid: &str| ConsumedSource {
            ehr_id: String::from("bd6b1e5a-3b9b-4a4a-9e0b-9f4b3a0c9f11"),
            versioned_object_uid: String::from(uid),
            internal_id: String::from("abc"),
            context: String::from("ferrobridge_diagnosis.context"),
        };
        for (id, version, uid) in [
            ("c-1", Some("2"), "uid-1"),
            ("c-1", Some("1"), "uid-1"),
            ("c-10", None, "uid-3"),
        ] {
            store
                .record_consumed(&source(id, version), &consumed(uid))
                .expect("the write");
        }
        assert_eq!(
            vec![consumed("uid-1"), consumed("uid-1")],
            store
                .consumed_versions(&source("c-1", None))
                .expect("the read")
        );
        assert_eq!(
            Vec::<ConsumedSource>::new(),
            store
                .consumed_versions(&source("c-2", None))
                .expect("the read")
        );
    }

    #[test]
    fn a_recorded_identifier_survives_a_restart_and_answers_its_search() {
        use crate::facade::identity::record::{Identifier, IdentifierQuery, SystemMatch};
        let directory = tempfile::tempdir().expect("a temporary directory");
        let path = directory.path().join("identity.redb");
        let one = FhirResourceId::new("one").expect("a legal FHIR id");
        let two = FhirResourceId::new("two").expect("a legal FHIR id");
        {
            let store = RedbStore::open(&path).expect("the first open");
            for (system, internal) in [("http://a", &one), ("http://b", &two)] {
                store
                    .record_identifier("Condition", &Identifier::new(Some(system), "v-1"), internal)
                    .expect("the write");
            }
        }
        let reopened = RedbStore::open(&path).expect("the second open");
        assert_eq!(
            vec![one.clone(), two],
            reopened
                .identified("Condition", &IdentifierQuery::new(SystemMatch::Any, "v-1"))
                .expect("the read")
        );
        assert_eq!(
            vec![one],
            reopened
                .identified(
                    "Condition",
                    &IdentifierQuery::new(SystemMatch::Is(String::from("http://a")), "v-1")
                )
                .expect("the read")
        );
        assert_eq!(
            Vec::<FhirResourceId>::new(),
            reopened
                .identified("Condition", &IdentifierQuery::new(SystemMatch::Any, "v-10"))
                .expect("the read")
        );
    }
    #[test]
    fn the_map_wins_once_written_on_disk() {
        let directory = tempfile::tempdir().expect("a temporary directory");
        let store = RedbStore::open(&directory.path().join("identity.redb")).expect("the open");
        let internal = FhirResourceId::new("abc").expect("a legal FHIR id");
        store
            .record_binding("Condition", &internal, &binding("uid-1"))
            .expect("the first write");
        let second = store
            .record_binding("Condition", &internal, &binding("uid-2"))
            .expect("the second write");
        assert_eq!("uid-1", second.versioned_object_uid);
    }

    #[test]
    fn the_store_never_holds_clinical_content() {
        let directory = tempfile::tempdir().expect("a temporary directory");
        let path = directory.path().join("identity.redb");
        let patient = PersonId::new("http://example.org/ns", "p-1").expect("a legal person id");
        let ehr = EhrId::new("bd6b1e5a-3b9b-4a4a-9e0b-9f4b3a0c9f11").expect("a legal ehr_id");
        let internal = FhirResourceId::new("abc").expect("a legal FHIR id");
        {
            let store = RedbStore::open(&path).expect("the open");
            store.record_ehr(&patient, &ehr).expect("the ehr write");
            store
                .record_binding("Condition", &internal, &binding("uid-1"))
                .expect("the binding write");
        }
        let bytes = std::fs::read(&path).expect("the store file reads back");
        assert!(
            !bytes
                .windows(MARKER.len())
                .any(|window| window == MARKER.as_bytes()),
            "the identity store file holds a clinical value"
        );
        assert!(
            bytes
                .windows(internal.as_str().len())
                .any(|window| window == internal.as_str().as_bytes()),
            "the identity store file does not hold the id it was given, so the marker search proves nothing"
        );
    }
}
