// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! What the pages read from the bridge, each document at most once per
//! request.
//!
//! A page asks for a document through its server function ([`info_now`],
//! [`state_now`] and the others) under a `Resource`. In the server's own
//! render the server function is a plain call, and in the browser a call to
//! the console's server, so a page drawn in the browser reads what the
//! server's render read. On the server a page and the shell around it often
//! read the same document (the shell's status pill reads the bridge's info,
//! its navigation the running load), so every document a request reads is
//! kept in the request's `cached::Reads` and asked of the bridge once. Each read
//! checks the session first and uses the operator's own access token;
//! nothing outlives the request. No specification governs the console: our
//! own design.
#![cfg_attr(
    target_arch = "wasm32",
    allow(
        clippy::unused_async_trait_impl,
        reason = "the server function macro writes the browser half's call as an async trait method; only that target has it"
    )
)]

use ferrobridge_operator_api::capability::Capability;
use ferrobridge_operator_api::configuration::Configuration;
use ferrobridge_operator_api::health::{Info, Readiness};
use ferrobridge_operator_api::operator::{EngineState, Listed};
use leptos::prelude::*;
use leptos::server_fn::ServerFn;

use crate::model::{OpenedFile, Outcome, ViewError};

/// The count of the changes the operator made in the page, which every
/// read keys on, so a read drawn after a reload or a save asks again.
#[derive(Debug, Clone, Copy)]
pub struct Changes(pub RwSignal<u32>);

/// The count of the operator's changes, tracked; zero outside the page tree.
#[must_use]
pub fn changes() -> u32 {
    use_context::<Changes>().map_or(0, |changes| changes.0.get())
}

/// Counts one more change, so every read drawn in the page asks again.
pub fn changed() {
    if let Some(Changes(changes)) = use_context::<Changes>() {
        changes.update(|count| *count = count.wrapping_add(1));
    }
}

/// Counts a change each time `action` resolves, so the reads drawn after a
/// reload or a save ask the bridge again, and counts the answer as drawn
/// ([`drawn_after`]).
pub fn follow<A>(action: ServerAction<A>)
where
    A: ServerFn + Clone + Send + Sync + 'static,
    A::Output: Send + Sync + 'static,
    A::Error: Send + Sync + 'static,
{
    Effect::watch(
        move || action.version().get(),
        move |_, _, _| {
            changed();
            ferrohealth_ui::widget::submit::drawn();
        },
        false,
    );
}

/// Counts each answer of `action` the page draws in place as a drawn post
/// (`ferrohealth_ui::widget::submit::drawn`), so a busy search box ends its
/// wait.
pub fn drawn_after<A>(action: ServerAction<A>)
where
    A: ServerFn + Clone + Send + Sync + 'static,
    A::Output: Send + Sync + 'static,
    A::Error: Send + Sync + 'static,
{
    Effect::watch(
        move || action.version().get(),
        move |_, _, _| ferrohealth_ui::widget::submit::drawn(),
        false,
    );
}

/// The bridge's build facts, pins and lanes, `GET /health/info`.
///
/// # Errors
/// Returns [`ViewError::SignedOut`] without a live session, the console's
/// other faults as their [`ViewError`], and [`ViewError::Unavailable`] when
/// the console's server does not answer.
#[server(endpoint = "info")]
pub async fn info_now() -> Result<Outcome<Info>, ViewError> {
    cached::info().await
}

/// The bridge's readiness, `GET /health/readiness`.
///
/// # Errors
/// As [`info_now`].
#[server(endpoint = "readiness")]
pub async fn readiness_now() -> Result<Outcome<Readiness>, ViewError> {
    cached::readiness().await
}

/// The running load, `GET /operator/state`.
///
/// # Errors
/// As [`info_now`].
#[server(endpoint = "state")]
pub async fn state_now() -> Result<Outcome<EngineState>, ViewError> {
    cached::state().await
}

/// Every mapping file, `GET /operator/mappings`.
///
/// # Errors
/// As [`info_now`].
#[server(endpoint = "listing")]
pub async fn listing_now() -> Result<Outcome<Vec<Listed>>, ViewError> {
    cached::listing().await
}

/// The bridge's `CapabilityStatement`, `GET /fhir/metadata`, reduced to
/// what a page draws of it.
///
/// # Errors
/// As [`info_now`].
#[server(endpoint = "capability")]
pub async fn capability_now() -> Result<Outcome<Capability>, ViewError> {
    cached::capability().await
}

/// The configuration the bridge started on, `GET /operator/configuration`.
///
/// # Errors
/// As [`info_now`].
#[server(endpoint = "configuration")]
pub async fn configuration_now() -> Result<Outcome<Configuration>, ViewError> {
    cached::configuration().await
}

/// The bridge's SMART discovery document, `GET
/// /fhir/.well-known/smart-configuration`.
///
/// # Errors
/// As [`info_now`].
#[server(endpoint = "smart")]
pub async fn smart_now() -> Result<Outcome<serde_json::Value>, ViewError> {
    cached::smart().await
}

/// One mapping file, `GET /operator/mappings/{path}`, as text.
///
/// # Errors
/// As [`info_now`].
#[server(endpoint = "file")]
pub async fn file_now(
    /// The file's wire path.
    path: String,
) -> Result<Outcome<OpenedFile>, ViewError> {
    cached::file(&path).await.map(|outcome| {
        outcome.map(|file| OpenedFile {
            text: String::from_utf8(file.bytes).ok(),
            etag: file.etag,
        })
    })
}

/// The reads the server makes, kept once per request.
#[cfg(not(target_arch = "wasm32"))]
pub mod cached {
    use std::sync::Arc;

    use ferrobridge_operator_api::configuration::Configuration;
    use ferrobridge_operator_api::health::{Info, Readiness};
    use ferrobridge_operator_api::operator::{EngineState, Listed};
    use leptos::context::use_context;
    use tokio::sync::OnceCell;

    use ferrobridge_operator_api::capability::{Capability, Resource};
    use fhir_types::r4::capability_statement::CapabilityStatement;

    use crate::bridge::MappingFile;
    use crate::model::{Loaded, loaded};

    /// The documents one request has read from the bridge.
    #[derive(Debug, Clone, Default)]
    pub struct Reads {
        /// The build facts, pins and lanes.
        info: Arc<OnceCell<Loaded<Info>>>,
        /// The readiness of every indicator.
        readiness: Arc<OnceCell<Loaded<Readiness>>>,
        /// The running load.
        state: Arc<OnceCell<Loaded<EngineState>>>,
        /// The mapping files.
        listing: Arc<OnceCell<Loaded<Vec<Listed>>>>,
        /// The `CapabilityStatement`.
        capability: Arc<OnceCell<Loaded<Capability>>>,
        /// The running configuration.
        configuration: Arc<OnceCell<Loaded<Configuration>>>,
    }

    impl Reads {
        /// A fresh set for one request.
        #[must_use]
        pub fn new() -> Self {
            Self::default()
        }
    }

    /// The reads of the request being served, a fresh set when it carries
    /// none.
    fn reads() -> Reads {
        use_context::<Reads>().unwrap_or_default()
    }

    /// The bridge's build facts, pins and lanes, `GET /health/info`.
    ///
    /// # Errors
    /// Returns [`crate::model::ViewError::SignedOut`] without a live session, and the
    /// console's other faults as their [`crate::model::ViewError`].
    pub async fn info() -> Loaded<Info> {
        let cell = reads().info;
        cell.get_or_init(|| async {
            let (state, token) = crate::session::request::signed_in()?;
            loaded(state.bridge().health_info(&token).await)
        })
        .await
        .clone()
    }

    /// The bridge's readiness, `GET /health/readiness`; the document carries
    /// each indicator's state, which is what a page draws.
    ///
    /// # Errors
    /// As [`info`].
    pub async fn readiness() -> Loaded<Readiness> {
        let cell = reads().readiness;
        cell.get_or_init(|| async {
            let (state, token) = crate::session::request::signed_in()?;
            loaded(state.bridge().readiness(&token).await)
                .map(|outcome| outcome.map(|(_status, readiness)| readiness))
        })
        .await
        .clone()
    }

    /// The running load, `GET /operator/state`.
    ///
    /// # Errors
    /// As [`info`].
    pub async fn state() -> Loaded<EngineState> {
        let cell = reads().state;
        cell.get_or_init(|| async {
            let (state, token) = crate::session::request::signed_in()?;
            loaded(state.bridge().operator_state(&token).await)
        })
        .await
        .clone()
    }

    /// Every mapping file, `GET /operator/mappings`.
    ///
    /// # Errors
    /// As [`info`].
    pub async fn listing() -> Loaded<Vec<Listed>> {
        let cell = reads().listing;
        cell.get_or_init(|| async {
            let (state, token) = crate::session::request::signed_in()?;
            loaded(state.bridge().mapping_files(&token).await)
        })
        .await
        .clone()
    }

    /// The bridge's `CapabilityStatement`, `GET /fhir/metadata`, reduced to
    /// what a page draws of it.
    ///
    /// # Errors
    /// As [`info`].
    pub async fn capability() -> Loaded<Capability> {
        let cell = reads().capability;
        cell.get_or_init(|| async {
            let (state, token) = crate::session::request::signed_in()?;
            loaded(state.bridge().capability_statement(&token).await)
                .map(|outcome| outcome.map(|statement| reduced(&statement)))
        })
        .await
        .clone()
    }

    /// What `statement` declares, as a page draws it: its FHIR version and
    /// each resource of its `rest` blocks (FHIR R4 `capabilitystatement.html`).
    #[must_use]
    pub fn reduced(statement: &CapabilityStatement) -> Capability {
        let words = |values: Vec<Option<String>>| values.into_iter().flatten().collect();
        Capability {
            fhir_version: statement.fhir_version.value.clone(),
            resources: statement
                .rest
                .iter()
                .flat_map(|rest| rest.resource.iter())
                .map(|resource| Resource {
                    r#type: resource.r#type.value.clone().unwrap_or_default(),
                    supported_profiles: u32::try_from(resource.supported_profile.len())
                        .unwrap_or(u32::MAX),
                    interactions: words(
                        resource
                            .interaction
                            .iter()
                            .map(|interaction| interaction.code.value.clone())
                            .collect(),
                    ),
                    search_params: words(
                        resource
                            .search_param
                            .iter()
                            .map(|param| param.name.value.clone())
                            .collect(),
                    ),
                    operations: words(
                        resource
                            .operation
                            .iter()
                            .map(|operation| operation.name.value.clone())
                            .collect(),
                    ),
                })
                .collect(),
        }
    }

    /// The configuration the bridge started on, `GET /operator/configuration`.
    ///
    /// # Errors
    /// As [`info`].
    pub async fn configuration() -> Loaded<Configuration> {
        let cell = reads().configuration;
        cell.get_or_init(|| async {
            let (state, token) = crate::session::request::signed_in()?;
            loaded(state.bridge().configuration(&token).await)
        })
        .await
        .clone()
    }

    /// The bridge's SMART discovery document, `GET
    /// /fhir/.well-known/smart-configuration`.
    ///
    /// # Errors
    /// As [`info`].
    pub async fn smart() -> Loaded<serde_json::Value> {
        let (state, token) = crate::session::request::signed_in()?;
        loaded(state.bridge().smart_configuration(&token).await)
    }

    /// One mapping file's verbatim bytes, `GET /operator/mappings/{path}`.
    ///
    /// # Errors
    /// As [`info`].
    pub async fn file(path: &str) -> Loaded<MappingFile> {
        let (state, token) = crate::session::request::signed_in()?;
        loaded(state.bridge().mapping_file(path, &token).await)
    }
}
