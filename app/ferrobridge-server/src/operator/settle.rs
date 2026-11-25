// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The one settle step of the operator API: an operator's retry or discard
//! of parked work, the change feed's dead letters and the inbound engine's
//! work items alike.
//!
//! The step refuses a reason the route could not read, records the subject
//! of the operator's token as who decided, and hands the decision to the
//! route's store; each route keeps its own wire. No specification governs
//! the operator surface: our own design.

use ferrohealth_smart::resource::Access;

use super::refused::Refused;
use super::save::subject_of;
use crate::problem::Kind;
use crate::retry::{Decided, Decision};

/// Settles one parked item as `decision` says: refuses a missing `reason`
/// with the detail `unreasoned`, then hands the decision, with the subject
/// of `access` as who decided, to `store`, which answers the refusal of an
/// item it could not settle.
///
/// # Errors
/// Returns the refusal of a missing reason, or the one `store` answers.
pub(super) async fn settle(
    access: &Access,
    decision: Decision,
    reason: Option<String>,
    unreasoned: &'static str,
    store: impl AsyncFnOnce(&Decided) -> Result<(), Refused>,
) -> Result<Decided, Refused> {
    let reason = reason.ok_or(Refused::Problem(Kind::ReasonRequired, unreasoned))?;
    let decided = Decided {
        decision,
        by: subject_of(access).to_owned(),
        reason,
    };
    store(&decided).await?;
    Ok(decided)
}
