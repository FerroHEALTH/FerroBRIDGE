// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The runtime settings registers: operational policy the operator changes
//! without a restart, versioned in the state store.
//!
//! Every write appends one version naming the version it replaces, under a
//! transaction-level advisory lock on the register, so a writer that read an
//! older version is refused (RFC 9110 §13.1.1). The write notifies
//! [`CHANNEL`] before it commits; each replica keeps the newest version of
//! each register in memory, drops a register its listener hears of, and
//! reads the store on every request while the listener is down. A register
//! with no version holds its default. No specification governs the register
//! mechanics: our own design on family precedent.

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, PoisonError, RwLock};

use sqlx::Row as _;
use sqlx::postgres::{PgPool, PgRow};

use crate::store::listen::{Dropping, Listened, Subscriber};

/// The channel a register write is notified on, its payload the register's
/// name.
pub const CHANNEL: &str = "ferrobridge_settings";

/// A runtime settings register.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Register {
    /// The category of the data under each template and archetype, Regulation
    /// (EU) 2025/327 Annex II 3.2(c).
    AuditCategories,
}

impl Register {
    /// Every register.
    pub const ALL: [Self; 1] = [Self::AuditCategories];

    /// Returns the name the API and the store know the register by.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::AuditCategories => "audit-categories",
        }
    }

    /// Returns the register `name` names, or `None` for any other name.
    #[must_use]
    pub fn of_name(name: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|register| register.name() == name)
    }

    /// Returns the document the register holds while no version exists.
    #[must_use]
    pub fn default_document(self) -> serde_json::Value {
        match self {
            Self::AuditCategories => serde_json::json!({
                "national": [],
                "templates": {},
                "archetypes": {},
            }),
        }
    }

    /// Checks `document` against the register's shape and rules.
    ///
    /// # Errors
    ///
    /// Returns [`RegisterError::Shape`] for a document of another shape and
    /// [`RegisterError::Categories`] for a category map that does not hold.
    pub fn check(self, document: &serde_json::Value) -> Result<(), RegisterError> {
        match self {
            Self::AuditCategories => category_map(document).map(|_map| ()),
        }
    }
}

/// The `audit-categories` document (Annex II 3.2(c)): each template id and
/// archetype id to its Art 14(1) categories, or to `none`.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct CategoryDocument {
    /// The national categories the map may name, each `<system>|<code>`.
    pub national: Vec<String>,
    /// The categories of the data under each template id.
    pub templates: BTreeMap<String, ehds_logging::map::Declared>,
    /// The categories of the data under each archetype id.
    pub archetypes: BTreeMap<String, ehds_logging::map::Declared>,
}

/// Returns the category map an `audit-categories` document declares.
fn category_map(
    document: &serde_json::Value,
) -> Result<ehds_logging::map::CategoryMap, RegisterError> {
    let categories: CategoryDocument =
        serde_json::from_value(document.clone()).map_err(RegisterError::Shape)?;
    // NOTE: openEHR BASE Release 1.3.0, Identification Package: template and archetype
    // ids compare case-insensitively.
    ehds_logging::map::CategoryMap::declare_with(
        ehds_logging::map::Matching::OpenEhr,
        &categories.national,
        &categories.templates,
        &categories.archetypes,
    )
    .map_err(RegisterError::Categories)
}

/// The category map of one `audit-categories` version, named by that version
/// so every record classified under it names the revision.
#[derive(Debug)]
pub struct Categories {
    /// The version it was compiled from.
    number: i64,
    /// The map, its digest `audit-categories/<number>`.
    map: ehds_logging::map::CategoryMap,
}

impl Categories {
    /// Compiles the category map of `version` of `audit-categories`.
    ///
    /// # Errors
    ///
    /// Returns the check's error for a document that does not hold.
    pub fn compile(version: &Version) -> Result<Self, RegisterError> {
        // NOTE: no specification governs this: our own design; the digest names the
        // register revision, so a record classified during an outage can be re-checked.
        let map = category_map(&version.document)?.with_digest(format!(
            "{}/{}",
            Register::AuditCategories.name(),
            version.number
        ));
        Ok(Self {
            number: version.number,
            map,
        })
    }

    /// Returns the version the map was compiled from.
    #[must_use]
    pub const fn number(&self) -> i64 {
        self.number
    }

    /// Returns the map.
    #[must_use]
    pub const fn map(&self) -> &ehds_logging::map::CategoryMap {
        &self.map
    }
}

/// One version of a register.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Version {
    /// The version number; `0` is the default, which no one committed.
    pub number: i64,
    /// The document.
    pub document: serde_json::Value,
    /// Who committed it; `None` for the default.
    pub committer: Option<String>,
    /// When it was committed, RFC 3339 UTC; `None` for the default.
    pub committed_at: Option<String>,
}

impl Version {
    /// Returns the default version of `register`.
    #[must_use]
    pub fn default_of(register: Register) -> Self {
        Self {
            number: 0,
            document: register.default_document(),
            committer: None,
            committed_at: None,
        }
    }
}

/// How a write ended, short of a failure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Written {
    /// The version was committed.
    Committed(Version),
    /// The version named is not the newest; nothing was written.
    Stale {
        /// The newest version.
        current: i64,
    },
}

/// Why a register could not be read or written.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum RegisterError {
    /// The state store refused or failed a statement.
    #[error("a settings register statement failed")]
    Database(#[source] Box<sqlx::Error>),
    /// The document is not the register's shape.
    #[error("the document is not the register's shape")]
    Shape(#[source] serde_json::Error),
    /// The category map does not compile.
    #[error("the category map does not hold")]
    Categories(#[source] ehds_logging::map::MapErrors),
    /// A stored document is not JSON this version reads.
    #[error("the register {register} holds version {version} this version cannot read")]
    Record {
        /// The register.
        register: &'static str,
        /// The version.
        version: i64,
    },
}

impl From<sqlx::Error> for RegisterError {
    fn from(source: sqlx::Error) -> Self {
        Self::Database(Box::new(source))
    }
}

/// The registers of one state store, with this replica's copy of each.
#[derive(Debug)]
pub struct Registers {
    /// The store's pool.
    pool: PgPool,
    /// The listener state of [`CHANNEL`].
    listened: Arc<Listened>,
    /// The copy of the newest version of each register.
    copy: Arc<Copy>,
}

/// This replica's copy of the newest version of each register.
#[derive(Debug, Default)]
struct Copy {
    /// The newest version of each register read since the last change.
    versions: RwLock<BTreeMap<Register, Arc<Version>>>,
    /// Moves on at every drop, so a read that raced one is not kept.
    generation: AtomicU64,
}

impl Copy {
    /// Drops `register`, or every register for `None`.
    fn drop_register(&self, register: Option<Register>) {
        let mut versions = self
            .versions
            .write()
            .unwrap_or_else(PoisonError::into_inner);
        match register {
            Some(register) => {
                versions.remove(&register);
            }
            None => versions.clear(),
        }
        self.generation.fetch_add(1, Ordering::AcqRel);
    }
}

impl Subscriber for Copy {
    fn notified<'a>(&'a self, payload: &'a str) -> Dropping<'a> {
        Box::pin(async move {
            // NOTE: no specification governs this: our own design; a payload that names
            // no register drops every copy, so an unknown name never leaves one stale.
            self.drop_register(Register::of_name(payload));
        })
    }

    fn reset(&self) -> Dropping<'_> {
        Box::pin(async move { self.drop_register(None) })
    }
}

impl Registers {
    /// Returns the registers the state store `store` keeps.
    #[must_use]
    pub fn new(store: &crate::store::Store) -> Self {
        let copy = Arc::new(Copy::default());
        Self {
            pool: store.pool().clone(),
            listened: Arc::new(Listened::with_subscriber(CHANNEL, copy.clone())),
            copy,
        }
    }

    /// Returns the listener state of [`CHANNEL`], for the replica's `LISTEN`
    /// session.
    #[must_use]
    pub fn listened(&self) -> Arc<Listened> {
        Arc::clone(&self.listened)
    }

    /// Returns the version of `register` in force: this replica's copy while
    /// the listener holds it, the store's newest version otherwise.
    ///
    /// # Errors
    ///
    /// Returns [`RegisterError::Database`] when the store does not answer and
    /// [`RegisterError::Record`] for a version this version cannot read.
    pub async fn current(&self, register: Register) -> Result<Arc<Version>, RegisterError> {
        let listening = self.listened.is_listening();
        if listening
            && let Some(version) = self
                .copy
                .versions
                .read()
                .unwrap_or_else(PoisonError::into_inner)
                .get(&register)
        {
            return Ok(Arc::clone(version));
        }
        let generation = self.copy.generation.load(Ordering::Acquire);
        let version = Arc::new(self.newest(register).await?);
        if listening {
            let mut versions = self
                .copy
                .versions
                .write()
                .unwrap_or_else(PoisonError::into_inner);
            if self.copy.generation.load(Ordering::Acquire) == generation {
                versions.insert(register, Arc::clone(&version));
            }
        }
        Ok(version)
    }

    /// Returns the newest version of `register` in the store, its default
    /// when none exists.
    async fn newest(&self, register: Register) -> Result<Version, RegisterError> {
        let row = sqlx::query(
            "SELECT version, document::text AS document, committer, \
             to_char(committed_at AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"') AS committed_at \
             FROM settings_register WHERE register = $1 ORDER BY version DESC LIMIT 1",
        )
        .bind(register.name())
        .fetch_optional(&self.pool)
        .await
        ?;
        row.map_or_else(
            || Ok(Version::default_of(register)),
            |row| version(register, &row),
        )
    }

    /// Returns every committed version of `register`, newest first.
    ///
    /// # Errors
    ///
    /// Returns what [`Self::current`] returns.
    pub async fn history(&self, register: Register) -> Result<Vec<Version>, RegisterError> {
        let rows = sqlx::query(
            "SELECT version, document::text AS document, committer, \
             to_char(committed_at AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"') AS committed_at \
             FROM settings_register WHERE register = $1 ORDER BY version DESC",
        )
        .bind(register.name())
        .fetch_all(&self.pool)
        .await
        ?;
        rows.iter().map(|row| version(register, row)).collect()
    }

    /// Appends `document` as the version after `replaces`, committed by
    /// `committer`, when `replaces` is the newest version, and notifies
    /// every replica; the document is checked first.
    ///
    /// # Errors
    ///
    /// Returns the check's error for a document that does not hold, and
    /// [`RegisterError::Database`] when a statement or the commit fails.
    pub async fn write(
        &self,
        register: Register,
        replaces: i64,
        document: &serde_json::Value,
        committer: &str,
    ) -> Result<Written, RegisterError> {
        register.check(document)?;
        let mut transaction = self.pool.begin().await?;
        sqlx::query("SELECT pg_advisory_xact_lock(hashtext('ferrobridge_settings:' || $1))")
            .bind(register.name())
            .execute(&mut *transaction)
            .await?;
        let current: i64 = sqlx::query_scalar(
            "SELECT coalesce(max(version), 0) FROM settings_register WHERE register = $1",
        )
        .bind(register.name())
        .fetch_one(&mut *transaction)
        .await?;
        if current != replaces {
            return Ok(Written::Stale { current });
        }
        let row = sqlx::query(
            "INSERT INTO settings_register (register, version, document, committer) \
             VALUES ($1, $2, $3::jsonb, $4) \
             RETURNING version, document::text AS document, committer, \
             to_char(committed_at AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"') AS committed_at",
        )
        .bind(register.name())
        .bind(current.saturating_add(1))
        .bind(document.to_string())
        .bind(committer)
        .fetch_one(&mut *transaction)
        .await
        ?;
        sqlx::query("SELECT pg_notify($1, $2)")
            .bind(CHANNEL)
            .bind(register.name())
            .execute(&mut *transaction)
            .await?;
        transaction.commit().await?;
        let recorded = version(register, &row)?;
        self.copy.drop_register(Some(register));
        Ok(Written::Committed(recorded))
    }
}

/// Returns the version of `register` a `settings_register` row holds.
fn version(register: Register, row: &PgRow) -> Result<Version, RegisterError> {
    let number: i64 = row.try_get("version")?;
    let document: String = row.try_get("document")?;
    Ok(Version {
        number,
        document: serde_json::from_str(&document).map_err(|_unreadable| RegisterError::Record {
            register: register.name(),
            version: number,
        })?,
        committer: Some(row.try_get("committer")?),
        committed_at: Some(row.try_get("committed_at")?),
    })
}

#[cfg(test)]
mod tests {
    use super::{Register, RegisterError};

    #[test]
    fn every_register_is_known_by_its_name_and_its_default_holds() {
        for register in Register::ALL {
            assert_eq!(Some(register), Register::of_name(register.name()));
            register
                .check(&register.default_document())
                .expect("the default holds");
        }
        assert_eq!(None, Register::of_name("access-delay"));
    }

    #[test]
    fn a_category_naming_no_art_14_category_is_refused() {
        let refused = Register::AuditCategories.check(&serde_json::json!({
            "templates": {"Synthetic.v1": ["No-Such-Category"]},
        }));
        assert!(
            matches!(refused, Err(RegisterError::Categories(_))),
            "{refused:?}"
        );
    }

    #[test]
    fn a_compiled_map_names_its_register_revision() {
        let version = super::Version {
            number: 3,
            document: Register::AuditCategories.default_document(),
            committer: None,
            committed_at: None,
        };
        let compiled = super::Categories::compile(&version).expect("the default compiles");
        assert_eq!(Some("audit-categories/3"), compiled.map().digest());
        assert_eq!(3, compiled.number());
    }

    #[test]
    fn a_category_map_of_another_shape_is_refused() {
        let refused = Register::AuditCategories.check(&serde_json::json!({"shipped": true}));
        assert!(
            matches!(refused, Err(RegisterError::Shape(_))),
            "{refused:?}"
        );
    }
}
