// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The identity map in the state store, which every replica shares.
//!
//! Every write is record-once: one statement inserts with `ON CONFLICT DO
//! NOTHING` and answers the row that stands (`rows`), so two replicas
//! recording one binding agree on the winner, and one committed entry's
//! writes share one transaction. A person's identifier and every
//! `Resource.identifier` value are stored as their HMAC under the lookup key,
//! and the person an EHR was recorded for is sealed, so no identifier value
//! is stored in clear (GDPR Art 5(1)(c) and Art 32,
//! <https://eur-lex.europa.eu/eli/reg/2016/679/oj>). The `identity_meta` row
//! pins the derivation and the lookup key, so a build of another derivation
//! or a changed key refuses the start. The rows no write changes are read
//! through the replica's copy ([`crate::identity::cache`]). No specification
//! governs the storage: our own design.

mod rows;

use std::sync::Arc;

use sqlx::Row as _;
use sqlx::postgres::PgPool;

use crate::cdr::ids::EhrId;
use crate::identity::ExternalResourceId;
use crate::identity::FhirResourceId;
use crate::identity::PersonId;
use crate::identity::cache::{IdentityCopy, Rows, SealedPerson};
use crate::identity::derive::DERIVATION;
use crate::identity::record::CommittedSource;
use crate::identity::record::CompositionBinding;
use crate::identity::record::ConsumedSource;
use crate::identity::record::EhrSubject;
use crate::identity::record::Identifier;
use crate::identity::record::IdentifierQuery;
use crate::identity::record::SourceVersion;
use crate::identity::record::SystemMatch;
use crate::identity::store::EntryWrite;
use crate::identity::store::Store;
use crate::identity::store::StoreError;
use crate::identity::store::identified_id;
use crate::store::listen::Listened;
use crate::store::seal::{KeyId, SealKeys};
use rows::checked;

/// The digest purpose of a person identifier.
const PERSON: &str = "person_ehr.id";

/// The digest purpose of an identifier value.
const IDENTIFIER: &str = "resource_identifier.value";

/// The identity map over the state store's pool.
#[derive(Debug, Clone)]
pub struct StateStore {
    /// The pool.
    pool: PgPool,
    /// The keys persons are sealed and identifiers digested under.
    keys: SealKeys,
    /// The replica's copy of the record-once rows, when it keeps one.
    copy: Option<IdentityCopy>,
}

impl StateStore {
    /// Returns the map over `store`, after checking the derivation and the
    /// lookup key the store recorded against this build's.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError::Unsealed`] when `[state.seal]` is absent,
    /// [`StoreError::Derivation`] or [`StoreError::LookupKey`] when the store
    /// was written by another derivation or under another lookup key, and
    /// [`StoreError::Database`] when a statement fails.
    pub async fn open(store: &crate::store::Store) -> Result<Self, StoreError> {
        let keys = store.seal().cloned().ok_or(StoreError::Unsealed)?;
        let pool = store.pool().clone();
        let fingerprint = keys.lookup().fingerprint();
        let expected = i32::try_from(DERIVATION).map_err(|_overflow| StoreError::Derivation {
            found: i64::from(DERIVATION),
            expected: DERIVATION,
        })?;
        sqlx::query(
            "INSERT INTO identity_meta (derivation, lookup_key) VALUES ($1, $2) \
             ON CONFLICT (singleton) DO NOTHING",
        )
        .bind(expected)
        .bind(&fingerprint)
        .execute(&pool)
        .await?;
        let row = sqlx::query("SELECT derivation, lookup_key FROM identity_meta")
            .fetch_one(&pool)
            .await?;
        let found: i32 = row.try_get("derivation")?;
        if found != expected {
            return Err(StoreError::Derivation {
                found: i64::from(found),
                expected: DERIVATION,
            });
        }
        let pinned: Vec<u8> = row.try_get("lookup_key")?;
        if pinned != fingerprint {
            return Err(StoreError::LookupKey);
        }
        Ok(Self {
            pool,
            keys,
            copy: None,
        })
    }

    /// Returns this map reading the record-once rows through a copy of at
    /// most `entries` entries per table; `0` keeps none.
    #[must_use]
    pub fn with_copy(self, entries: usize) -> Self {
        Self {
            copy: IdentityCopy::new(entries),
            ..self
        }
    }

    /// Returns the copied rows while the copy may serve.
    fn serving(&self) -> Option<&Rows> {
        self.copy.as_ref().and_then(IdentityCopy::serving)
    }

    /// Returns the associated data a Patient party's sealed person is bound
    /// to.
    fn party_context(party: &str) -> Vec<u8> {
        format!("party_person\u{1f}{party}").into_bytes()
    }

    /// Returns the associated data an EHR's sealed person is bound to.
    fn subject_context(ehr: &str) -> Vec<u8> {
        format!("ehr_subject\u{1f}{ehr}").into_bytes()
    }

    /// Returns the sealed person `row` holds in its `key_id` and
    /// `person_sealed` columns, read for `key`; `None` without a row.
    fn sealed_of_row(
        row: Option<sqlx::postgres::PgRow>,
        key: &str,
    ) -> Result<Option<SealedPerson>, StoreError> {
        let Some(row) = row else {
            return Ok(None);
        };
        let key_id: String = row.try_get("key_id")?;
        let sealed: Vec<u8> = row.try_get("person_sealed")?;
        let key_id = checked(key, "sealing key id", key_id, KeyId::new)?;
        Ok(Some(SealedPerson { key_id, sealed }))
    }

    /// Returns the person `person` seals under `context`, read for `key`.
    fn opened(
        &self,
        person: &SealedPerson,
        key: &str,
        context: &[u8],
    ) -> Result<PersonId, StoreError> {
        let plain = self
            .keys
            .open(&person.key_id, context, &person.sealed)
            .map_err(|source| StoreError::Seal { source })?;
        let subject: EhrSubject =
            serde_json::from_slice(&plain).map_err(|source| StoreError::Record {
                key: key.to_owned(),
                source: Box::new(source),
            })?;
        PersonId::new(&subject.namespace, &subject.id).map_err(|source| StoreError::Identifier {
            key: key.to_owned(),
            kind: "person id",
            value: subject.namespace,
            source: Box::new(source),
        })
    }

    /// Returns the key a person's EHR is copied under.
    fn person_key(&self, person: &PersonId) -> (String, Vec<u8>) {
        (
            person.namespace().to_owned(),
            self.keys.lookup().digest(PERSON, person.id()),
        )
    }
}

impl From<sqlx::Error> for StoreError {
    fn from(source: sqlx::Error) -> Self {
        Self::Database {
            source: Box::new(source),
        }
    }
}

/// Returns the key a resource type and an id are copied under.
fn typed(resource_type: &str, id: &str) -> (String, String) {
    (resource_type.to_owned(), id.to_owned())
}

#[async_trait::async_trait]
impl Store for StateStore {
    fn listened(&self) -> Option<Arc<Listened>> {
        self.copy.as_ref().map(IdentityCopy::listened)
    }

    async fn ehr_of(&self, patient: &PersonId) -> Result<Option<EhrId>, StoreError> {
        let key = self.person_key(patient);
        if let Some(found) = self.serving().and_then(|rows| rows.ehrs.get(&key)) {
            return Ok(Some(found));
        }
        let found: Option<String> = sqlx::query_scalar(
            "SELECT ehr_id FROM person_ehr WHERE namespace = $1 AND id_hmac = $2",
        )
        .bind(&key.0)
        .bind(&key.1)
        .fetch_optional(&self.pool)
        .await?;
        let found = found
            .map(|value| checked(patient.namespace(), "ehr_id", value, EhrId::new))
            .transpose()?;
        if let (Some(rows), Some(ehr)) = (self.serving(), &found) {
            rows.ehrs.put(key, ehr.clone());
        }
        Ok(found)
    }

    async fn record_ehr(&self, patient: &PersonId, ehr: &EhrId) -> Result<EhrId, StoreError> {
        let key = self.person_key(patient);
        if let Some(found) = self.serving().and_then(|rows| rows.ehrs.get(&key)) {
            return Ok(found);
        }
        let subject = EhrSubject {
            namespace: patient.namespace().to_owned(),
            id: patient.id().to_owned(),
        };
        let plain = serde_json::to_vec(&subject).map_err(|source| StoreError::Record {
            key: ehr.as_str().to_owned(),
            source: Box::new(source),
        })?;
        let (key_id, sealed) = self
            .keys
            .seal(&Self::subject_context(ehr.as_str()), &plain)
            .map_err(|source| StoreError::Seal { source })?;
        let stood: Option<String> = sqlx::query_scalar(
            "WITH won AS (INSERT INTO person_ehr (namespace, id_hmac, ehr_id) VALUES ($1, $2, $3) \
               ON CONFLICT (namespace, id_hmac) DO NOTHING RETURNING ehr_id), \
             subject AS (INSERT INTO ehr_subject (ehr_id, key_id, person_sealed) \
               SELECT ehr_id, $4, $5 FROM won ON CONFLICT (ehr_id) DO NOTHING) \
             SELECT ehr_id FROM won UNION ALL \
             SELECT ehr_id FROM person_ehr WHERE namespace = $1 AND id_hmac = $2 LIMIT 1",
        )
        .bind(&key.0)
        .bind(&key.1)
        .bind(ehr.as_str())
        .bind(key_id.as_str())
        .bind(&sealed)
        .fetch_optional(&self.pool)
        .await?;
        let stood = match stood {
            Some(value) => Some(checked(patient.namespace(), "ehr_id", value, EhrId::new)?),
            None => self.ehr_of(patient).await?,
        };
        let stood = stood.ok_or_else(|| StoreError::Missing {
            key: patient.namespace().to_owned(),
        })?;
        if let Some(rows) = self.serving() {
            rows.ehrs.put(key, stood.clone());
        }
        Ok(stood)
    }

    async fn person_of(&self, ehr: &EhrId) -> Result<Option<PersonId>, StoreError> {
        let context = Self::subject_context(ehr.as_str());
        let copied = self
            .serving()
            .and_then(|rows| rows.subjects.get(&ehr.as_str().to_owned()));
        if let Some(sealed) = copied {
            return self.opened(&sealed, ehr.as_str(), &context).map(Some);
        }
        let row = sqlx::query("SELECT key_id, person_sealed FROM ehr_subject WHERE ehr_id = $1")
            .bind(ehr.as_str())
            .fetch_optional(&self.pool)
            .await?;
        let sealed = Self::sealed_of_row(row, ehr.as_str())?;
        if let (Some(rows), Some(sealed)) = (self.serving(), &sealed) {
            rows.subjects.put(ehr.as_str().to_owned(), sealed.clone());
        }
        sealed
            .map(|sealed| self.opened(&sealed, ehr.as_str(), &context))
            .transpose()
    }

    async fn person_of_party(&self, party: &str) -> Result<Option<PersonId>, StoreError> {
        let row =
            sqlx::query("SELECT key_id, person_sealed FROM party_person WHERE party_uid = $1")
                .bind(party)
                .fetch_optional(&self.pool)
                .await?;
        Self::sealed_of_row(row, party)?
            .map(|sealed| self.opened(&sealed, party, &Self::party_context(party)))
            .transpose()
    }

    async fn party_of(&self, person: &PersonId) -> Result<Option<String>, StoreError> {
        let digest = self.keys.lookup().digest(PERSON, person.id());
        sqlx::query_scalar(
            "SELECT party_uid FROM party_person WHERE namespace = $1 AND id_hmac = $2",
        )
        .bind(person.namespace())
        .bind(&digest)
        .fetch_optional(&self.pool)
        .await
        .map_err(StoreError::from)
    }

    async fn record_party(&self, party: &str, person: &PersonId) -> Result<(), StoreError> {
        let digest = self.keys.lookup().digest(PERSON, person.id());
        let plain = serde_json::to_vec(&EhrSubject {
            namespace: person.namespace().to_owned(),
            id: person.id().to_owned(),
        })
        .map_err(|source| StoreError::Record {
            key: party.to_owned(),
            source: Box::new(source),
        })?;
        let (key_id, sealed) = self
            .keys
            .seal(&Self::party_context(party), &plain)
            .map_err(|source| StoreError::Seal { source })?;
        sqlx::query(
            "INSERT INTO party_person (party_uid, namespace, id_hmac, key_id, person_sealed) \
             VALUES ($1, $2, $3, $4, $5) ON CONFLICT (party_uid) DO UPDATE SET \
             namespace = EXCLUDED.namespace, id_hmac = EXCLUDED.id_hmac, \
             key_id = EXCLUDED.key_id, person_sealed = EXCLUDED.person_sealed",
        )
        .bind(party)
        .bind(person.namespace())
        .bind(&digest)
        .bind(key_id.as_str())
        .bind(&sealed)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn internal_of(
        &self,
        resource_type: &str,
        external: &ExternalResourceId,
    ) -> Result<Option<FhirResourceId>, StoreError> {
        let key = typed(resource_type, external.as_str());
        if let Some(found) = self.serving().and_then(|rows| rows.internals.get(&key)) {
            return Ok(Some(found));
        }
        let mut connection = self.pool.acquire().await?;
        let found = rows::internal(&mut connection, resource_type, external).await?;
        if let (Some(rows), Some(id)) = (self.serving(), &found) {
            rows.internals.put(key, id.clone());
        }
        Ok(found)
    }

    async fn record_internal(
        &self,
        resource_type: &str,
        external: &ExternalResourceId,
        internal: &FhirResourceId,
    ) -> Result<FhirResourceId, StoreError> {
        let key = typed(resource_type, external.as_str());
        if let Some(found) = self.serving().and_then(|rows| rows.internals.get(&key)) {
            return Ok(found);
        }
        let mut connection = self.pool.acquire().await?;
        let stood =
            rows::record_internal(&mut connection, resource_type, external, internal).await?;
        if let Some(rows) = self.serving() {
            rows.internals.put(key, stood.clone());
        }
        Ok(stood)
    }

    async fn binding_of(
        &self,
        resource_type: &str,
        internal: &FhirResourceId,
    ) -> Result<Option<CompositionBinding>, StoreError> {
        let key = typed(resource_type, internal.as_str());
        if let Some(found) = self.serving().and_then(|rows| rows.bindings.get(&key)) {
            return Ok(Some(found));
        }
        let mut connection = self.pool.acquire().await?;
        let found = rows::binding(&mut connection, resource_type, internal).await?;
        if let (Some(rows), Some(binding)) = (self.serving(), &found) {
            rows.bindings.put(key, binding.clone());
        }
        Ok(found)
    }

    async fn record_binding(
        &self,
        resource_type: &str,
        internal: &FhirResourceId,
        binding: &CompositionBinding,
    ) -> Result<CompositionBinding, StoreError> {
        let key = typed(resource_type, internal.as_str());
        if let Some(found) = self.serving().and_then(|rows| rows.bindings.get(&key)) {
            return Ok(found);
        }
        let mut connection = self.pool.acquire().await?;
        let stood = rows::record_binding(&mut connection, resource_type, internal, binding).await?;
        if let Some(rows) = self.serving() {
            rows.bindings.put(key, stood.clone());
        }
        Ok(stood)
    }

    async fn consumed(&self, source: &SourceVersion) -> Result<Option<ConsumedSource>, StoreError> {
        let mut connection = self.pool.acquire().await?;
        rows::consumed(&mut connection, source).await
    }

    async fn consumed_versions(
        &self,
        source: &SourceVersion,
    ) -> Result<Vec<ConsumedSource>, StoreError> {
        let found = sqlx::query(
            "SELECT ehr_id, versioned_object_uid, internal_id, context FROM consumed_source \
             WHERE resource_type = $1 AND source_id = $2 ORDER BY version_id COLLATE \"C\"",
        )
        .bind(source.resource_type())
        .bind(source.id().as_str())
        .fetch_all(&self.pool)
        .await?;
        found.iter().map(rows::consumed_of_row).collect()
    }

    async fn record_consumed(
        &self,
        source: &SourceVersion,
        consumed: &ConsumedSource,
    ) -> Result<ConsumedSource, StoreError> {
        let mut connection = self.pool.acquire().await?;
        rows::record_consumed(&mut connection, source, consumed).await
    }

    async fn committed(
        &self,
        source: &SourceVersion,
    ) -> Result<Option<CommittedSource>, StoreError> {
        let row = rows::keyed(
            "SELECT ehr_id, contribution_uid FROM committed_source \
             WHERE resource_type = $1 AND source_id = $2 AND version_id = $3",
            source,
        )
        .fetch_optional(&self.pool)
        .await?;
        row.map(|row| rows::committed_of_row(&row)).transpose()
    }

    async fn record_committed(
        &self,
        source: &SourceVersion,
        committed: &CommittedSource,
    ) -> Result<CommittedSource, StoreError> {
        let stood = rows::keyed(
            "WITH won AS (INSERT INTO committed_source (resource_type, source_id, version_id, \
               ehr_id, contribution_uid) VALUES ($1, $2, $3, $4, $5) \
               ON CONFLICT (resource_type, source_id, version_id) DO NOTHING \
               RETURNING ehr_id, contribution_uid) \
             SELECT ehr_id, contribution_uid FROM won UNION ALL \
             SELECT ehr_id, contribution_uid FROM committed_source \
             WHERE resource_type = $1 AND source_id = $2 AND version_id = $3 LIMIT 1",
            source,
        )
        .bind(&committed.ehr_id)
        .bind(&committed.contribution_uid)
        .fetch_optional(&self.pool)
        .await?;
        let stood = match stood {
            Some(row) => Some(rows::committed_of_row(&row)?),
            None => self.committed(source).await?,
        };
        stood.ok_or_else(|| StoreError::Missing {
            key: source.storage_key(),
        })
    }

    async fn identified(
        &self,
        resource_type: &str,
        query: &IdentifierQuery,
    ) -> Result<Vec<FhirResourceId>, StoreError> {
        let digest = self.keys.lookup().digest(IDENTIFIER, query.value());
        // NOTE: no specification governs this: our own design; the order is
        // the system framed by its length, then the logical id, as stored before.
        let (filter, system) = match query.system() {
            SystemMatch::Any => ("", None),
            SystemMatch::Absent => (" AND system = ''", None),
            SystemMatch::Is(system) => (" AND system = $3", Some(system.as_str())),
        };
        let statement = format!(
            "SELECT internal_id FROM resource_identifier WHERE resource_type = $1 \
             AND value_hmac = $2{filter} ORDER BY (octet_length(system)::text || ':' || system) \
             COLLATE \"C\", internal_id COLLATE \"C\""
        );
        let mut select = sqlx::query_scalar::<_, String>(sqlx::AssertSqlSafe(statement))
            .bind(resource_type)
            .bind(&digest);
        if let Some(system) = system {
            select = select.bind(system);
        }
        let found = select.fetch_all(&self.pool).await?;
        found
            .iter()
            .map(|value| identified_id(resource_type, value))
            .collect()
    }

    async fn record_identifier(
        &self,
        resource_type: &str,
        identifier: &Identifier,
        internal: &FhirResourceId,
    ) -> Result<(), StoreError> {
        let digest = self.keys.lookup().digest(IDENTIFIER, identifier.value());
        let mut connection = self.pool.acquire().await?;
        rows::record_identifier(
            &mut connection,
            resource_type,
            (identifier.system().unwrap_or_default(), &digest),
            internal,
        )
        .await
    }

    async fn record_entry(
        &self,
        write: &EntryWrite<'_>,
    ) -> Result<(FhirResourceId, CompositionBinding), StoreError> {
        let internal_key = typed(write.resource_type, write.key.as_str());
        let copied_id = self
            .serving()
            .and_then(|rows| rows.internals.get(&internal_key));
        let copied_binding = copied_id.as_ref().and_then(|id| {
            self.serving()
                .and_then(|rows| rows.bindings.get(&typed(write.resource_type, id.as_str())))
        });
        if let (Some(id), Some(stood)) = (&copied_id, &copied_binding)
            && write.identifiers.is_empty()
            && write.source.is_none()
        {
            return Ok((id.clone(), stood.clone()));
        }
        let systems: Vec<&str> = write
            .identifiers
            .iter()
            .map(|identifier| identifier.system().unwrap_or_default())
            .collect();
        let digests: Vec<Vec<u8>> = write
            .identifiers
            .iter()
            .map(|identifier| self.keys.lookup().digest(IDENTIFIER, identifier.value()))
            .collect();
        let mut connection = self.pool.acquire().await?;
        let recorded = rows::record_entry(&mut connection, write, (&systems, &digests)).await?;
        let (id, stood) = match recorded {
            Some(recorded) => recorded,
            None => rows::record_entry_stepwise(&mut connection, write, &digests).await?,
        };
        if let Some(rows) = self.serving() {
            rows.internals.put(internal_key, id.clone());
            rows.bindings
                .put(typed(write.resource_type, id.as_str()), stood.clone());
        }
        Ok((id, stood))
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::StateStore;
    use super::rows::version_slot;
    use crate::cdr::ids::EhrId;
    use crate::identity::record::{
        CommittedSource, CompositionBinding, ConsumedSource, Identifier, IdentifierQuery,
        SourceVersion, SystemMatch,
    };
    use crate::identity::store::{Store as _, StoreError};
    use crate::identity::{ExternalResourceId, FhirResourceId, PersonId};
    use crate::store::seal::{KeyId, LookupKey, SealKeys, SealingKey};

    /// The base64 of 32 bytes of `A`, made for the tests.
    const KEY: &str = "QUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUE=";

    /// A map over a database no server answers.
    fn unreachable() -> StateStore {
        let key = SealingKey::from_base64(KEY).expect("a 32-byte key");
        StateStore {
            pool: sqlx::postgres::PgPoolOptions::new()
                .acquire_timeout(Duration::from_millis(300))
                .connect_lazy("postgres://bridge@127.0.0.1:1/state?sslmode=disable")
                .expect("a lazy pool"),
            keys: SealKeys::new(
                (KeyId::new("k").expect("an id"), key.clone()),
                Vec::new(),
                LookupKey::new(&key),
            ),
            copy: None,
        }
    }

    /// Whether `outcome` is the database's refusal.
    fn refused<T: std::fmt::Debug>(outcome: &Result<T, StoreError>) -> bool {
        matches!(outcome, Err(StoreError::Database { .. }))
    }

    #[tokio::test]
    async fn an_unreachable_store_refuses_every_call_and_answers_nothing_empty() {
        let map = unreachable();
        let person = PersonId::new("http://example.org/ns", "p-1").expect("a person");
        let ehr = EhrId::new("bd6b1e5a-3b9b-4a4a-9e0b-9f4b3a0c9f11").expect("an ehr_id");
        let external = ExternalResourceId::new("sender-1").expect("an external id");
        let id = FhirResourceId::new("abc").expect("a FHIR id");
        let source = SourceVersion::new("Condition", external.clone(), None);
        let binding: CompositionBinding = serde_json::from_value(serde_json::json!({
            "ehr_id": "e", "versioned_object_uid": "u", "template_id": "t",
            "resource_type": "Condition", "entry_path": "/p", "split": 0,
            "context": "c", "derivation": null, "created": null
        }))
        .expect("a binding");
        let consumed = ConsumedSource {
            ehr_id: String::from("e"),
            versioned_object_uid: String::from("u"),
            internal_id: String::from("abc"),
            context: String::from("c"),
        };
        let committed = CommittedSource {
            ehr_id: String::from("e"),
            contribution_uid: String::from("c"),
        };
        assert!(refused(&map.ehr_of(&person).await));
        assert!(refused(&map.record_ehr(&person, &ehr).await));
        assert!(refused(&map.person_of(&ehr).await));
        assert!(refused(&map.internal_of("Condition", &external).await));
        assert!(refused(
            &map.record_internal("Condition", &external, &id).await
        ));
        assert!(refused(&map.binding_of("Condition", &id).await));
        assert!(refused(
            &map.record_binding("Condition", &id, &binding).await
        ));
        assert!(refused(&map.consumed(&source).await));
        assert!(refused(&map.consumed_versions(&source).await));
        assert!(refused(&map.record_consumed(&source, &consumed).await));
        assert!(refused(&map.committed(&source).await));
        assert!(refused(&map.record_committed(&source, &committed).await));
        for system in [
            SystemMatch::Any,
            SystemMatch::Absent,
            SystemMatch::Is(String::from("urn:s")),
        ] {
            let query = IdentifierQuery::new(system, "v");
            assert!(refused(&map.identified("Patient", &query).await));
        }
        let identifier = Identifier::new(Some("urn:s"), "v");
        assert!(refused(
            &map.record_identifier("Patient", &identifier, &id).await
        ));
    }

    #[test]
    fn a_source_without_a_version_takes_the_empty_slot() {
        let external = ExternalResourceId::new("s").expect("an external id");
        assert_eq!(
            "",
            version_slot(&SourceVersion::new("Condition", external.clone(), None))
        );
        assert_eq!(
            "2",
            version_slot(&SourceVersion::new(
                "Condition",
                external,
                Some(String::from("2"))
            ))
        );
    }
}
