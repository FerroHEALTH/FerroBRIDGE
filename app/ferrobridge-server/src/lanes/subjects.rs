// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The subject link the facade and the inbound engine ask through: audited
//! under `[audit]`, and answered through the identity source's pseudonym
//! cache with the holder of the deployment's PMIR subscription under
//! `[identity_source.cache]`. No specification governs the start order: our
//! own design.

use std::sync::Arc;

use crate::config::FacadeSettings;
use crate::state::AppState;

/// Makes `lane`'s subject link record every exchange with the identity
/// source in `log`.
pub(super) fn audited(lane: &mut FacadeSettings, log: &crate::audit::AccessLog) {
    let identity = lane.settings.subject.identity().clone();
    lane.settings.subject = crate::identity::subject::SubjectLink::new(Arc::new(
        identity.audited(Arc::clone(log.recorder())),
    ));
}

/// Returns `state` running the pseudonym cache `[identity_source.cache]`
/// configures, with `lane`'s subject link asking through it; `state`
/// unchanged without the table.
///
/// # Errors
/// Returns an error when the table is present and the state store is not,
/// or the store has no sealing keys.
pub(super) fn cached(lane: &mut FacadeSettings, state: AppState) -> anyhow::Result<AppState> {
    use anyhow::Context;

    let identity = lane.settings.subject.identity();
    let Some(settings) = identity.cache_settings() else {
        return Ok(state);
    };
    let store = state.store().context(
        "[identity_source.cache] needs a [state] section: its subscription is held there",
    )?;
    let running = crate::identity_source::Running::start(settings, store)
        .context("starting the identity source's pseudonym cache")?;
    lane.settings.subject = crate::identity::subject::SubjectLink::new(Arc::new(
        identity.clone().with_cache(running.cache.clone()),
    ));
    tracing::info!(
        lane = "identity_source",
        "the identity source's pseudonyms are cached while its PMIR feed is active"
    );
    Ok(state.with_pseudonyms(Arc::new(running)))
}
