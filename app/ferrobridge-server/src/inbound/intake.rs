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
use super::store::InboxStore;
use super::verify::{Key, Request};
use crate::problem::{Kind, blank, refuse};
use crate::sources::Sources;
use crate::sources::document::{Document, Webhook};

/// The intake one replica serves.
#[derive(Debug)]
pub struct Intake {
    /// The sources requests are verified against.
    sources: Arc<Sources>,
    /// The inbox.
    store: InboxStore,
    /// The largest body any source may send, in bytes.
    body_limit: usize,
}

impl Intake {
    /// Returns the intake over `sources` and `store`, reading at most
    /// `body_limit` bytes of any body.
    #[must_use]
    pub const fn new(sources: Arc<Sources>, store: InboxStore, body_limit: usize) -> Self {
        Self {
            sources,
            store,
            body_limit,
        }
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
        Ok(_) => {
            return refuse(
                Kind::UnknownSource,
                "no enabled webhook source has this name",
            );
        }
        Err(error) => {
            tracing::warn!(%error, "the inbound sources could not be read");
            return unavailable(
                Kind::InboundUnavailable,
                "the inbound sources could not be read",
            );
        }
    };
    let Document::Webhook(webhook) = &source.parsed else {
        return refuse(
            Kind::UnknownSource,
            "no enabled webhook source has this name",
        );
    };
    let limit = webhook.limits.body_limit.min(intake.body_limit);
    let Ok(bytes) = axum::body::to_bytes(body, limit).await else {
        return blank(
            StatusCode::PAYLOAD_TOO_LARGE,
            "the body is larger than the source may send",
        );
    };
    let keys = match intake.sources.keys(&source, &webhook.keys) {
        Ok(keys) => keys
            .iter()
            .map(|key| Key::new(key.expose_secret().as_bytes().to_vec()))
            .collect::<Vec<_>>(),
        Err(unavailable) => return unavailable.into_response(),
    };
    let request = Request {
        headers: &headers,
        body: &bytes,
        peer: peer
            .as_ref()
            .and_then(|axum::Extension(chain)| chain.0.first()),
        now: jiff::Timestamp::now().as_second(),
    };
    if let Err(reason) = webhook.verify.verify(&request, &keys) {
        tracing::info!(source = %source.name, %reason, "an inbound request failed verification");
        return refuse(
            Kind::Unverified,
            "the request does not prove it comes from the source",
        );
    }
    let Ok(value) = serde_json::from_slice::<serde_json::Value>(&bytes) else {
        return refuse(Kind::UnreadableEvent, "the body is no JSON");
    };
    let event = match webhook.events.read(&headers, &value) {
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
            return unavailable(
                Kind::InboundBusy,
                "the source has as much pending work as it may",
            );
        }
        Ok(_) => {}
        Err(error) => {
            tracing::warn!(%error, source = %source.name, "the inbox could not be read");
            return unavailable(Kind::InboundUnavailable, "the inbox could not be read");
        }
    }
    match intake.store.accept(source.id, tenant, &event).await {
        Ok(_) => StatusCode::OK.into_response(),
        Err(error) => {
            tracing::warn!(%error, source = %source.name, "an inbound event could not be stored");
            unavailable(Kind::InboundUnavailable, "the event could not be stored")
        }
    }
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
