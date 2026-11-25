// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! `/operator/sources/{id}/parked`, `…/settle`, `…/replay`, `…/reconcile`
//! and `…/redeliver`: the parked inbound work of one source, settled by an
//! operator, work run again, and the source's recovery run on demand.
//!
//! A settlement records the subject of the operator's token and the reason
//! given, and every write logs one line under [`guard::AUDIT_TARGET`]. No
//! specification governs the operator surface: our own design.

use std::sync::Arc;

use axum::Router;
use axum::extract::{FromRequestParts, Path, State};
use axum::http::StatusCode;
use axum::http::request::Parts;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use ferrobridge_operator_api::inbound::{ParkedItem, ParkedItems, Replay, Replayed, Settle};
use ferrohealth_smart::resource::Access;
use uuid::Uuid;

use super::guard;
use super::lane::{Lane, Optional as _};
use super::refused::Refused;
use super::save::subject_of;
use crate::inbound::intake::Intake;
use crate::inbound::recover::Recoverer;
use crate::inbound::store::{self, WorkKey};
use crate::problem::Kind;
use crate::retry::Decision;
use crate::sources::Source;

/// The intake a request reads and writes, `None` while `[inbound]` is off.
type Held = State<Option<Arc<Intake>>>;

/// The routes, behind the operator guard of `smart`.
pub fn routes(intake: Option<Arc<Intake>>, smart: &crate::smart::Smart) -> Router {
    Router::new()
        .route("/operator/sources/{id}/parked", get(parked))
        .route("/operator/sources/{id}/settle", post(settle))
        .route("/operator/sources/{id}/replay", post(replay))
        .route("/operator/sources/{id}/reconcile", post(reconcile))
        .route("/operator/sources/{id}/redeliver", post(redeliver))
        .with_state(intake)
        .route_layer(guard::layer(smart))
}

/// The `404` of an id no source has.
const NO_SOURCE: Refused = Refused::Problem(Kind::NotFound, "no source has this id");

/// The detail of a store that failed.
const UNAVAILABLE: &str = "the inbox could not be read or written";

/// The `502` detail of a recovery call that failed.
const FAILED: &str = "the source refused or failed the recovery call";

/// Returns the `503` of a store that failed, logging `what` and the cause.
fn unavailable(what: &'static str) -> impl FnOnce(sqlx::Error) -> Refused {
    move |error| {
        tracing::warn!(%error, "{what}");
        Refused::Problem(Kind::InboundUnavailable, UNAVAILABLE)
    }
}

/// Returns the source `id` names in the sources of `intake`.
async fn source(intake: &Intake, id: &str) -> Result<Arc<Source>, Refused> {
    let id = Uuid::parse_str(id).map_err(|_text| NO_SOURCE)?;
    intake
        .sources()
        .get(id)
        .await
        .map_err(unavailable("the inbound sources could not be read"))?
        .ok_or(NO_SOURCE)
}

/// What a recovery route acts on: the source its path names, the recovery
/// that runs it, and the subject of the operator's token.
struct Recovery {
    /// The recovery of the intake.
    recoverer: Arc<Recoverer>,
    /// The source the path names.
    source: Arc<Source>,
    /// The subject of the operator's token.
    subject: String,
}

impl FromRequestParts<Option<Arc<Intake>>> for Recovery {
    type Rejection = Response;

    async fn from_request_parts(
        parts: &mut Parts,
        intake: &Option<Arc<Intake>>,
    ) -> Result<Self, Response> {
        let Path(id) = Path::<String>::from_request_parts(parts, intake)
            .await
            .map_err(IntoResponse::into_response)?;
        let access = Access::from_request_parts(parts, intake)
            .await
            .map_err(IntoResponse::into_response)?;
        let Lane(intake) = Lane::from_request_parts(parts, intake)
            .await
            .map_err(IntoResponse::into_response)?;
        let source = source(&intake, &id)
            .await
            .map_err(IntoResponse::into_response)?;
        let recoverer = intake
            .recoverer()
            .cloned()
            .ok_or(Intake::OFF)
            .map_err(IntoResponse::into_response)?;
        Ok(Self {
            recoverer,
            source,
            subject: subject_of(&access).to_owned(),
        })
    }
}

/// `POST /operator/sources/{id}/reconcile`: runs every poll of the source
/// now under its recovery lease; `202` with the items marked, `409` while
/// another replica holds the lease.
async fn reconcile(
    Recovery {
        recoverer,
        source,
        subject,
    }: Recovery,
) -> Result<Response, Refused> {
    let lease = recoverer
        .lease_of(source.id)
        .map_err(|_lane| Refused::Problem(Kind::InboundUnavailable, UNAVAILABLE))?;
    let generation = lease
        .claim()
        .await
        .map_err(|error| {
            tracing::warn!(%error, "the recovery lease could not be taken");
            Refused::Problem(Kind::InboundUnavailable, UNAVAILABLE)
        })?
        .ok_or(Refused::Problem(
            Kind::RecoveryLeased,
            "another replica runs this source's recovery",
        ))?;
    let items = recoverer
        .reconcile(&source, &lease, generation, true)
        .await
        .map_err(|error| {
            tracing::warn!(%error, source = %source.name, "a reconciliation an operator asked for failed");
            Refused::Problem(Kind::RecoveryFailed, FAILED)
        })?;
    guard::audited!(
        &subject,
        "reconcile",
        StatusCode::ACCEPTED,
        source = source.id.to_string().as_str()
    );
    Ok((StatusCode::ACCEPTED, axum::Json(Replayed { items })).into_response())
}

/// `POST /operator/sources/{id}/redeliver`: calls the source's redelivery
/// step now; `204` once the source accepted it.
async fn redeliver(
    Recovery {
        recoverer,
        source,
        subject,
    }: Recovery,
) -> Result<Response, Refused> {
    let called = recoverer.redeliver(&source).await.map_err(|error| {
        tracing::warn!(%error, source = %source.name, "a redelivery an operator asked for failed");
        Refused::Problem(Kind::RecoveryFailed, FAILED)
    })?;
    if !called {
        return Err(Refused::Problem(
            Kind::NotFound,
            "the source declares no redelivery step",
        ));
    }
    guard::audited!(
        &subject,
        "redeliver",
        StatusCode::NO_CONTENT,
        source = source.id.to_string().as_str()
    );
    Ok(StatusCode::NO_CONTENT.into_response())
}

/// `GET /operator/sources/{id}/parked`.
async fn parked(Lane(intake): Lane<Intake>, Path(id): Path<String>) -> Result<Response, Refused> {
    let source = source(&intake, &id).await?;
    let items = intake
        .store()
        .parked(source.id)
        .await
        .map_err(unavailable("the parked inbound work could not be read"))?;
    Ok(axum::Json(ParkedItems {
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
    .into_response())
}

/// `POST /operator/sources/{id}/settle`: `204` once settled, `404` when no
/// parked item has the entity.
async fn settle(
    State(intake): Held,
    Path(id): Path<String>,
    access: Access,
    axum::Json(body): axum::Json<Settle>,
) -> Result<Response, Refused> {
    let intake = intake.ok_or(Intake::OFF)?;
    let source = source(&intake, &id).await?;
    let reason = Some(body.reason.trim())
        .filter(|reason| !reason.is_empty())
        .map(str::to_owned);
    let key = WorkKey {
        source_id: source.id,
        tenant: body.tenant,
        entity: body.entity,
    };
    let decision = Decision::from(body.settlement);
    super::settle::settle(
        &access,
        decision,
        reason,
        "a settlement names its reason",
        async |decided| {
            let settled = intake
                .store()
                .settle(&key, decided)
                .await
                .map_err(unavailable("a parked inbound item could not be settled"))?;
            if settled {
                Ok(())
            } else {
                Err(Refused::Problem(
                    Kind::NotParked,
                    "no parked item has this entity",
                ))
            }
        },
    )
    .await?;
    guard::audited!(
        subject_of(&access),
        decision.as_str(),
        StatusCode::NO_CONTENT,
        source = source.id.to_string().as_str()
    );
    Ok(StatusCode::NO_CONTENT.into_response())
}

/// `POST /operator/sources/{id}/replay`: `202` with the items that run
/// again.
async fn replay(
    State(intake): Held,
    Path(id): Path<String>,
    access: Access,
    axum::Json(body): axum::Json<Replay>,
) -> Result<Response, Refused> {
    let intake = intake.ok_or(Intake::OFF)?;
    let source = source(&intake, &id).await?;
    let replay = match body {
        Replay::Entity { tenant, entity } => store::Replay::Entity { tenant, entity },
        Replay::Received { from, to } => match (
            from.parse::<jiff::Timestamp>(),
            to.parse::<jiff::Timestamp>(),
        ) {
            (Ok(from), Ok(to)) if from < to => store::Replay::Received { from, to },
            _ => {
                return Err(Refused::Blank(
                    StatusCode::BAD_REQUEST,
                    "a replay range names two RFC 3339 instants, the first before the last",
                ));
            }
        },
    };
    let items = intake
        .store()
        .replay(source.id, &replay)
        .await
        .map_err(unavailable("inbound work could not be replayed"))?;
    guard::audited!(
        subject_of(&access),
        "replay",
        StatusCode::ACCEPTED,
        source = source.id.to_string().as_str()
    );
    Ok((StatusCode::ACCEPTED, axum::Json(Replayed { items })).into_response())
}
