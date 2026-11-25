// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The one extractor of a lane the operator API serves only while the bridge
//! runs it: the lane the router's state holds, or the lane's refusal.
//!
//! No specification governs the operator surface: our own design.

use std::sync::Arc;

use axum::extract::FromRequestParts;
use http::request::Parts;

use super::refused::Refused;
use crate::problem::Kind;

/// A lane a router holds as `Option<Arc<Self>>`, `None` while it is off.
pub(crate) trait Optional: Send + Sync + 'static {
    /// The refusal of a request while the lane is off.
    const OFF: Refused;
}

/// The lane a request acts on, refused with its [`Optional::OFF`] while the
/// lane is off.
#[derive(Debug)]
pub(crate) struct Lane<T>(pub(crate) Arc<T>);

impl<T: Optional> FromRequestParts<Option<Arc<T>>> for Lane<T> {
    type Rejection = Refused;

    fn from_request_parts(
        _parts: &mut Parts,
        lane: &Option<Arc<T>>,
    ) -> impl Future<Output = Result<Self, Refused>> + Send {
        std::future::ready(lane.clone().map(Self).ok_or(T::OFF))
    }
}

impl Optional for crate::etl::history::Store {
    const OFF: Refused = Refused::Problem(
        Kind::NotFound,
        "this bridge has no CDM database to keep a run history in",
    );
}

impl Optional for crate::hl7v2::directory::Directory {
    const OFF: Refused = Refused::Problem(Kind::NotFound, "this bridge runs no HL7 v2 face");
}

impl Optional for crate::inbound::intake::Intake {
    const OFF: Refused = Refused::Problem(
        Kind::NotFound,
        "this bridge runs no inbound intake: [inbound] is not on",
    );
}

impl Optional for crate::settings::Registers {
    const OFF: Refused = Refused::Problem(
        Kind::NotConfigured,
        "no [state] is configured, so the bridge keeps no settings register",
    );
}

impl Optional for crate::sources::Sources {
    const OFF: Refused = Refused::Problem(
        Kind::NotFound,
        "this bridge keeps no inbound sources: [state] is not configured",
    );
}
