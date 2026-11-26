// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The inbound sources: versioned rows in the state store, written through
//! the operator API, and each replica's snapshot of them.
//!
//! An operator adds, changes, disables or deletes a source without a restart
//! and without a new port ([`store`]). Each write notifies [`SOURCES`]
//! when it commits; each replica's `LISTEN` session
//! reloads the source a notification names into its snapshot behind one
//! pointer swap, and reloads every source when it connects again, and while
//! it is disconnected every lookup reads its row. A document names its
//! secrets ([`document`]); they resolve through the `[secrets]` directory
//! when used, so a write naming a secret the directory does not hold is
//! refused, and a secret gone at runtime fails closed. No specification
//! governs a bridge's source registry: our own design, on the family's
//! register and listener precedent.

pub mod document;
pub mod store;

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, PoisonError, RwLock};

use ferrobridge_operator_api::sources::Kind;
use ferrohealth_secret::dir::SecretDir;
use secrecy::SecretString;
use uuid::Uuid;

use crate::store::listen::{Dropping, Listened, SOURCES, Subscriber};
use document::{Document, Invalid, SecretRef};
use store::SourceStore;

/// One source at its head version.
#[derive(Debug, Clone)]
pub struct Source {
    /// The id.
    pub id: Uuid,
    /// The name, unique.
    pub name: String,
    /// The kind.
    pub kind: Kind,
    /// Whether it is served.
    pub enabled: bool,
    /// The head version.
    pub version: i64,
    /// The document as it was written.
    pub document: serde_json::Value,
    /// The document, read.
    pub parsed: Document,
    /// Who wrote the head version.
    pub updated_by: String,
    /// When, RFC 3339 UTC.
    pub updated_at: String,
}

impl Source {
    /// Returns the source `write` describes at version 1, its document read,
    /// as a fixed route table holds it.
    ///
    /// # Errors
    /// Returns [`Invalid`] when the document is no document of its kind.
    pub fn fixed(write: &ferrobridge_operator_api::sources::Write) -> Result<Self, Invalid> {
        Ok(Self {
            id: Uuid::new_v4(),
            name: write.name.clone(),
            kind: write.kind,
            enabled: write.enabled,
            version: 1,
            document: write.document.clone(),
            parsed: Document::read(write.kind, &write.document)?,
            updated_by: String::new(),
            updated_at: String::new(),
        })
    }

    /// Returns the source as the operator API writes it.
    #[must_use]
    pub fn document(&self) -> ferrobridge_operator_api::sources::Source {
        ferrobridge_operator_api::sources::Source {
            id: self.id.to_string(),
            name: self.name.clone(),
            kind: self.kind,
            enabled: self.enabled,
            version: self.version.cast_unsigned(),
            document: self.document.clone(),
            updated_by: self.updated_by.clone(),
            updated_at: self.updated_at.clone(),
        }
    }
}

/// A secret a source names that cannot be read now: the source fails closed.
#[derive(Debug, thiserror::Error)]
#[error("the secret {name} of source {source_name} cannot be read")]
pub struct SecretUnavailable {
    /// The source.
    pub source_name: String,
    /// The secret's name, never its value.
    pub name: String,
    /// Why.
    #[source]
    pub cause: Option<ferrohealth_secret::dir::SecretDirError>,
}

impl axum::response::IntoResponse for SecretUnavailable {
    fn into_response(self) -> axum::response::Response {
        tracing::error!(source = %self.source_name, secret = %self.name, "a secret a source names cannot be read; the source fails closed");
        crate::problem::refuse(
            crate::problem::Kind::SourcesUnavailable,
            "a secret the source names cannot be read",
        )
    }
}

/// What a replica holds of every source.
#[derive(Debug, Default)]
struct Snapshot {
    /// The sources by id.
    by_id: BTreeMap<Uuid, Arc<Source>>,
}

impl Snapshot {
    /// Returns the snapshot of `sources`.
    fn of(sources: impl IntoIterator<Item = Source>) -> Self {
        Self {
            by_id: sources
                .into_iter()
                .map(|source| (source.id, Arc::new(source)))
                .collect(),
        }
    }

    /// Returns the source named `name`.
    fn named(&self, name: &str) -> Option<Arc<Source>> {
        self.by_id
            .values()
            .find(|source| source.name == name)
            .cloned()
    }

    /// Returns the source the MLLP sender `application` at `facility` is
    /// routed to.
    fn routed(&self, application: &str, facility: &str) -> Option<Arc<Source>> {
        self.by_id
            .values()
            .find(|source| {
                source.parsed.routes().iter().any(|route| {
                    route.sending_application == application && route.sending_facility == facility
                })
            })
            .cloned()
    }
}

/// The snapshot and the store it is read from, which the listener reloads.
#[derive(Debug)]
struct Held {
    /// The rows.
    store: SourceStore,
    /// The snapshot, swapped whole.
    snapshot: RwLock<Arc<Snapshot>>,
    /// Whether the snapshot holds every write the listener heard of; a failed
    /// reload sends every lookup to the store until the next one succeeds.
    whole: AtomicBool,
}

impl Held {
    /// Returns the current snapshot.
    fn current(&self) -> Arc<Snapshot> {
        Arc::clone(&self.snapshot.read().unwrap_or_else(PoisonError::into_inner))
    }

    /// Swaps in `snapshot`.
    fn swap(&self, snapshot: Snapshot) {
        *self
            .snapshot
            .write()
            .unwrap_or_else(PoisonError::into_inner) = Arc::new(snapshot);
    }

    /// Reloads the source `id` into the snapshot.
    async fn reload(&self, id: Uuid) -> Result<(), sqlx::Error> {
        let loaded = self.store.get(id).await?;
        let mut sources: BTreeMap<Uuid, Arc<Source>> = self.current().by_id.clone();
        match loaded {
            Some(source) => {
                sources.insert(id, Arc::new(source));
            }
            None => {
                sources.remove(&id);
            }
        }
        self.swap(Snapshot { by_id: sources });
        Ok(())
    }

    /// Reloads every source into the snapshot.
    async fn reload_all(&self) -> Result<(), sqlx::Error> {
        let sources = self.store.list().await?;
        self.swap(Snapshot::of(sources));
        Ok(())
    }
}

impl Subscriber for Held {
    fn notified<'a>(&'a self, payload: &'a str) -> Dropping<'a> {
        Box::pin(async move {
            let id = payload
                .split_once(':')
                .and_then(|(id, _version)| Uuid::parse_str(id).ok());
            let reloaded = match id {
                Some(id) => self.reload(id).await,
                None => self.reload_all().await,
            };
            if let Err(error) = reloaded {
                tracing::warn!(%error, "a changed source could not be reloaded; lookups read the store");
                self.whole.store(false, Ordering::Release);
            }
        })
    }

    fn reset(&self) -> Dropping<'_> {
        Box::pin(async move {
            match self.reload_all().await {
                Ok(()) => self.whole.store(true, Ordering::Release),
                Err(error) => {
                    tracing::warn!(%error, "the sources could not be reloaded; lookups read the store");
                    self.whole.store(false, Ordering::Release);
                }
            }
        })
    }
}

/// Where the HL7 v2 face finds the source an MLLP sender is routed to.
#[async_trait::async_trait]
pub trait Routes: std::fmt::Debug + Send + Sync {
    /// Returns the source the sender `application` at `facility` (MSH-3 and
    /// MSH-4, as their namespace id or `^<universal id>^<type>`) is routed
    /// to.
    ///
    /// # Errors
    /// Returns [`sqlx::Error`] when the routes cannot be read.
    async fn routed(
        &self,
        application: &str,
        facility: &str,
    ) -> Result<Option<Arc<Source>>, sqlx::Error>;
}

#[async_trait::async_trait]
impl Routes for Sources {
    async fn routed(
        &self,
        application: &str,
        facility: &str,
    ) -> Result<Option<Arc<Source>>, sqlx::Error> {
        if self.trusted() {
            return Ok(self.held.current().routed(application, facility));
        }
        Ok(self
            .held
            .store
            .routed(application, facility)
            .await?
            .map(Arc::new))
    }
}

/// A route table that never changes, for a test of the face.
#[derive(Debug, Clone, Default)]
pub struct Fixed(Vec<Arc<Source>>);

impl Fixed {
    /// Returns the table of `sources`.
    #[must_use]
    pub fn new(sources: Vec<Source>) -> Self {
        Self(sources.into_iter().map(Arc::new).collect())
    }
}

#[async_trait::async_trait]
impl Routes for Fixed {
    async fn routed(
        &self,
        application: &str,
        facility: &str,
    ) -> Result<Option<Arc<Source>>, sqlx::Error> {
        Ok(Snapshot {
            by_id: self
                .0
                .iter()
                .map(|source| (source.id, Arc::clone(source)))
                .collect(),
        }
        .routed(application, facility))
    }
}

/// Every source this replica serves, the rows behind them and the secrets
/// they name.
#[derive(Debug)]
pub struct Sources {
    /// The snapshot and its store.
    held: Arc<Held>,
    /// The listener state of [`SOURCES`].
    listened: Arc<Listened>,
    /// The `[secrets]` directory, when configured.
    secrets: Option<SecretDir>,
    /// The thread that reads the directory again, while the sources live.
    watch: Option<ferrohealth_secret::dir::Watch>,
    /// The maps and logical models a declared resolution renders through.
    structuremaps: Arc<crate::structuremap::Running>,
}

impl Sources {
    /// Returns the sources `settings` configure over `store`: the
    /// `[secrets]` directory opened and read again at its poll interval.
    ///
    /// # Errors
    /// Returns [`ferrohealth_secret::dir::SecretDirError`] when the
    /// directory cannot be opened or watched.
    pub fn open(
        store: &crate::store::Store,
        secrets: Option<&ferrohealth_secret::dir::SecretDirConfig>,
    ) -> Result<Self, ferrohealth_secret::dir::SecretDirError> {
        let Some(config) = secrets else {
            return Ok(Self::new(store.pool().clone(), None));
        };
        let dir = SecretDir::open(&config.dir)?;
        let watch = dir.watch(config.poll_seconds.interval(), |error| {
            tracing::warn!(error = %error, "the [secrets] directory could not be read again");
        })?;
        let mut sources = Self::new(store.pool().clone(), Some(dir));
        sources.watch = Some(watch);
        Ok(sources)
    }

    /// Returns the sources the state store behind `pool` keeps, resolving
    /// their secrets in `secrets`; every lookup reads the store until the
    /// listener [`Self::listened`] carries connects.
    #[must_use]
    pub fn new(pool: sqlx::postgres::PgPool, secrets: Option<SecretDir>) -> Self {
        let held = Arc::new(Held {
            store: SourceStore::new(pool),
            snapshot: RwLock::new(Arc::new(Snapshot::default())),
            whole: AtomicBool::new(false),
        });
        let listened = Arc::new(Listened::with_subscriber(
            SOURCES.name(),
            Arc::<Held>::clone(&held),
        ));
        Self {
            held,
            listened,
            secrets,
            watch: None,
            structuremaps: Arc::default(),
        }
    }

    /// Returns these sources resolving through the maps of `structuremaps`.
    #[must_use]
    pub fn with_structuremaps(self, structuremaps: Arc<crate::structuremap::Running>) -> Self {
        Self {
            structuremaps,
            ..self
        }
    }

    /// Returns the maps and logical models of the running mapping revision.
    #[must_use]
    pub const fn structuremaps(&self) -> &Arc<crate::structuremap::Running> {
        &self.structuremaps
    }

    /// Returns the listener state the replica's `LISTEN` session carries.
    #[must_use]
    pub fn listened(&self) -> Arc<Listened> {
        Arc::clone(&self.listened)
    }

    /// Returns the rows.
    #[must_use]
    pub fn store(&self) -> &SourceStore {
        &self.held.store
    }

    /// Whether a lookup may answer from the snapshot.
    fn trusted(&self) -> bool {
        self.listened.is_listening() && self.held.whole.load(Ordering::Acquire)
    }

    /// Returns the source named `name`: from the snapshot while the
    /// listener is connected, from its row otherwise.
    ///
    /// # Errors
    /// Returns [`sqlx::Error`] when the row must be read and cannot be.
    pub async fn named(&self, name: &str) -> Result<Option<Arc<Source>>, sqlx::Error> {
        if self.trusted() {
            return Ok(self.held.current().named(name));
        }
        Ok(self.held.store.named(name).await?.map(Arc::new))
    }

    /// Returns the source `id`: from the snapshot while the listener is
    /// connected, from its row otherwise.
    ///
    /// # Errors
    /// Returns [`sqlx::Error`] when the row must be read and cannot be.
    pub async fn get(&self, id: Uuid) -> Result<Option<Arc<Source>>, sqlx::Error> {
        if self.trusted() {
            return Ok(self.held.current().by_id.get(&id).cloned());
        }
        Ok(self.held.store.get(id).await?.map(Arc::new))
    }

    /// Reads `value` as a document of `kind`, every secret it names held by
    /// the `[secrets]` directory and the map of its resolution in the
    /// running mapping revision.
    ///
    /// # Errors
    /// Returns [`Invalid`] for a document of another shape, one that breaks
    /// a rule of its kind, one naming a secret the directory does not hold,
    /// or one whose resolution names a map the revision does not run.
    pub fn validate(&self, kind: Kind, value: &serde_json::Value) -> Result<Document, Invalid> {
        let document = Document::read(kind, value)?;
        if let Document::Webhook(webhook) = &document
            && let Some(resolution) = webhook.resolve.as_ref()
        {
            resolution.runs_on(&self.structuremaps.current())?;
        }
        for name in document.secrets() {
            let held = self
                .secrets
                .as_ref()
                .is_some_and(|secrets| secrets.get(name).is_ok());
            if !held {
                return Err(Invalid(format!(
                    "the secret {name} is not in the [secrets] directory"
                )));
            }
        }
        Ok(document)
    }

    /// Returns the values of `keys`, which `source` names, newest first.
    ///
    /// # Errors
    /// Returns [`SecretUnavailable`] for the first key the `[secrets]`
    /// directory cannot answer now.
    pub fn keys(
        &self,
        source: &Source,
        keys: &[SecretRef],
    ) -> Result<Vec<SecretString>, SecretUnavailable> {
        keys.iter()
            .map(|key| {
                let unavailable = |cause| SecretUnavailable {
                    source_name: source.name.clone(),
                    name: key.secret.clone(),
                    cause,
                };
                let secrets = self.secrets.as_ref().ok_or_else(|| unavailable(None))?;
                secrets
                    .get(&key.secret)
                    .map_err(|error| unavailable(Some(error)))
            })
            .collect()
    }
}
