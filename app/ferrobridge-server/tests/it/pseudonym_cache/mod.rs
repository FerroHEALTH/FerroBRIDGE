// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The identity source's pseudonym cache: the ITI-94 exchanges of its
//! subscription audited under `[audit]`, and, against `FerroPIX` when
//! `FERROBRIDGE_E2E=1` admits the harness, the cache kept true by the PMIR
//! feed ([`e2e`]).

mod e2e;

use std::collections::BTreeMap;
use std::error::Error;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use ferrobridge_server::config::{Config, Process};
use ferrobridge_server::identity_source::IdentitySource;

/// A recorder that keeps every exchange it is given.
#[derive(Default)]
pub(super) struct Kept(Mutex<Vec<ihe_atna::balp::Exchange>>);

impl Kept {
    /// How many exchanges of the audit profile `kind` were kept.
    pub(super) fn count(&self, kind: &ihe_atna::balp::EventKind) -> usize {
        self.0
            .lock()
            .unwrap()
            .iter()
            .filter(|exchange| exchange.kind.profile == kind.profile)
            .count()
    }

    /// The outcome of the last exchange of the audit profile `kind` kept.
    pub(super) fn last_outcome(
        &self,
        kind: &ihe_atna::balp::EventKind,
    ) -> Option<ihe_atna::balp::Outcome> {
        self.0
            .lock()
            .unwrap()
            .iter()
            .rev()
            .find(|exchange| exchange.kind.profile == kind.profile)
            .map(|exchange| exchange.outcome)
    }
}

#[async_trait::async_trait]
impl ihe_atna::balp::AuditRecorder for Kept {
    async fn record(
        &self,
        exchange: ihe_atna::balp::Exchange,
    ) -> Result<(), ihe_atna::balp::AuditError> {
        self.0.lock().unwrap().push(exchange);
        Ok(())
    }
}

/// Returns the identity source `text` configures, resolved for a job.
pub(super) fn identity_of(text: &str) -> Result<IdentitySource, Box<dyn Error>> {
    let settings = Config::from_sources(Some(text), &BTreeMap::new())?.resolve(Process::Job)?;
    let identity = settings
        .identity_source
        .ok_or("the identity source is resolved")?;
    Ok((*identity).clone())
}

#[tokio::test]
async fn an_audited_identity_source_records_each_iti_94_exchange_of_its_subscription()
-> Result<(), Box<dyn Error>> {
    // PMIR 1.6.0 §2:3.94.5.1: the Subscriber records each create and read of its subscription.
    let registry = wiremock::MockServer::start().await;
    let identity = identity_of(&format!(
        "[identity_source]\npseudonym_domain = \"urn:oid:2.999.9\"\nsource_domains = [\"urn:oid:2.999.1\"]\n\n\
         [identity_source.endpoint]\nurl = \"{}/fhir\"\n\n\
         [identity_source.cache]\nfeed_url = \"https://bridge.example.org/pmir/feed\"\nfeed_clients = [\"ferropix\"]\n\n\
         [state]\nurl = \"postgres://bridge@db.invalid/state?sslmode=disable\"\n",
        registry.uri()
    ))?;
    let kept = Arc::new(Kept::default());
    let identity = identity.audited(kept.clone());
    let subscription = &identity
        .cache_settings()
        .ok_or("the cache is configured")?
        .subscription;
    let timeout = Duration::from_secs(5);
    let subscriber = &subscription.subscriber;
    let created = subscriber.subscribe(&subscription.request, timeout).await;
    assert!(created.is_err(), "the stub registry creates nothing");
    let located = ihe_iti::pmir::subscription::Subscribed::at(
        subscriber.endpoint(),
        &format!("{}/synthetic-subscription", subscriber.endpoint()),
    )?;
    let read = subscriber.status(&located, timeout).await;
    assert!(read.is_err(), "the stub registry holds nothing");
    assert_eq!(1, kept.count(&ihe_iti::pmir::audit::subscription_create()));
    assert_eq!(1, kept.count(&ihe_iti::pmir::audit::subscription_read()));
    Ok(())
}
