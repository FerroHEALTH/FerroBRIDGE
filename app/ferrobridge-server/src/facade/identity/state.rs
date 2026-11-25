// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The identity map in the state store, which every replica shares.
//!
//! Every write is record-once: one statement inserts with `ON CONFLICT DO
//! NOTHING` and answers the row that stands, read again only when a
//! concurrent writer's row was not yet visible to it, so two replicas
//! recording one binding agree on the winner. A person's identifier and every `Resource.identifier`
//! value are stored as their HMAC under the lookup key, and the person an
//! EHR was recorded for is sealed, so no identifier value is stored in clear
//! (GDPR Art 5(1)(c) and Art 32, <https://eur-lex.europa.eu/eli/reg/2016/679/oj>).
//! The `identity_meta` row pins the derivation and the lookup key, so a
//! build of another derivation or a changed key refuses the start. No
//! specification governs the storage: our own design.

use sqlx::Row as _;
use sqlx::postgres::PgPool;

use crate::cdr::ids::EhrId;
use crate::facade::identity::ExternalResourceId;
use crate::facade::identity::FhirResourceId;
use crate::facade::identity::PersonId;
use crate::facade::identity::derive::DERIVATION;
use crate::facade::identity::record::CommittedSource;
use crate::facade::identity::record::CompositionBinding;
use crate::facade::identity::record::ConsumedSource;
use crate::facade::identity::record::EhrSubject;
use crate::facade::identity::record::Identifier;
use crate::facade::identity::record::IdentifierQuery;
use crate::facade::identity::record::SourceVersion;
use crate::facade::identity::record::SystemMatch;
use crate::facade::identity::store::Store;
use crate::facade::identity::store::StoreError;
use crate::facade::identity::store::identified_id;
use crate::store::seal::{KeyId, SealKeys};

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
        .await
        .map_err(database)?;
        let row = sqlx::query("SELECT derivation, lookup_key FROM identity_meta")
            .fetch_one(&pool)
            .await
            .map_err(database)?;
        let found: i32 = row.try_get("derivation").map_err(database)?;
        if found != expected {
            return Err(StoreError::Derivation {
                found: i64::from(found),
                expected: DERIVATION,
            });
        }
        let pinned: Vec<u8> = row.try_get("lookup_key").map_err(database)?;
        if pinned != fingerprint {
            return Err(StoreError::LookupKey);
        }
        Ok(Self { pool, keys })
    }

    /// Returns the associated data a Patient party's sealed person is bound
    /// to.
    fn party_context(party: &str) -> Vec<u8> {
        format!("party_person\u{1f}{party}").into_bytes()
    }

    /// Returns the person `row` seals under `context`, its `key_id` and
    /// `person_sealed` columns, read for `key`; `None` without a row.
    fn opened_person(
        &self,
        row: Option<sqlx::postgres::PgRow>,
        key: &str,
        context: &[u8],
    ) -> Result<Option<PersonId>, StoreError> {
        let Some(row) = row else {
            return Ok(None);
        };
        let key_id: String = row.try_get("key_id").map_err(database)?;
        let sealed: Vec<u8> = row.try_get("person_sealed").map_err(database)?;
        let key_id = checked(key, "sealing key id", key_id, KeyId::new)?;
        let plain = self
            .keys
            .open(&key_id, context, &sealed)
            .map_err(|source| StoreError::Seal { source })?;
        let subject: EhrSubject =
            serde_json::from_slice(&plain).map_err(|source| StoreError::Record {
                key: key.to_owned(),
                source: Box::new(source),
            })?;
        PersonId::new(&subject.namespace, &subject.id)
            .map(Some)
            .map_err(|source| StoreError::Identifier {
                key: key.to_owned(),
                kind: "person id",
                value: subject.namespace,
                source: Box::new(source),
            })
    }

    /// Returns the associated data an EHR's sealed person is bound to.
    fn subject_context(ehr: &str) -> Vec<u8> {
        format!("ehr_subject\u{1f}{ehr}").into_bytes()
    }
}

/// Returns `error` as the store's.
fn database(error: sqlx::Error) -> StoreError {
    StoreError::Database {
        source: Box::new(error),
    }
}

/// Returns the identifier `value` read under `key` as a `kind`.
fn checked<T, E: std::error::Error + Send + Sync + 'static>(
    key: &str,
    kind: &'static str,
    value: String,
    parse: impl FnOnce(&str) -> Result<T, E>,
) -> Result<T, StoreError> {
    parse(&value).map_err(|source| StoreError::Identifier {
        key: key.to_owned(),
        kind,
        value,
        source: Box::new(source),
    })
}

/// Returns the binding a row holds.
fn binding_of_row(key: &str, value: serde_json::Value) -> Result<CompositionBinding, StoreError> {
    serde_json::from_value(value).map_err(|source| StoreError::Record {
        key: key.to_owned(),
        source: Box::new(source),
    })
}

/// Returns the version slot of `source`: its `meta.versionId`, or the empty
/// string for none, which sorts before every version as the old keys did.
fn version_slot(source: &SourceVersion) -> &str {
    source.version_id().unwrap_or_default()
}

#[async_trait::async_trait]
impl Store for StateStore {
    async fn ehr_of(&self, patient: &PersonId) -> Result<Option<EhrId>, StoreError> {
        let digest = self.keys.lookup().digest(PERSON, patient.id());
        let found: Option<String> = sqlx::query_scalar(
            "SELECT ehr_id FROM person_ehr WHERE namespace = $1 AND id_hmac = $2",
        )
        .bind(patient.namespace())
        .bind(&digest)
        .fetch_optional(&self.pool)
        .await
        .map_err(database)?;
        found
            .map(|value| checked(patient.namespace(), "ehr_id", value, EhrId::new))
            .transpose()
    }

    async fn record_ehr(&self, patient: &PersonId, ehr: &EhrId) -> Result<EhrId, StoreError> {
        let digest = self.keys.lookup().digest(PERSON, patient.id());
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
        .bind(patient.namespace())
        .bind(&digest)
        .bind(ehr.as_str())
        .bind(key_id.as_str())
        .bind(&sealed)
        .fetch_optional(&self.pool)
        .await
        .map_err(database)?;
        let stood = match stood {
            Some(value) => Some(checked(patient.namespace(), "ehr_id", value, EhrId::new)?),
            None => self.ehr_of(patient).await?,
        };
        stood.ok_or_else(|| StoreError::Missing {
            key: patient.namespace().to_owned(),
        })
    }

    async fn person_of(&self, ehr: &EhrId) -> Result<Option<PersonId>, StoreError> {
        let row = sqlx::query("SELECT key_id, person_sealed FROM ehr_subject WHERE ehr_id = $1")
            .bind(ehr.as_str())
            .fetch_optional(&self.pool)
            .await
            .map_err(database)?;
        self.opened_person(row, ehr.as_str(), &Self::subject_context(ehr.as_str()))
    }

    async fn person_of_party(&self, party: &str) -> Result<Option<PersonId>, StoreError> {
        let row =
            sqlx::query("SELECT key_id, person_sealed FROM party_person WHERE party_uid = $1")
                .bind(party)
                .fetch_optional(&self.pool)
                .await
                .map_err(database)?;
        self.opened_person(row, party, &Self::party_context(party))
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
        .map_err(database)
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
        .await
        .map_err(database)?;
        Ok(())
    }

    async fn internal_of(
        &self,
        resource_type: &str,
        external: &ExternalResourceId,
    ) -> Result<Option<FhirResourceId>, StoreError> {
        let found: Option<String> = sqlx::query_scalar(
            "SELECT internal_id FROM external_internal WHERE resource_type = $1 AND external_id = $2",
        )
        .bind(resource_type)
        .bind(external.as_str())
        .fetch_optional(&self.pool)
        .await
        .map_err(database)?;
        found
            .map(|value| checked(external.as_str(), "FHIR id", value, FhirResourceId::new))
            .transpose()
    }

    async fn record_internal(
        &self,
        resource_type: &str,
        external: &ExternalResourceId,
        internal: &FhirResourceId,
    ) -> Result<FhirResourceId, StoreError> {
        let stood: Option<String> = sqlx::query_scalar(
            "WITH won AS (INSERT INTO external_internal (resource_type, external_id, internal_id) \
               VALUES ($1, $2, $3) ON CONFLICT (resource_type, external_id) DO NOTHING \
               RETURNING internal_id) \
             SELECT internal_id FROM won UNION ALL SELECT internal_id FROM external_internal \
             WHERE resource_type = $1 AND external_id = $2 LIMIT 1",
        )
        .bind(resource_type)
        .bind(external.as_str())
        .bind(internal.as_str())
        .fetch_optional(&self.pool)
        .await
        .map_err(database)?;
        let stood = match stood {
            Some(value) => Some(checked(
                external.as_str(),
                "FHIR id",
                value,
                FhirResourceId::new,
            )?),
            None => self.internal_of(resource_type, external).await?,
        };
        stood.ok_or_else(|| StoreError::Missing {
            key: external.as_str().to_owned(),
        })
    }

    async fn binding_of(
        &self,
        resource_type: &str,
        internal: &FhirResourceId,
    ) -> Result<Option<CompositionBinding>, StoreError> {
        let found: Option<serde_json::Value> = sqlx::query_scalar(
            "SELECT binding FROM composition_binding WHERE resource_type = $1 AND internal_id = $2",
        )
        .bind(resource_type)
        .bind(internal.as_str())
        .fetch_optional(&self.pool)
        .await
        .map_err(database)?;
        found
            .map(|value| binding_of_row(internal.as_str(), value))
            .transpose()
    }

    async fn record_binding(
        &self,
        resource_type: &str,
        internal: &FhirResourceId,
        binding: &CompositionBinding,
    ) -> Result<CompositionBinding, StoreError> {
        let value = serde_json::to_value(binding).map_err(|source| StoreError::Record {
            key: internal.as_str().to_owned(),
            source: Box::new(source),
        })?;
        let stood: Option<serde_json::Value> = sqlx::query_scalar(
            "WITH won AS (INSERT INTO composition_binding (resource_type, internal_id, binding) \
               VALUES ($1, $2, $3) ON CONFLICT (resource_type, internal_id) DO NOTHING \
               RETURNING binding) \
             SELECT binding FROM won UNION ALL SELECT binding FROM composition_binding \
             WHERE resource_type = $1 AND internal_id = $2 LIMIT 1",
        )
        .bind(resource_type)
        .bind(internal.as_str())
        .bind(&value)
        .fetch_optional(&self.pool)
        .await
        .map_err(database)?;
        let stood = match stood {
            Some(value) => Some(binding_of_row(internal.as_str(), value)?),
            None => self.binding_of(resource_type, internal).await?,
        };
        stood.ok_or_else(|| StoreError::Missing {
            key: internal.as_str().to_owned(),
        })
    }

    async fn consumed(&self, source: &SourceVersion) -> Result<Option<ConsumedSource>, StoreError> {
        let row = keyed(
            "SELECT ehr_id, versioned_object_uid, internal_id, context FROM consumed_source \
             WHERE resource_type = $1 AND source_id = $2 AND version_id = $3",
            source,
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(database)?;
        row.map(|row| consumed_of_row(&row)).transpose()
    }

    async fn consumed_versions(
        &self,
        source: &SourceVersion,
    ) -> Result<Vec<ConsumedSource>, StoreError> {
        let rows = sqlx::query(
            "SELECT ehr_id, versioned_object_uid, internal_id, context FROM consumed_source \
             WHERE resource_type = $1 AND source_id = $2 ORDER BY version_id COLLATE \"C\"",
        )
        .bind(source.resource_type())
        .bind(source.id().as_str())
        .fetch_all(&self.pool)
        .await
        .map_err(database)?;
        rows.iter().map(consumed_of_row).collect()
    }

    async fn record_consumed(
        &self,
        source: &SourceVersion,
        consumed: &ConsumedSource,
    ) -> Result<ConsumedSource, StoreError> {
        let stood = keyed(
            "WITH won AS (INSERT INTO consumed_source (resource_type, source_id, version_id, \
               ehr_id, versioned_object_uid, internal_id, context) \
               VALUES ($1, $2, $3, $4, $5, $6, $7) \
               ON CONFLICT (resource_type, source_id, version_id) DO NOTHING \
               RETURNING ehr_id, versioned_object_uid, internal_id, context) \
             SELECT ehr_id, versioned_object_uid, internal_id, context FROM won UNION ALL \
             SELECT ehr_id, versioned_object_uid, internal_id, context FROM consumed_source \
             WHERE resource_type = $1 AND source_id = $2 AND version_id = $3 LIMIT 1",
            source,
        )
        .bind(&consumed.ehr_id)
        .bind(&consumed.versioned_object_uid)
        .bind(&consumed.internal_id)
        .bind(&consumed.context)
        .fetch_optional(&self.pool)
        .await
        .map_err(database)?;
        let stood = match stood {
            Some(row) => Some(consumed_of_row(&row)?),
            None => self.consumed(source).await?,
        };
        stood.ok_or_else(|| StoreError::Missing {
            key: source.storage_key(),
        })
    }

    async fn committed(
        &self,
        source: &SourceVersion,
    ) -> Result<Option<CommittedSource>, StoreError> {
        let row = keyed(
            "SELECT ehr_id, contribution_uid FROM committed_source \
             WHERE resource_type = $1 AND source_id = $2 AND version_id = $3",
            source,
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(database)?;
        row.map(|row| committed_of_row(&row)).transpose()
    }

    async fn record_committed(
        &self,
        source: &SourceVersion,
        committed: &CommittedSource,
    ) -> Result<CommittedSource, StoreError> {
        let stood = keyed(
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
        .await
        .map_err(database)?;
        let stood = match stood {
            Some(row) => Some(committed_of_row(&row)?),
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
        let rows = select.fetch_all(&self.pool).await.map_err(database)?;
        rows.iter()
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
        sqlx::query(
            "INSERT INTO resource_identifier (resource_type, system, value_hmac, internal_id) \
             VALUES ($1, $2, $3, $4) ON CONFLICT DO NOTHING",
        )
        .bind(resource_type)
        .bind(identifier.system().unwrap_or_default())
        .bind(&digest)
        .bind(internal.as_str())
        .execute(&self.pool)
        .await
        .map_err(database)?;
        Ok(())
    }
}

/// Returns the statement `sql` with the three keys of `source` bound as its
/// first three parameters.
fn keyed<'q>(
    sql: &'static str,
    source: &'q SourceVersion,
) -> sqlx::query::Query<'q, sqlx::Postgres, sqlx::postgres::PgArguments> {
    sqlx::query(sql)
        .bind(source.resource_type())
        .bind(source.id().as_str())
        .bind(version_slot(source))
}

/// Returns the committed record a row holds.
fn committed_of_row(row: &sqlx::postgres::PgRow) -> Result<CommittedSource, StoreError> {
    Ok(CommittedSource {
        ehr_id: row.try_get("ehr_id").map_err(database)?,
        contribution_uid: row.try_get("contribution_uid").map_err(database)?,
    })
}

/// Returns the consumed record a row holds.
fn consumed_of_row(row: &sqlx::postgres::PgRow) -> Result<ConsumedSource, StoreError> {
    Ok(ConsumedSource {
        ehr_id: row.try_get("ehr_id").map_err(database)?,
        versioned_object_uid: row.try_get("versioned_object_uid").map_err(database)?,
        internal_id: row.try_get("internal_id").map_err(database)?,
        context: row.try_get("context").map_err(database)?,
    })
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::{StateStore, version_slot};
    use crate::cdr::ids::EhrId;
    use crate::facade::identity::record::{
        CommittedSource, CompositionBinding, ConsumedSource, Identifier, IdentifierQuery,
        SourceVersion, SystemMatch,
    };
    use crate::facade::identity::store::{Store as _, StoreError};
    use crate::facade::identity::{ExternalResourceId, FhirResourceId, PersonId};
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
