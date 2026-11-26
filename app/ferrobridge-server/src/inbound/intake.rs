// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! `POST /inbound/{source}`: one verified event, stored before it is
//! answered.
//!
//! The raw body is read within the source's limit, verified before it is
//! parsed, read through the source's event map and stored with its work item
//! in one transaction; then the source gets its `200`. A probe, an event the
//! map ignores and an event for a tenant the source does not serve are
//! answered `200` and not stored. A failed verification answers `401` and
//! stores nothing. Above the source's `max_pending`, or with the store down,
//! the intake answers `503`, so the source sends again. No specification
//! governs an inbound intake: our own design.

use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse as _, Response};
use axum::routing::post;
use ferrohealth_server::tls::PeerCertificates;
use secrecy::ExposeSecret as _;

use super::event::{Event, Reading};
use super::keeper::Keeper;
use super::recover::{Recoverer, Unavailable};
use super::store::InboxStore;
use super::verify::{Key, Proofs, Refused, Request, StaticHeader, Verifier};
use crate::problem::{Kind, blank, refuse};
use crate::sources::document::{Document, FhirSubscription, Webhook};
use crate::sources::{Source, Sources};

/// The intake one replica serves.
#[derive(Debug)]
pub struct Intake {
    /// The sources requests are verified against.
    sources: Arc<Sources>,
    /// The inbox.
    store: InboxStore,
    /// The state of the FHIR Subscriptions the sources notify.
    keeper: Keeper,
    /// The largest body any source may send, in bytes.
    body_limit: usize,
    /// The recovery steps, which a run of `503` answers calls for.
    recoverer: Option<Arc<Recoverer>>,
    /// The consecutive `503` answers per source.
    unavailable: Unavailable,
}

impl Intake {
    /// Returns the intake over `sources` and `store`, reading at most
    /// `body_limit` bytes of any body.
    #[must_use]
    pub fn new(sources: Arc<Sources>, store: InboxStore, body_limit: usize) -> Self {
        Self {
            sources,
            keeper: Keeper::new(store.pool().clone()),
            store,
            body_limit,
            recoverer: None,
            unavailable: Unavailable::new(),
        }
    }

    /// Returns this intake calling the redelivery steps of `recoverer`
    /// after a run of `503` answers.
    #[must_use]
    pub fn with_recoverer(mut self, recoverer: Arc<Recoverer>) -> Self {
        self.recoverer = Some(recoverer);
        self
    }

    /// Returns the recovery steps, when they run.
    #[must_use]
    pub const fn recoverer(&self) -> Option<&Arc<Recoverer>> {
        self.recoverer.as_ref()
    }

    /// Returns the inbox.
    #[must_use]
    pub const fn store(&self) -> &InboxStore {
        &self.store
    }

    /// Returns the sources.
    #[must_use]
    pub const fn sources(&self) -> &Arc<Sources> {
        &self.sources
    }
}

/// The routes of the intake.
pub fn routes(intake: Arc<Intake>) -> Router {
    Router::new()
        .route("/inbound/{source}", post(receive))
        .with_state(intake)
}

/// How long a source waits before it sends again after a `503`.
const RETRY_AFTER: HeaderValue = HeaderValue::from_static("5");

/// Returns `503` with `Retry-After` for `kind`.
fn unavailable(kind: Kind, detail: &'static str) -> Response {
    let mut response = refuse(kind, detail);
    response
        .headers_mut()
        .insert(header::RETRY_AFTER, RETRY_AFTER);
    response
}

/// `POST /inbound/{source}`.
async fn receive(
    State(intake): State<Arc<Intake>>,
    Path(name): Path<String>,
    peer: Option<axum::Extension<PeerCertificates>>,
    headers: HeaderMap,
    body: Body,
) -> Response {
    let source = match intake.sources.named(&name).await {
        Ok(Some(source)) if source.enabled => source,
        Ok(_) => return refuse(Kind::UnknownSource, UNKNOWN),
        Err(error) => {
            tracing::warn!(%error, "the inbound sources could not be read");
            return unavailable(
                Kind::InboundUnavailable,
                "the inbound sources could not be read",
            );
        }
    };
    let limit = match &source.parsed {
        Document::Webhook(webhook) => webhook.limits.body_limit.min(intake.body_limit),
        Document::FhirSubscription(_) => intake.body_limit,
        Document::Mllp(_) => return refuse(Kind::UnknownSource, UNKNOWN),
    };
    let Ok(bytes) = axum::body::to_bytes(body, limit).await else {
        return blank(
            StatusCode::PAYLOAD_TOO_LARGE,
            "the body is larger than the source may send",
        );
    };
    let request = Request {
        headers: &headers,
        body: &bytes,
        peer: peer
            .as_ref()
            .and_then(|axum::Extension(chain)| chain.0.first()),
        now: jiff::Timestamp::now().as_second(),
    };
    match &source.parsed {
        Document::Webhook(webhook) => receive_event(&intake, &source, webhook, &request).await,
        Document::FhirSubscription(subscription) => {
            receive_notification(&intake, &source, subscription, &request).await
        }
        Document::Mllp(_) => refuse(Kind::UnknownSource, UNKNOWN),
    }
}

/// The `404` detail of a name no enabled source of an HTTP kind has.
const UNKNOWN: &str = "no enabled webhook or FHIR Subscription source has this name";

/// Checks `request` against every proof of `source`, answering the `401`
/// of the first that fails, logged with its reason.
fn unproven(source: &Source, proofs: &Proofs<'_>, request: &Request<'_>) -> Option<Response> {
    let failed = proofs.verify(request, None).err()?;
    let reason = failed.map_or(Refused::Declaration, |unproven| unproven.reason);
    tracing::info!(source = %source.name, %reason, "an inbound request failed verification");
    Some(refuse(
        Kind::Unverified,
        "the request does not prove it comes from the source",
    ))
}

/// Returns the values of `keys` as verifier keys.
fn keys_of(
    intake: &Intake,
    source: &Source,
    keys: &[crate::sources::document::SecretRef],
) -> Result<Vec<Key>, crate::sources::SecretUnavailable> {
    Ok(intake
        .sources
        .keys(source, keys)?
        .iter()
        .map(|key| Key::new(key.expose_secret().as_bytes().to_vec()))
        .collect())
}

/// One webhook event.
async fn receive_event(
    intake: &Arc<Intake>,
    source: &Arc<Source>,
    webhook: &Webhook,
    request: &Request<'_>,
) -> Response {
    let keys = match keys_of(intake, source, &webhook.keys) {
        Ok(keys) => keys,
        Err(unavailable) => return unavailable.into_response(),
    };
    if let Some(refused) = unproven(
        source,
        &Proofs::default().with(&webhook.verify, keys),
        request,
    ) {
        return refused;
    }
    let Ok(value) = serde_json::from_slice::<serde_json::Value>(request.body) else {
        return refuse(Kind::UnreadableEvent, "the body is no JSON");
    };
    let event = match webhook.events.read(request.headers, &value) {
        Ok(Reading::Store(event)) => event,
        Ok(Reading::Answer(_)) => return StatusCode::OK.into_response(),
        Err(unreadable) => {
            tracing::info!(source = %source.name, %unreadable, "an inbound event could not be read");
            return refuse(
                Kind::UnreadableEvent,
                "the event lacks a part the source's map reads",
            );
        }
    };
    let Some(tenant) = served(webhook, &event) else {
        return StatusCode::OK.into_response();
    };
    match intake.store.pending(source.id).await {
        Ok(pending) if pending >= webhook.limits.max_pending => {
            intake.unavailable.refused(source.id);
            return unavailable(
                Kind::InboundBusy,
                "the source has as much pending work as it may",
            );
        }
        Ok(_) => {}
        Err(error) => {
            tracing::warn!(%error, source = %source.name, "the inbox could not be read");
            intake.unavailable.refused(source.id);
            return unavailable(Kind::InboundUnavailable, "the inbox could not be read");
        }
    }
    match intake.store.accept(source.id, tenant, &event).await {
        Ok(_) => {
            redeliver_after_outage(intake, source, webhook);
            StatusCode::OK.into_response()
        }
        Err(error) => {
            tracing::warn!(%error, source = %source.name, "an inbound event could not be stored");
            intake.unavailable.refused(source.id);
            unavailable(Kind::InboundUnavailable, "the event could not be stored")
        }
    }
}

/// One FHIR Subscription notification: verified by the header and the
/// client certificate the source declares, read as an R4 notification
/// Bundle, and recorded with one work item per event.
async fn receive_notification(
    intake: &Intake,
    source: &Source,
    subscription: &FhirSubscription,
    request: &Request<'_>,
) -> Response {
    let mut proofs = Proofs::default();
    if let Some(header) = &subscription.header {
        let keys = match keys_of(intake, source, &header.keys) {
            Ok(keys) => keys,
            Err(unavailable) => return unavailable.into_response(),
        };
        let verifier = Verifier::StaticHeader(StaticHeader {
            header: header.name.clone(),
        });
        proofs = proofs.with(verifier, keys);
    }
    if let Some(mtls) = &subscription.mtls {
        proofs = proofs.with(Verifier::Mtls(mtls.clone()), Vec::new());
    }
    if let Some(refused) = unproven(source, &proofs, request) {
        return refused;
    }
    let notification = match serde_json::from_slice::<serde_json::Value>(request.body)
        .map_err(|_json| super::subscription::Malformed::NotHistory)
        .and_then(|value| super::subscription::read(&value))
    {
        Ok(notification) => notification,
        Err(malformed) => {
            tracing::info!(source = %source.name, %malformed, "a notification could not be read");
            return refuse(
                Kind::UnreadableEvent,
                "the body is no R4 subscription notification",
            );
        }
    };
    match intake
        .keeper
        .receive(source.id, &subscription.subscriptions, &notification)
        .await
    {
        Ok(_) => StatusCode::OK.into_response(),
        Err(error) => {
            tracing::warn!(%error, source = %source.name, "a notification could not be recorded");
            unavailable(
                Kind::InboundUnavailable,
                "the notification could not be recorded",
            )
        }
    }
}

/// Calls the redelivery step of `source` on its own task when this accepted
/// event ends a run of `503` answers as long as the step asks for.
fn redeliver_after_outage(intake: &Arc<Intake>, source: &Arc<Source>, webhook: &Webhook) {
    let Some(redeliver) = webhook.recovery.redeliver.as_ref() else {
        return;
    };
    let after = redeliver.after_unavailable.unwrap_or(1);
    if !intake.unavailable.accepted(source.id, after) {
        return;
    }
    let Some(recoverer) = intake.recoverer.clone() else {
        return;
    };
    let source = Arc::clone(source);
    drop(tokio::spawn(async move {
        if let Err(error) = recoverer.redeliver(&source).await {
            tracing::warn!(%error, source = %source.name, "the redelivery after an outage failed");
        }
    }));
}

/// Returns the tenant `event` is stored under, `None` when the source serves
/// no enabled tenant of that name; a source whose map names no tenant serves
/// its one declared tenant.
fn served<'a>(webhook: &'a Webhook, event: &Event) -> Option<&'a str> {
    let tenant = match event.tenant.as_deref() {
        Some(name) => webhook.tenants.iter().find(|tenant| tenant.name == name),
        None => webhook.tenants.first(),
    }
    .filter(|tenant| tenant.enabled)?;
    Some(tenant.name.as_str())
}
