// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The inbound lane's start: the sources of the state store with their
//! intake, and the workers that apply the stored work. No specification
//! governs the start order: our own design.

use std::sync::Arc;

use crate::config::Settings;
use crate::state::AppState;

/// Returns the inbound workers of `state`, applying each item with the one
/// handler over the facade's identity source, CDR client and identity map,
/// when both the intake and the facade run.
pub(super) fn workers(
    settings: &Settings,
    state: &AppState,
) -> Option<Arc<crate::inbound::worker::Pool>> {
    let (Some(inbound), Some(store), Some(sources), Some(facade)) = (
        settings.inbound,
        state.store(),
        state.sources(),
        state.facade(),
    ) else {
        return None;
    };
    let handler = crate::inbound::apply::Apply::new(
        Arc::new(crate::inbound::resolve::Declared::new(Arc::clone(sources))),
        facade.settings().subject.identity_handle(),
        facade.client().clone(),
        facade.store_handle(),
    )
    .with_access_log(state.access_log().cloned());
    Some(Arc::new(crate::inbound::worker::Pool::new(
        crate::inbound::store::InboxStore::new(store.pool().clone()),
        Arc::clone(sources),
        Arc::new(handler),
        inbound.workers,
    )))
}

/// Returns `state` serving the inbound sources of its state store, the
/// `[secrets]` directory they name opened.
pub(super) fn open_sources(settings: &Settings, state: AppState) -> anyhow::Result<AppState> {
    use anyhow::Context;

    let Some(store) = state.store() else {
        return Ok(state);
    };
    let translator = state.terminology().cloned().map(|client| {
        let translator: Arc<dyn crate::structuremap::engine::translate::Translate> =
            Arc::new(crate::structuremap::engine::translate::Terminology::new(
                client,
                tokio::runtime::Handle::current(),
            ));
        translator
    });
    let sources = Arc::new(
        crate::sources::Sources::open(store, settings.secrets.as_ref())
            .context("opening the [secrets] directory the inbound sources name")?
            .with_structuremaps(Arc::new(crate::structuremap::Running::new(translator))),
    );
    let intake = settings.inbound.map(|inbound| {
        crate::inbound::intake::Intake::new(
            Arc::clone(&sources),
            crate::inbound::store::InboxStore::new(store.pool().clone()),
            inbound.body_limit,
        )
        .with_recoverer(Arc::new(crate::inbound::recover::Recoverer::new(
            store.clone(),
            Arc::clone(&sources),
            inbound.workers.lease,
        )))
    });
    let state = state.with_sources(sources);
    Ok(match intake {
        Some(intake) => state.with_intake(Arc::new(intake)),
        None => state,
    })
}
