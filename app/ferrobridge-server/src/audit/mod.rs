// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The bridge's own access records, behind `[audit]`.
//!
//! Every access an authenticated caller makes through the FHIR facade is
//! recorded as the access record of Regulation (EU) 2025/327 Annex II 3.2,
//! built through `ehds-logging` with its categories classified from the
//! template and archetype ids the request reached ([`reach`]), and written
//! as an IHE BALP `AuditEvent` ([`facade`]). Each message the HL7 v2 face
//! commits is recorded as a system import ([`hl7v2`], ITI TF-2 Table
//! 3.20.4.1.1.1-1 "PHI-import"). Both are stored in a spool on disk and
//! sent from there over the ITI-20 ATX: FHIR Feed (ITI TF-2 §3.20.4.2) by
//! the `ihe-atna` forwarder.
//!
//! Recording fails closed: an access whose record has no place in the spool
//! is refused before the CDR is reached, the `AuditRecorder` contract of
//! `ihe-atna` (no specification governs this: our own design). The OMOP ETL
//! and the operator API record nothing of their own.

pub mod facade;
pub mod hl7v2;
pub mod reach;
mod spool;

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use ehds_logging::balp::BalpSink;
use ehds_logging::label::LabelPolicy;
use ehds_logging::map::CategoryMap;
use ehds_logging::retention::RetentionPolicy;
use ihe_atna::atna::feed::FeedRepository;
use ihe_atna::atna::forwarder::Forwarder;
use ihe_atna::atna::forwarder::Status;
use ihe_atna::atna::spool::Bounds;
use ihe_atna::atna::spool::Content;
use ihe_atna::atna::spool::Spool;
use ihe_atna::atna::spool::SpoolError;
use ihe_atna::balp::AuditRecorder;
use ihe_atna::balp::NetworkAddress;
use ihe_atna::balp::Observer;
use ihe_atna::balp::SOURCE_APPLICATION_SERVER;
use url::Url;

use spool::SpoolRecorder;

/// The access-record lane, resolved from `[audit]`.
#[derive(Debug, Clone)]
pub struct Settings {
    /// The Audit Record Repository's FHIR Feed.
    pub feed: FeedRepository,
    /// The directory the spool keeps the records in.
    pub spool: PathBuf,
    /// The spool's bounds.
    pub bounds: Bounds,
    /// The longest wait between two delivery attempts.
    pub retry_max: Duration,
    /// How the bridge names itself as the recording system.
    pub source_id: String,
    /// The site the bridge runs at.
    pub site: Option<String>,
    /// The deployment's category map.
    pub categories: CategoryMap,
}

/// Why the access-record lane could not start.
#[derive(Debug, thiserror::Error)]
#[error("the access-record spool at {} could not be opened", path.display())]
pub struct OpenError {
    /// The spool's directory.
    path: PathBuf,
    /// Why it could not be opened.
    #[source]
    source: SpoolError,
}

/// The access-record lane: the spool, its forwarder and what every record is
/// classified and kept by.
pub struct AccessLog {
    /// The forwarder that delivers the spool to the repository.
    forwarder: Arc<Forwarder<Spool>>,
    /// The recorder that stores each record in the spool.
    recorder: Arc<dyn AuditRecorder>,
    /// The sink that writes an access record as a BALP `AuditEvent`.
    sink: BalpSink,
    /// The deployment's category map.
    categories: CategoryMap,
    /// How long a record is kept.
    retention: RetentionPolicy,
    /// The security labels a record carries.
    labels: LabelPolicy,
    /// The CDR the facade's data come from, the origin of Annex II 3.2(e).
    origin: String,
    /// The address the facade's requests arrive at.
    endpoint: NetworkAddress,
}

impl std::fmt::Debug for AccessLog {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AccessLog")
            .field("forwarder", &self.forwarder)
            .field("origin", &self.origin)
            .finish_non_exhaustive()
    }
}

impl AccessLog {
    /// Opens the spool `settings` names for the facade served at `base`, whose
    /// data come from the CDR at `origin`.
    ///
    /// Nothing is delivered until [`AccessLog::spawn`].
    ///
    /// # Errors
    ///
    /// Returns [`OpenError`] when the spool cannot be opened.
    pub fn open(settings: &Settings, base: &Url, origin: &Url) -> Result<Self, OpenError> {
        let spool = Spool::open_for(&settings.spool, settings.bounds, Content::AuditEvents)
            .map_err(|source| OpenError {
                path: settings.spool.clone(),
                source,
            })?;
        let forwarder = Forwarder::fhir_feed(spool, settings.feed.clone(), settings.retry_max);
        let observer = Observer {
            source_id: settings.source_id.clone(),
            site: settings.site.clone(),
            host: NetworkAddress::host(base.host_str().unwrap_or(&settings.source_id)),
            source_types: vec![SOURCE_APPLICATION_SERVER],
        };
        let recorder: Arc<dyn AuditRecorder> =
            Arc::new(SpoolRecorder::new(Arc::clone(&forwarder), observer));
        let endpoint = NetworkAddress::uri(base);
        let sink = BalpSink::new(Arc::clone(&recorder), endpoint.clone());
        Ok(Self {
            forwarder,
            recorder,
            sink,
            categories: settings.categories.clone(),
            retention: RetentionPolicy::default(),
            labels: LabelPolicy::default(),
            origin: NetworkAddress::uri(origin).address(),
            endpoint,
        })
    }

    /// Starts delivering the spool, oldest record first, until
    /// [`AccessLog::stop`].
    #[must_use]
    pub fn spawn(&self) -> tokio::task::JoinHandle<()> {
        tokio::spawn(Arc::clone(&self.forwarder).run())
    }

    /// Asks the delivery [`AccessLog::spawn`] started to end; a record not yet
    /// delivered stays in the spool for the next start.
    pub fn stop(&self) {
        self.forwarder.stop();
    }

    /// Returns what the spool holds and how its deliveries went.
    #[must_use]
    pub fn status(&self) -> Status {
        self.forwarder.status()
    }

    /// Returns the recorder a record of the system's own is stored through.
    #[must_use]
    pub fn recorder(&self) -> &dyn AuditRecorder {
        self.recorder.as_ref()
    }

    /// Returns the address the facade's requests arrive at.
    #[must_use]
    pub const fn endpoint(&self) -> &NetworkAddress {
        &self.endpoint
    }
}
