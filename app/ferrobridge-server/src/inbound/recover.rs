// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Recovery per source: the declared redelivery call and the since-cursor
//! reconciliation that catch what an outage lost.
//!
//! A webhook source may retry nothing on its own, so the receiver recovers:
//! the redelivery call asks the source to send its failed events again, at
//! start and after a run of `503` answers, and each reconciliation poll reads
//! the entities changed since its cursor and marks each one's work item due.
//! One replica runs a source's recovery under the lane lease
//! `inbound-recover:<source id>`, and a cursor moves only under the lease
//! generation that read it. No specification governs the recovery schedule:
//! our own design.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

use secrecy::ExposeSecret as _;
use serde::Deserialize;
use tokio::sync::watch;
use uuid::Uuid;

use super::event::Change;
use super::store::{CursorError, InboxStore};
use crate::sources::document::{Document, SecretRef, Webhook};
use crate::sources::{Source, Sources};
use crate::store::Store;
use crate::store::lease::{Generation, Lane, Lease, Renewed};

/// The recovery steps a webhook source declares.
#[derive(Debug, Clone, PartialEq, Eq, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Recovery {
    /// The call that asks the source to send its failed events again.
    pub redeliver: Option<Redeliver>,
    /// The polls that read what changed since a cursor.
    pub reconcile: Option<Reconcile>,
}

/// The redelivery call.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Redeliver {
    /// The HTTP method, `PUT` or `POST`.
    pub method: CallMethod,
    /// The URL.
    pub url: String,
    /// The secret holding the PEM client certificate and key the call
    /// presents.
    #[serde(default)]
    pub credentials: Option<SecretRef>,
    /// The consecutive `503` answers after which the next accepted event
    /// calls for a redelivery; one when left out.
    #[serde(default)]
    pub after_unavailable: Option<u32>,
}

/// The HTTP methods a recovery call takes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum CallMethod {
    /// `GET`.
    Get,
    /// `PUT`.
    Put,
    /// `POST`.
    Post,
}

/// The reconciliation polls of one source.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Reconcile {
    /// Seconds between two runs.
    pub every: u64,
    /// Seconds a new cursor lies before the instant its poll started, so an
    /// entity changed while the poll ran is read again next time.
    #[serde(default)]
    pub overlap: u64,
    /// Seconds before now the first cursor of a tenant lies.
    pub first: u64,
    /// The polls.
    pub polls: Vec<Poll>,
}

/// One since-cursor poll, run per tenant with the tenant's credentials.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Poll {
    /// The URL.
    pub url: String,
    /// The query parameter the cursor travels in, an RFC 3339 instant.
    pub cursor: String,
    /// The pointer to the array of changed entities in the answer.
    pub items: String,
    /// The pointer to an entity's id within one item.
    pub entity: String,
    /// The change each returned entity is marked with.
    pub change: PollChange,
}

/// The change a poll marks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PollChange {
    /// The entity changed.
    Upsert,
    /// The entity was removed.
    Remove,
}

impl Recovery {
    /// Returns the rules the document's shape cannot carry, broken, as the
    /// sentence of the first.
    ///
    /// # Errors
    /// Returns the sentence for a zero interval, a poll without a URL, a
    /// cursor parameter or a pointer, and a URL that is not HTTP.
    pub fn check(&self) -> Result<(), String> {
        if let Some(redeliver) = &self.redeliver {
            url(&redeliver.url)?;
        }
        if let Some(reconcile) = &self.reconcile {
            if reconcile.every == 0 || reconcile.first == 0 {
                return Err(String::from(
                    "recovery.reconcile.every and first are at least 1",
                ));
            }
            if reconcile.polls.is_empty() {
                return Err(String::from("recovery.reconcile names no poll"));
            }
            for poll in &reconcile.polls {
                url(&poll.url)?;
                if poll.cursor.trim().is_empty() {
                    return Err(String::from("a poll names no cursor parameter"));
                }
                if !poll.items.starts_with('/') && !poll.items.is_empty() {
                    return Err(format!("{} is no JSON Pointer", poll.items));
                }
                if !poll.entity.starts_with('/') {
                    return Err(format!("{} is no JSON Pointer", poll.entity));
                }
            }
        }
        Ok(())
    }

    /// Returns the secrets the steps name.
    pub fn secrets(&self) -> impl Iterator<Item = &str> {
        self.redeliver
            .iter()
            .filter_map(|redeliver| redeliver.credentials.as_ref())
            .map(|secret| secret.secret.as_str())
    }
}

/// Refuses `text` unless it is an HTTP or HTTPS URL.
fn url(text: &str) -> Result<(), String> {
    match url::Url::parse(text) {
        Ok(parsed) if matches!(parsed.scheme(), "http" | "https") => Ok(()),
        _ => Err(format!("{text} is no HTTP URL")),
    }
}

/// Why a recovery step failed, naming no value of a record.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum RecoverError {
    /// A secret the step names cannot be read.
    #[error("a secret the recovery step names cannot be read")]
    Secret(#[source] crate::sources::SecretUnavailable),
    /// The secret the step names holds nothing.
    #[error("the recovery step's credentials are empty")]
    NoCredentials,
    /// The client certificate does not read as PEM.
    #[error("the recovery step's client certificate does not read")]
    Identity(#[source] reqwest::Error),
    /// The call could not be made.
    #[error("the recovery call failed")]
    Call(#[source] reqwest::Error),
    /// The source answered with a failure status.
    #[error("the source answered {0}")]
    Status(http::StatusCode),
    /// The answer is no JSON or has no array at the declared pointer.
    #[error("the poll's answer does not read")]
    Unreadable,
    /// The cursor could not be read or moved.
    #[error("the cursor could not be read or moved")]
    Cursor(#[from] CursorError),
}

/// The redelivery calls one source is owed: one at start, and one after a
/// run of `503` answers.
#[derive(Debug)]
pub struct Unavailable {
    /// The consecutive `503` answers per source.
    streaks: std::sync::Mutex<BTreeMap<Uuid, Arc<AtomicU32>>>,
}

impl Default for Unavailable {
    fn default() -> Self {
        Self::new()
    }
}

impl Unavailable {
    /// Returns no streak for any source.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            streaks: std::sync::Mutex::new(BTreeMap::new()),
        }
    }

    /// Returns the streak of `source`.
    fn streak(&self, source: Uuid) -> Arc<AtomicU32> {
        let mut streaks = self
            .streaks
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        Arc::clone(streaks.entry(source).or_default())
    }

    /// Counts one `503` answered to `source`.
    pub fn refused(&self, source: Uuid) {
        self.streak(source).fetch_add(1, Ordering::AcqRel);
    }

    /// Ends the streak of `source` with an accepted event; `true` when the
    /// streak reached `after` and a redelivery is owed.
    pub fn accepted(&self, source: Uuid, after: u32) -> bool {
        let ended = self.streak(source).swap(0, Ordering::AcqRel);
        ended >= after.max(1)
    }
}

/// The recovery of every webhook source on one replica.
#[derive(Debug)]
pub struct Recoverer {
    /// The store, for the lane leases.
    store: Store,
    /// The inbox and the cursors.
    inbox: InboxStore,
    /// The sources.
    sources: Arc<Sources>,
    /// How long a recovery lease holds.
    lease: Duration,
}

impl Recoverer {
    /// Returns the recovery over `store` and `sources`, each source's lane
    /// leased for `lease` at a time.
    #[must_use]
    pub fn new(store: Store, sources: Arc<Sources>, lease: Duration) -> Self {
        Self {
            inbox: InboxStore::new(store.pool().clone()),
            store,
            sources,
            lease,
        }
    }

    /// Returns the lease of `source`'s recovery lane.
    ///
    /// # Errors
    /// Returns [`crate::store::lease::LeaseError`] for a lane that cannot be
    /// named, which a source id always can.
    pub fn lease_of(&self, source: Uuid) -> Result<Lease, crate::store::lease::LeaseError> {
        Ok(self
            .store
            .lease(Lane::new(format!("inbound-recover:{source}"))?, self.lease))
    }

    /// Calls `source`'s redelivery step, when it declares one; `false` when
    /// it declares none.
    ///
    /// # Errors
    /// Returns [`RecoverError`] when the call fails.
    pub async fn redeliver(&self, source: &Source) -> Result<bool, RecoverError> {
        let Some(redeliver) =
            webhook_of(source).and_then(|webhook| webhook.recovery.redeliver.as_ref())
        else {
            return Ok(false);
        };
        let client = self.client(source, redeliver.credentials.as_ref())?;
        let method = match redeliver.method {
            CallMethod::Get => reqwest::Method::GET,
            CallMethod::Put => reqwest::Method::PUT,
            CallMethod::Post => reqwest::Method::POST,
        };
        let answer = client
            .request(method, &redeliver.url)
            .send()
            .await
            .map_err(RecoverError::Call)?;
        if !answer.status().is_success() {
            return Err(RecoverError::Status(answer.status()));
        }
        Ok(true)
    }

    /// Runs every poll of `source` for each enabled tenant whose cursor is
    /// due, under `generation` of the source's lease; returns the entities
    /// marked.
    ///
    /// # Errors
    /// Returns [`RecoverError`] for the first poll that fails; the cursors
    /// of the polls before it have moved.
    pub async fn reconcile(
        &self,
        source: &Source,
        lease: &Lease,
        generation: Generation,
        force: bool,
    ) -> Result<u64, RecoverError> {
        let Some((webhook, reconcile)) = webhook_of(source)
            .and_then(|webhook| Some((webhook, webhook.recovery.reconcile.as_ref()?)))
        else {
            return Ok(0);
        };
        let mut marked = 0;
        for tenant in webhook.tenants.iter().filter(|tenant| tenant.enabled) {
            let client = self.client(source, tenant.credentials.as_ref())?;
            for (index, poll) in reconcile.polls.iter().enumerate() {
                let index = i32::try_from(index).unwrap_or(i32::MAX);
                let started = jiff::Timestamp::now();
                let cursor = self
                    .inbox
                    .cursor(source.id, &tenant.name, index)
                    .await
                    .map_err(CursorError::Database)?;
                let since = match cursor {
                    Some((cursor, polled_at))
                        if force
                            || started.as_second().saturating_sub(polled_at.as_second())
                                >= i64::try_from(reconcile.every).unwrap_or(i64::MAX) =>
                    {
                        cursor
                    }
                    Some(_) => continue,
                    None => {
                        started
                            - jiff::SignedDuration::from_secs(
                                i64::try_from(reconcile.first).unwrap_or(i64::MAX),
                            )
                    }
                };
                let entities = fetch(&client, poll, since).await?;
                let change = match poll.change {
                    PollChange::Upsert => Change::Upsert,
                    PollChange::Remove => Change::Remove,
                };
                marked += self
                    .inbox
                    .mark(source.id, &tenant.name, &entities, change)
                    .await
                    .map_err(CursorError::Database)?;
                let next = started
                    - jiff::SignedDuration::from_secs(
                        i64::try_from(reconcile.overlap).unwrap_or(i64::MAX),
                    );
                self.inbox
                    .move_cursor(lease, generation, source.id, &tenant.name, index, next)
                    .await?;
            }
        }
        Ok(marked)
    }

    /// Recovers each gap of `source`'s FHIR Subscriptions through `$events`
    /// under `generation` of the source's lease; returns the events stored.
    ///
    /// # Errors
    /// Returns [`RecoverError`] for the first gap that cannot be recovered.
    pub async fn catch_up(
        &self,
        source: &Source,
        lease: &Lease,
        generation: Generation,
    ) -> Result<u64, RecoverError> {
        let Document::FhirSubscription(subscription) = &source.parsed else {
            return Ok(0);
        };
        let keeper = super::keeper::Keeper::new(self.store.pool().clone());
        let mut stored = 0;
        for gap in keeper
            .gaps(source.id)
            .await
            .map_err(CursorError::Database)?
        {
            let credentials = subscription
                .tenants
                .iter()
                .find(|tenant| tenant.name == gap.tenant)
                .and_then(|tenant| tenant.credentials.as_ref());
            let client = self.client(source, credentials)?;
            let mut url = events_url(subscription.base.as_deref(), &gap.subscription)?;
            url.query_pairs_mut()
                .append_pair("eventsSinceNumber", &gap.from.to_string())
                .append_pair("eventsUntilNumber", &gap.to.to_string())
                .append_pair("content", "id-only");
            let body = get_json(&client, url, "application/fhir+json").await?;
            let notification =
                super::subscription::read(&body).map_err(|_malformed| RecoverError::Unreadable)?;
            if notification.subscription != gap.subscription {
                return Err(RecoverError::Unreadable);
            }
            if let super::keeper::Received::Recorded { stored: count, .. } = keeper
                .receive(source.id, &subscription.subscriptions, &notification)
                .await
                .map_err(CursorError::Database)?
            {
                stored += u64::try_from(count).unwrap_or(u64::MAX);
            }
            keeper.close(lease, generation, source.id, &gap).await?;
        }
        Ok(stored)
    }

    /// Returns a client presenting the PEM certificate and key `credentials`
    /// names, or none.
    fn client(
        &self,
        source: &Source,
        credentials: Option<&SecretRef>,
    ) -> Result<reqwest::Client, RecoverError> {
        let builder = reqwest::Client::builder().timeout(Duration::from_secs(30));
        let builder = match credentials {
            Some(credentials) => {
                let pem = self
                    .sources
                    .keys(source, std::slice::from_ref(credentials))
                    .map_err(RecoverError::Secret)?;
                let pem = pem
                    .first()
                    .map(|secret| secret.expose_secret().as_bytes().to_vec())
                    .ok_or(RecoverError::NoCredentials)?;
                builder.identity(reqwest::Identity::from_pem(&pem).map_err(RecoverError::Identity)?)
            }
            None => builder,
        };
        builder.build().map_err(RecoverError::Call)
    }

    /// Runs recovery for every webhook source on its own task until `stop`
    /// reads `true`: a redelivery for each source this replica leads at
    /// start, then reconciliation every `tick`.
    #[must_use]
    pub fn spawn(
        self: &Arc<Self>,
        tick: Duration,
        stop: &watch::Receiver<bool>,
    ) -> tokio::task::JoinHandle<()> {
        let recoverer = Arc::clone(self);
        let mut stop = stop.clone();
        tokio::spawn(async move {
            let mut redelivered = std::collections::BTreeSet::new();
            while !*stop.borrow() {
                recoverer.round(&mut redelivered).await;
                tokio::select! {
                    () = tokio::time::sleep(tick) => {}
                    _ = stop.changed() => {}
                }
            }
        })
    }

    /// Runs one round over every webhook source this replica can lead.
    async fn round(&self, redelivered: &mut std::collections::BTreeSet<Uuid>) {
        let sources = match self.sources.store().list().await {
            Ok(sources) => sources,
            Err(error) => {
                tracing::warn!(%error, "the inbound sources could not be listed for recovery");
                return;
            }
        };
        for source in sources.into_iter().filter(|source| source.enabled) {
            let recovers = match &source.parsed {
                Document::Webhook(webhook) => webhook.recovery != Recovery::default(),
                Document::FhirSubscription(_) => true,
                Document::Mllp(_) => false,
            };
            if !recovers {
                continue;
            }
            let Ok(lease) = self.lease_of(source.id) else {
                continue;
            };
            let generation = match lease.claim().await {
                Ok(Some(generation)) => generation,
                Ok(None) => continue,
                Err(error) => {
                    tracing::warn!(%error, source = %source.name, "the recovery lease could not be taken");
                    continue;
                }
            };
            if redelivered.insert(source.id)
                && let Err(error) = self.redeliver(&source).await
            {
                tracing::warn!(%error, source = %source.name, "the redelivery at start failed");
            }
            if let Err(error) = self.reconcile(&source, &lease, generation, false).await {
                tracing::warn!(%error, source = %source.name, "a reconciliation poll failed");
            }
            if let Err(error) = self.catch_up(&source, &lease, generation).await {
                tracing::warn!(%error, source = %source.name, "a subscription gap could not be recovered");
            }
            if matches!(lease.renew(generation).await, Ok(Renewed::Lost)) {
                tracing::info!(source = %source.name, "another replica took the recovery lease");
            }
        }
    }
}

/// Returns the webhook document of `source`.
fn webhook_of(source: &Source) -> Option<&Webhook> {
    match &source.parsed {
        Document::Webhook(webhook) => Some(webhook),
        Document::FhirSubscription(_) | Document::Mllp(_) => None,
    }
}

/// Returns the `$events` URL of `subscription`, resolving a relative
/// reference against `base` (Subscriptions R5 Backport STU 1.1.0,
/// `OperationDefinition` `backport-subscription-events`).
fn events_url(base: Option<&str>, subscription: &str) -> Result<url::Url, RecoverError> {
    let absolute = match url::Url::parse(subscription) {
        Ok(url) => url,
        Err(_relative) => {
            let base = base.ok_or(RecoverError::Unreadable)?;
            url::Url::parse(&format!("{}/{subscription}", base.trim_end_matches('/')))
                .map_err(|_url| RecoverError::Unreadable)?
        }
    };
    url::Url::parse(&format!(
        "{}/$events",
        absolute.as_str().trim_end_matches('/')
    ))
    .map_err(|_url| RecoverError::Unreadable)
}

/// Reads the JSON answer of a `GET` of `url` accepting `accept`, refusing a
/// failure status with the status.
async fn get_json(
    client: &reqwest::Client,
    url: url::Url,
    accept: &'static str,
) -> Result<serde_json::Value, RecoverError> {
    let answer = client
        .get(url)
        .header(http::header::ACCEPT, accept)
        .send()
        .await
        .map_err(RecoverError::Call)?;
    if !answer.status().is_success() {
        return Err(RecoverError::Status(answer.status()));
    }
    answer
        .json()
        .await
        .map_err(|_json| RecoverError::Unreadable)
}

/// Reads the entities `poll` returns since `since`.
async fn fetch(
    client: &reqwest::Client,
    poll: &Poll,
    since: jiff::Timestamp,
) -> Result<Vec<String>, RecoverError> {
    let mut url = url::Url::parse(&poll.url).map_err(|_url| RecoverError::Unreadable)?;
    url.query_pairs_mut()
        .append_pair(&poll.cursor, &since.to_string());
    let body = get_json(client, url, "application/json").await?;
    let items = body
        .pointer(&poll.items)
        .and_then(serde_json::Value::as_array)
        .ok_or(RecoverError::Unreadable)?;
    items
        .iter()
        .map(|item| match item.pointer(&poll.entity) {
            Some(serde_json::Value::String(text)) if !text.is_empty() => Ok(text.clone()),
            Some(serde_json::Value::Number(number)) => Ok(number.to_string()),
            _ => Err(RecoverError::Unreadable),
        })
        .collect()
}
