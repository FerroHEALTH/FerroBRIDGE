// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! SMART Health Links, behind `[shl]`: the person's one-time share of their
//! patient summary.
//!
//! The bridge is the SMART Health Links Sharing Application and its
//! manifest server (SMART Health Cards and Links 1.0.0,
//! `links-specification`): the xShare Yellow Button groups the Link Provider
//! with the Content Creator, and the bridge writes the patient summary. A
//! person with a `patient/` token creates a link, lists their links and
//! revokes one; a receiver resolves it without a credential, by a manifest
//! request or, under `U`, a direct file request.
//!
//! A link's file is sealed once, at creation, under a key the server hands
//! back in the payload and never keeps. Every link expires; its answers and
//! its wrong passcodes are counted over its life; its file locations are
//! single-use and lapse within the hour the specification allows. A link
//! that is no longer active answers `404`, the answer the specification
//! fixes. The store is [`store::LinkStore`], shared by every replica.

mod handlers;
pub mod passcode;
pub mod record;
pub mod seal;
pub mod store;

use std::sync::Arc;
use std::time::Duration;

use axum::Router;
use axum::routing::delete;
use axum::routing::get;
use axum::routing::post;
use ihe_atna::balp::AuditError;
use ihe_atna::balp::Exchange;
use ihe_atna::balp::NetworkAddress;
use ihe_atna::balp::Reserved;

use crate::audit::AccessLog;
use crate::facade::Facade;
use crate::store::lease::{Lane, Leader, Lease};

/// The path the person's routes sit under.
pub const BASE: &str = "/shl";

/// The path a manifest request and a direct file request are made to,
/// before the link's manifest id.
pub const MANIFEST: &str = "/shl/m/";

/// The path a file location is fetched from, before its token.
pub const FILE: &str = "/shl/f/";

/// How often the store is swept for expired links and lapsed locations.
const SWEEP: Duration = Duration::from_secs(60);

/// The lane the sweep's lease names.
const SWEEP_LANE: &str = "shl-sweep";

/// How long the sweep's lease holds, renewed every third of it.
// NOTE: no specification governs this: our own design; three sweeps long, so
// a replica that stops leaves the sweep to another within three minutes.
const SWEEP_LEASE: Duration = Duration::from_secs(180);

/// The links lane, resolved from `[shl]`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Settings {
    /// The public origin a receiver reaches the manifest routes at, with no
    /// trailing slash.
    pub base_url: String,
    /// How long a link lives when its creator names no expiry.
    pub default_expiry: Duration,
    /// The longest a link may live.
    pub max_expiry: Duration,
    /// The most answers a link may give.
    pub max_answers: u32,
    /// The wrong passcodes a link takes before it is disabled.
    pub passcode_attempts: u32,
    /// How long a file location lives.
    pub location_lifetime: Duration,
}

/// The links lane: the store, the facade that writes the documents, and
/// the access-record lane that records each event of a link.
#[derive(Debug)]
pub struct Links {
    /// The store.
    store: Arc<store::LinkStore>,
    /// What the deployment configured.
    settings: Settings,
    /// The facade the patient summary is written by.
    facade: Arc<Facade>,
    /// The access-record lane, when it is on.
    log: Option<Arc<AccessLog>>,
    /// The address the records name the bridge by.
    endpoint: NetworkAddress,
    /// The lease the sweep runs under, on one replica at a time.
    sweep_lease: Lease,
}

/// Why the lane could not start.
#[derive(Debug, thiserror::Error)]
#[error("the link store could not be opened")]
pub struct OpenError(#[source] store::StoreError);

impl Links {
    /// Opens the links of `state`, writing documents through `facade` and
    /// recording through `log`.
    ///
    /// # Errors
    ///
    /// Returns [`OpenError`] when `state` carries no sealing keys.
    pub fn open(
        state: &crate::store::Store,
        settings: Settings,
        facade: Arc<Facade>,
        log: Option<Arc<AccessLog>>,
    ) -> Result<Self, OpenError> {
        let store = store::LinkStore::open(state).map_err(OpenError)?;
        let sweep_lease = state.lease(
            Lane::new(SWEEP_LANE).map_err(|error| OpenError(store::StoreError::from(error)))?,
            SWEEP_LEASE,
        );
        let endpoint = log.as_ref().map_or_else(
            || NetworkAddress::MachineName(settings.base_url.clone()),
            |log| log.endpoint().clone(),
        );
        Ok(Self {
            store: Arc::new(store),
            settings,
            facade,
            log,
            endpoint,
            sweep_lease,
        })
    }

    /// Returns what the deployment configured.
    #[must_use]
    pub const fn settings(&self) -> &Settings {
        &self.settings
    }

    /// Sweeps the store every minute on its own task, recording each link
    /// that ends, until `stop` reads `true`; a sweep under way finishes
    /// first.
    #[must_use]
    pub fn spawn_sweeper(
        self: &Arc<Self>,
        stop: &tokio::sync::watch::Receiver<bool>,
    ) -> tokio::task::JoinHandle<()> {
        let links = Arc::clone(self);
        let mut stop = stop.clone();
        tokio::spawn(async move {
            let mut leader = Leader::new(Arc::new(links.sweep_lease.clone()));
            let mut ticks = tokio::time::interval(SWEEP);
            loop {
                tokio::select! {
                    biased;
                    () = async {
                        let _settled = stop.wait_for(|stopped| *stopped).await.is_ok();
                    } => break,
                    _tick = ticks.tick() => links.sweep(&mut leader).await,
                }
            }
            if let Err(error) = leader.release().await {
                tracing::warn!(reason = %crate::indicators::reason(&error), "the sweep lease could not be released; it runs out on its own");
            }
        })
    }

    /// Removes every expired link and lapsed location now while this
    /// replica leads the sweep, recording each link that ends.
    async fn sweep(&self, leader: &mut Leader) {
        let term = match leader.lead().await {
            Ok(Some(term)) => term,
            Ok(None) => return,
            Err(error) => {
                tracing::warn!(reason = %crate::indicators::reason(&error), "the sweep lease could not be claimed");
                return;
            }
        };
        let now = jiff::Timestamp::now().as_second();
        match self.store.sweep(&term, now).await {
            Ok(ended) => {
                for gone in ended {
                    self.ended(&gone).await;
                }
            }
            Err(store::StoreError::Fenced(_)) => leader.lost(),
            Err(error) => {
                tracing::warn!(error = %ferrohealth_server::run::chain(&error), "the link store could not be swept");
            }
        }
    }

    /// Records that `gone` left the store, logging a record that cannot be
    /// stored: the link is already gone.
    async fn ended(&self, gone: &store::Gone) {
        let exchange = record::exchange(
            record::Event::Ended(gone.ending),
            (&gone.handle, &gone.owner),
            &record::Actor::System,
            &self.endpoint,
        );
        let stored = match self.reserve().await {
            Ok(place) => place.store(exchange).await,
            Err(error) => Err(error),
        };
        if let Err(error) = stored {
            tracing::warn!(error = %ferrohealth_server::run::chain(&error), "the end of a link could not be recorded");
        }
    }

    /// Returns the place of one record: reserved in the outbox, or taken at
    /// store time from a recorder that reserves none.
    ///
    /// # Errors
    ///
    /// Returns the [`AuditError`] of an outbox that cannot take a record.
    async fn reserve(&self) -> Result<Place<'_>, AuditError> {
        match self.log.as_ref() {
            None => Ok(Place::Off),
            Some(log) => match log.recorder().reserve().await? {
                Some(reserved) => Ok(Place::Reserved(reserved)),
                None => Ok(Place::Later(log)),
            },
        }
    }
}

/// Where one record goes.
enum Place<'a> {
    /// No access-record lane runs.
    Off,
    /// A place reserved in the outbox.
    Reserved(Box<dyn Reserved>),
    /// The recorder takes the record when it is stored.
    Later(&'a Arc<AccessLog>),
}

impl Place<'_> {
    /// Stores `exchange` in this place.
    ///
    /// # Errors
    ///
    /// Returns the recorder's [`AuditError`].
    async fn store(self, exchange: Exchange) -> Result<(), AuditError> {
        match self {
            Self::Off => Ok(()),
            Self::Reserved(reserved) => reserved.record(exchange).await,
            Self::Later(log) => log.recorder().record(exchange).await,
        }
    }
}

/// Returns the lane's routes: the person's, behind `smart`'s gate with a
/// read permission on `Patient` and the level `[assurance] shl` names, and
/// the receiver's, public with any origin.
pub fn routes(links: Arc<Links>, smart: &crate::smart::Smart) -> Router {
    let person = Router::new()
        .route(BASE, post(handlers::create).get(handlers::list))
        .route(&format!("{BASE}/{{handle}}"), delete(handlers::revoke))
        .with_state(Arc::clone(&links))
        .route_layer(axum::middleware::from_fn_with_state(
            smart.assurance(crate::assurance::RouteClass::Shl),
            crate::assurance::require,
        ))
        .route_layer(handlers::guard(smart));
    let receiver = Router::new()
        .route(
            &format!("{MANIFEST}{{id}}"),
            post(handlers::manifest)
                .get(handlers::direct)
                .options(handlers::preflight),
        )
        .route(
            &format!("{FILE}{{token}}"),
            get(handlers::file).options(handlers::preflight),
        )
        .with_state(links)
        .layer(axum::middleware::from_fn(handlers::any_origin));
    person.merge(receiver)
}
