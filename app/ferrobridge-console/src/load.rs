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

use ferrobridge_operator_api::configuration::Configuration;
use ferrobridge_operator_api::health::{Info, Readiness};
use ferrobridge_operator_api::operator::{EngineState, Listed};
use fhir_types::codec::{Json as _, Path, Value};
use fhir_types::r4::capability_statement::CapabilityStatement;
use leptos::prelude::*;
use leptos::server_fn::ServerFn;

use crate::model::{Loaded, OpenedFile, Outcome, ViewError};

/// The bridge's `CapabilityStatement` with the JSON it arrived in.
pub type Capability = (CapabilityStatement, serde_json::Value);

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
/// reload or a save ask the bridge again.
pub fn follow<A>(action: ServerAction<A>)
where
    A: ServerFn + Clone + Send + Sync + 'static,
    A::Output: Send + Sync + 'static,
    A::Error: Send + Sync + 'static,
{
    Effect::watch(
        move || action.version().get(),
        move |_, _, _| changed(),
        false,
    );
}

/// The `CapabilityStatement` the JSON `raw` holds, with that JSON.
///
/// # Errors
/// Returns [`ViewError::Unreadable`] for JSON that is no R4
/// `CapabilityStatement` (FHIR R4 `capabilitystatement.html`).
pub fn capability_of(raw: serde_json::Value) -> Result<Capability, ViewError> {
    let Value::Object(object) = Value::from_serde_json(raw.clone()) else {
        return Err(ViewError::Unreadable);
    };
    CapabilityStatement::from_json(&object, &mut Path::root("CapabilityStatement"))
        .map(|statement| (statement, raw))
        .map_err(|_error| ViewError::Unreadable)
}

/// The bridge's `CapabilityStatement` as a page draws it, from the answer
/// of [`capability_now`].
///
/// # Errors
/// As [`capability_of`], and the fault or refusal `answer` carries.
pub fn capability(answer: Loaded<serde_json::Value>) -> Loaded<Capability> {
    match answer? {
        Outcome::Shown(raw) => capability_of(raw).map(Outcome::Shown),
        Outcome::Refused(refusal) => Ok(Outcome::Refused(refusal)),
    }
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

/// The JSON of the bridge's `CapabilityStatement`, `GET /fhir/metadata`,
/// which [`capability`] reads.
///
/// # Errors
/// As [`info_now`].
#[server(endpoint = "capability")]
pub async fn capability_now() -> Result<Outcome<serde_json::Value>, ViewError> {
    cached::capability()
        .await
        .map(|outcome| outcome.map(|(_statement, raw)| raw))
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

    use crate::bridge::MappingFile;
    use crate::load::Capability;
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
    /// Returns [`ViewError::SignedOut`] without a live session, and the
    /// console's other faults as their [`ViewError`].
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

    /// The bridge's `CapabilityStatement`, `GET /fhir/metadata`.
    ///
    /// # Errors
    /// As [`info`].
    pub async fn capability() -> Loaded<Capability> {
        let cell = reads().capability;
        cell.get_or_init(|| async {
            let (state, token) = crate::session::request::signed_in()?;
            loaded(state.bridge().capability(&token).await)
        })
        .await
        .clone()
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
