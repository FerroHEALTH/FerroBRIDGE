// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! `/operator/sources/{id}/parked`, `…/settle` and `…/replay`: the parked
//! inbound work of one source, settled by an operator, and work run again.
//!
//! A settlement records the subject of the operator's token and the reason
//! given. No specification governs the operator surface: our own design.

use std::sync::Arc;

use axum::Router;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse as _, Response};
use axum::routing::{get, post};
use ferrobridge_operator_api::inbound::{
    ParkedItem, ParkedItems, Replay, Replayed, Settle, Settlement,
};
use ferrohealth_smart::resource::Access;
use uuid::Uuid;

use super::guard;
use super::save::subject_of;
use crate::inbound::intake::Intake;
use crate::inbound::store::{self, WorkKey};
use crate::problem::{Kind, refuse};

/// The intake a request reads and writes, `None` while `[inbound]` is off.
type Held = State<Option<Arc<Intake>>>;

/// The routes, behind the operator guard of `smart`.
pub fn routes(intake: Option<Arc<Intake>>, smart: &crate::smart::Smart) -> Router {
    Router::new()
        .route("/operator/sources/{id}/parked", get(parked))
        .route("/operator/sources/{id}/settle", post(settle))
        .route("/operator/sources/{id}/replay", post(replay))
        .with_state(intake)
        .route_layer(guard::layer(smart))
}

/// The `404` of a bridge with the intake off.
const NO_INTAKE: &str = "this bridge runs no inbound intake: [inbound] is not on";

/// The `404` of an id no source has.
const NO_SOURCE: &str = "no source has this id";

/// Returns the intake and the source `id` names, or the kind and detail of
/// the refusal.
async fn source(
    intake: Option<&Arc<Intake>>,
    id: &str,
) -> Result<(Arc<Intake>, Uuid), (Kind, &'static str)> {
    let intake = intake.ok_or((Kind::NotFound, NO_INTAKE))?;
    let id = Uuid::parse_str(id).map_err(|_text| (Kind::NotFound, NO_SOURCE))?;
    match intake.sources().get(id).await {
        Ok(Some(_)) => Ok((Arc::clone(intake), id)),
        Ok(None) => Err((Kind::NotFound, NO_SOURCE)),
        Err(error) => {
            tracing::warn!(%error, "the inbound sources could not be read");
            Err((Kind::InboundUnavailable, UNAVAILABLE))
        }
    }
}

/// The detail of a store that failed.
const UNAVAILABLE: &str = "the inbox could not be read or written";

/// `GET /operator/sources/{id}/parked`.
async fn parked(State(intake): Held, Path(id): Path<String>) -> Response {
    let (intake, id) = match source(intake.as_ref(), &id).await {
        Ok(found) => found,
        Err((kind, detail)) => return refuse(kind, detail),
    };
    match intake.store().parked(id).await {
        Ok(items) => axum::Json(ParkedItems {
            items: items
                .into_iter()
                .map(|item| ParkedItem {
                    tenant: item.key.tenant,
                    entity: item.key.entity,
                    reason: item.reason,
                    attempts: item.attempts,
                    since: item.since,
                })
                .collect(),
        })
        .into_response(),
        Err(error) => {
            tracing::warn!(%error, "the parked inbound work could not be read");
            refuse(Kind::InboundUnavailable, UNAVAILABLE)
        }
    }
}

/// `POST /operator/sources/{id}/settle`: `204` once settled, `404` when no
/// parked item has the entity.
async fn settle(
    State(intake): Held,
    Path(id): Path<String>,
    access: Access,
    axum::Json(body): axum::Json<Settle>,
) -> Response {
    let (intake, id) = match source(intake.as_ref(), &id).await {
        Ok(found) => found,
        Err((kind, detail)) => return refuse(kind, detail),
    };
    if body.reason.trim().is_empty() {
        return refuse(Kind::ReasonRequired, "a settlement names its reason");
    }
    let key = WorkKey {
        source_id: id,
        tenant: body.tenant,
        entity: body.entity,
    };
    let settlement = match body.settlement {
        Settlement::Retry => store::Settlement::Retry,
        Settlement::Discard => store::Settlement::Discard,
    };
    match intake
        .store()
        .settle(&key, settlement, subject_of(&access), body.reason.trim())
        .await
    {
        Ok(true) => StatusCode::NO_CONTENT.into_response(),
        Ok(false) => refuse(Kind::NotParked, "no parked item has this entity"),
        Err(error) => {
            tracing::warn!(%error, "a parked inbound item could not be settled");
            refuse(Kind::InboundUnavailable, UNAVAILABLE)
        }
    }
}

/// `POST /operator/sources/{id}/replay`: `202` with the items that run
/// again.
async fn replay(
    State(intake): Held,
    Path(id): Path<String>,
    axum::Json(body): axum::Json<Replay>,
) -> Response {
    let (intake, id) = match source(intake.as_ref(), &id).await {
        Ok(found) => found,
        Err((kind, detail)) => return refuse(kind, detail),
    };
    let replay = match body {
        Replay::Entity { tenant, entity } => store::Replay::Entity { tenant, entity },
        Replay::Received { from, to } => {
            match (
                from.parse::<jiff::Timestamp>(),
                to.parse::<jiff::Timestamp>(),
            ) {
                (Ok(from), Ok(to)) if from < to => store::Replay::Received { from, to },
                _ => {
                    return crate::problem::blank(
                        StatusCode::BAD_REQUEST,
                        "a replay range names two RFC 3339 instants, the first before the last",
                    );
                }
            }
        }
    };
    match intake.store().replay(id, &replay).await {
        Ok(items) => (StatusCode::ACCEPTED, axum::Json(Replayed { items })).into_response(),
        Err(error) => {
            tracing::warn!(%error, "inbound work could not be replayed");
            refuse(Kind::InboundUnavailable, UNAVAILABLE)
        }
    }
}
