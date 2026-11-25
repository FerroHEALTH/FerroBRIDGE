// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The bridge's own access records, behind `[audit]`.
//!
//! Every access an authenticated caller makes through the FHIR facade is
//! recorded as the access record of Regulation (EU) 2025/327 Annex II 3.2,
//! built through `ehds-logging` with its categories classified from the
//! template and archetype ids the request reached ([`crate::cdr::reach`]), and written
//! as an IHE BALP `AuditEvent` ([`crate::facade::audit`]). Each message the HL7 v2 face
//! commits is recorded as a system import ([`hl7v2`], ITI TF-2 Table
//! 3.20.4.1.1.1-1 "PHI-import"). Both are stored in the state store's audit
//! outbox ([`outbox`]) before the answer, and sent from there over the ITI-20
//! ATX: FHIR Feed (ITI TF-2 §3.20.4.2) by the `ihe-atna` forwarder of
//! whichever replica claims them, so a replica that stops loses no record.
//!
//! Recording fails closed: an access whose record the outbox cannot take is
//! refused before the CDR is reached, the `AuditRecorder` contract of
//! `ihe-atna` (no specification governs this: our own design). The OMOP ETL
//! and the operator API record nothing of their own.

pub mod feed;
pub mod hl7v2;
pub mod outbox;

use std::sync::Arc;

use ehds_logging::balp::BalpSink;
use ehds_logging::label::LabelPolicy;
use ehds_logging::map::CategoryMap;
use ehds_logging::retention::RetentionPolicy;
use ihe_atna::atna::feed::FeedRepository;
use ihe_atna::atna::forwarder::Forwarder;
use ihe_atna::atna::forwarder::Status;
use ihe_atna::balp::AuditRecorder;
use ihe_atna::balp::NetworkAddress;
use ihe_atna::balp::Observer;
use ihe_atna::balp::SOURCE_APPLICATION_SERVER;
use url::Url;

use outbox::OutboxRecorder;
use outbox::StateOutbox;

/// The access-record lane, resolved from `[audit]`.
#[derive(Debug, Clone)]
pub struct Settings {
    /// The Audit Record Repository's FHIR Feed.
    pub feed: FeedRepository,
    /// How the bridge names itself as the recording system.
    pub source_id: String,
    /// The site the bridge runs at.
    pub site: Option<String>,
    /// The deployment's category map.
    pub categories: CategoryMap,
}

/// The access-record lane: the outbox, its forwarder and what every record
/// is classified and kept by.
pub struct AccessLog {
    /// The forwarder that delivers the outbox to the repository.
    forwarder: Arc<Forwarder<Arc<StateOutbox>>>,
    /// The recorder that stores each record in the outbox.
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
    /// Returns the lane `settings` names for the facade served at `base`,
    /// whose data come from the CDR at `origin`, keeping its outbox in
    /// `store`.
    ///
    /// Nothing is delivered until [`AccessLog::spawn`].
    #[must_use]
    pub fn open(
        settings: &Settings,
        base: &Url,
        origin: &Url,
        store: &crate::store::Store,
    ) -> Self {
        let outbox = Arc::new(StateOutbox::new(store.pool().clone()));
        let forwarder = Forwarder::fhir_feed_from(Arc::clone(&outbox), settings.feed.clone());
        let observer = Observer {
            source_id: settings.source_id.clone(),
            site: settings.site.clone(),
            host: NetworkAddress::host(base.host_str().unwrap_or(&settings.source_id)),
            source_types: vec![SOURCE_APPLICATION_SERVER],
        };
        let recorder: Arc<dyn AuditRecorder> = Arc::new(OutboxRecorder::new(outbox, observer));
        let endpoint = NetworkAddress::uri(base);
        let sink = BalpSink::new(Arc::clone(&recorder), endpoint.clone());
        Self {
            forwarder,
            recorder,
            sink,
            categories: settings.categories.clone(),
            retention: RetentionPolicy::default(),
            labels: LabelPolicy::default(),
            origin: NetworkAddress::uri(origin).address(),
            endpoint,
        }
    }

    /// Starts delivering the outbox, oldest record first, until
    /// [`AccessLog::stop`].
    #[must_use]
    pub fn spawn(&self) -> tokio::task::JoinHandle<()> {
        tokio::spawn(Arc::clone(&self.forwarder).run())
    }

    /// Asks the delivery [`AccessLog::spawn`] started to end; a record not yet
    /// delivered stays in the outbox for any replica's forwarder.
    pub fn stop(&self) {
        self.forwarder.stop();
    }

    /// Returns what the outbox holds and how its deliveries went.
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

    /// Returns the sink that writes an access record as a BALP `AuditEvent`.
    #[must_use]
    pub const fn sink(&self) -> &BalpSink {
        &self.sink
    }

    /// Returns the deployment's category map.
    #[must_use]
    pub const fn categories(&self) -> &CategoryMap {
        &self.categories
    }

    /// Returns how long a record is kept.
    #[must_use]
    pub const fn retention(&self) -> &RetentionPolicy {
        &self.retention
    }

    /// Returns the security labels a record carries.
    #[must_use]
    pub const fn labels(&self) -> &LabelPolicy {
        &self.labels
    }

    /// Returns the CDR the facade's data come from, the origin of Annex II
    /// 3.2(e).
    #[must_use]
    pub fn origin(&self) -> &str {
        &self.origin
    }
}
