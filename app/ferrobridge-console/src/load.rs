// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! What the pages read from the bridge, each document at most once per
//! request.
//!
//! A page and the shell around it often read the same document (the shell's
//! status pill reads the bridge's info, its navigation the running load), so
//! every document a request reads is kept in the request's [`Reads`] and
//! asked of the bridge once. Each read checks the session first and uses the
//! operator's own access token; nothing outlives the request. No
//! specification governs the console: our own design.

use std::sync::Arc;

use ferrobridge_operator_api::health::{Info, Readiness};
use ferrobridge_operator_api::operator::{EngineState, Listed};
use fhir_types::r4::capability_statement::CapabilityStatement;
use http::StatusCode;
use leptos::context::use_context;
use tokio::sync::OnceCell;

use crate::bridge::MappingFile;
use crate::model::{Loaded, ViewError, loaded};

/// The bridge's readiness with the status it answered.
pub type Ready = (StatusCode, Readiness);

/// The bridge's `CapabilityStatement` with the JSON it arrived in.
pub type Capability = (CapabilityStatement, serde_json::Value);

/// The documents one request has read from the bridge.
#[derive(Debug, Clone, Default)]
pub struct Reads {
    /// The build facts, pins and lanes.
    info: Arc<OnceCell<Loaded<Info>>>,
    /// The readiness of every indicator.
    readiness: Arc<OnceCell<Loaded<Ready>>>,
    /// The running load.
    state: Arc<OnceCell<Loaded<EngineState>>>,
    /// The mapping files.
    listing: Arc<OnceCell<Loaded<Vec<Listed>>>>,
    /// The `CapabilityStatement`.
    capability: Arc<OnceCell<Loaded<Capability>>>,
}

impl Reads {
    /// A fresh set for one request.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}

/// The reads of the request being rendered, a fresh set when the render
/// carries none.
fn reads() -> Reads {
    use_context::<Reads>().unwrap_or_default()
}

/// The bridge's build facts, pins and lanes, `GET /health/info`.
///
/// # Errors
/// Returns [`ViewError::SignedOut`] without a live session, and the console's
/// other faults as their [`ViewError`].
pub async fn info() -> Loaded<Info> {
    let cell = reads().info;
    cell.get_or_init(|| async {
        let (state, token) = crate::session::request::signed_in()?;
        loaded(state.bridge().health_info(&token).await)
    })
    .await
    .clone()
}

/// The bridge's readiness, `GET /health/readiness`.
///
/// # Errors
/// As [`info`].
pub async fn readiness() -> Loaded<Ready> {
    let cell = reads().readiness;
    cell.get_or_init(|| async {
        let (state, token) = crate::session::request::signed_in()?;
        loaded(state.bridge().readiness(&token).await)
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

/// Whether the request carries a live signed-in session.
///
/// # Errors
/// Returns the [`ViewError`] that keeps the request from the bridge.
pub fn admitted() -> Result<(), ViewError> {
    crate::session::request::signed_in().map(|_signed_in| ())
}
