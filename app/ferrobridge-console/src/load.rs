// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! What a page reads from the bridge, each document at most once per
//! request.
//!
//! A page and the shell around it often read the same document (the
//! shell's status pill reads the bridge's info, its navigation the running
//! load), so every document a request reads is kept in its [`Reads`] and
//! asked of the bridge once. Each read checks the session first and uses
//! the operator's own access token; nothing outlives the request. No
//! specification governs the console: our own design.

use std::sync::Arc;

use ferrobridge_operator_api::configuration::Configuration;
use ferrobridge_operator_api::health::Info;
use ferrobridge_operator_api::operator::{EngineState, Listed};
use ferrohealth_health::document::Readiness;
use tokio::sync::OnceCell;

use ferrobridge_operator_api::capability::{Capability, Resource};
use fhir_types::r4::capability_statement::CapabilityStatement;

use crate::bridge::{AccessToken, MappingFile};
use crate::model::{Loaded, OpenedFile, ViewError, loaded};
use crate::server::ConsoleState;

/// The documents one request has read from the bridge.
#[derive(Debug, Clone)]
pub struct Reads {
    /// The console's state, whose bridge client asks.
    state: ConsoleState,
    /// The operator's access token, or why the request has none.
    signed: Result<AccessToken, ViewError>,
    /// The build facts, pins and lanes.
    info: Arc<OnceCell<Loaded<Info>>>,
    /// The readiness of every indicator.
    readiness: Arc<OnceCell<Loaded<Readiness>>>,
    /// The running load.
    running: Arc<OnceCell<Loaded<EngineState>>>,
    /// The mapping files.
    listing: Arc<OnceCell<Loaded<Vec<Listed>>>>,
    /// The `CapabilityStatement`.
    capability: Arc<OnceCell<Loaded<Capability>>>,
    /// The running configuration.
    configuration: Arc<OnceCell<Loaded<Configuration>>>,
}

impl Reads {
    /// A fresh set for one request to the console `state`, as the operator
    /// whose token `signed` holds.
    #[must_use]
    pub fn new(state: ConsoleState, signed: Result<AccessToken, ViewError>) -> Self {
        Self {
            state,
            signed,
            info: Arc::default(),
            readiness: Arc::default(),
            running: Arc::default(),
            listing: Arc::default(),
            capability: Arc::default(),
            configuration: Arc::default(),
        }
    }

    /// The console's state and the operator's token, for a request with a
    /// live session.
    ///
    /// # Errors
    /// Returns [`ViewError::SignedOut`] without a live session, and
    /// [`ViewError::Unavailable`] when the session store cannot be read.
    pub fn signed_in(&self) -> Result<(&ConsoleState, &AccessToken), ViewError> {
        self.signed
            .as_ref()
            .map(|token| (&self.state, token))
            .map_err(Clone::clone)
    }

    /// The bridge's build facts, pins and lanes, `GET /health/info`.
    ///
    /// # Errors
    /// Returns [`ViewError::SignedOut`] without a live session, and the
    /// console's other faults as their [`ViewError`].
    pub async fn info(&self) -> Loaded<Info> {
        let cell = &self.info;
        cell.get_or_init(|| async {
            let (state, token) = self.signed_in()?;
            loaded(state.bridge().health_info(token).await)
        })
        .await
        .clone()
    }

    /// The bridge's readiness, `GET /health/readiness`; the document carries
    /// each indicator's state, which is what a page draws.
    ///
    /// # Errors
    /// As [`Self::info`].
    pub async fn readiness(&self) -> Loaded<Readiness> {
        let cell = &self.readiness;
        cell.get_or_init(|| async {
            let (state, token) = self.signed_in()?;
            loaded(state.bridge().readiness(token).await)
                .map(|outcome| outcome.map(|(_status, readiness)| readiness))
        })
        .await
        .clone()
    }

    /// The running load, `GET /operator/state`.
    ///
    /// # Errors
    /// As [`Self::info`].
    pub async fn running(&self) -> Loaded<EngineState> {
        let cell = &self.running;
        cell.get_or_init(|| async {
            let (state, token) = self.signed_in()?;
            loaded(state.bridge().operator_state(token).await)
        })
        .await
        .clone()
    }

    /// Every mapping file, `GET /operator/mappings`.
    ///
    /// # Errors
    /// As [`Self::info`].
    pub async fn listing(&self) -> Loaded<Vec<Listed>> {
        let cell = &self.listing;
        cell.get_or_init(|| async {
            let (state, token) = self.signed_in()?;
            loaded(state.bridge().mapping_files(token).await)
        })
        .await
        .clone()
    }

    /// The bridge's `CapabilityStatement`, `GET /fhir/metadata`, reduced to
    /// what a page draws of it.
    ///
    /// # Errors
    /// As [`Self::info`].
    pub async fn capability(&self) -> Loaded<Capability> {
        let cell = &self.capability;
        cell.get_or_init(|| async {
            let (state, token) = self.signed_in()?;
            loaded(state.bridge().capability_statement(token).await)
                .map(|outcome| outcome.map(|statement| reduced(&statement)))
        })
        .await
        .clone()
    }

    /// The configuration the bridge started on, `GET /operator/configuration`.
    ///
    /// # Errors
    /// As [`Self::info`].
    pub async fn configuration(&self) -> Loaded<Configuration> {
        let cell = &self.configuration;
        cell.get_or_init(|| async {
            let (state, token) = self.signed_in()?;
            loaded(state.bridge().configuration(token).await)
        })
        .await
        .clone()
    }

    /// The bridge's SMART discovery document, `GET
    /// /fhir/.well-known/smart-configuration`.
    ///
    /// # Errors
    /// As [`Self::info`].
    pub async fn smart(&self) -> Loaded<serde_json::Value> {
        let (state, token) = self.signed_in()?;
        loaded(state.bridge().smart_configuration(token).await)
    }

    /// One mapping file's verbatim bytes, `GET /operator/mappings/{path}`.
    ///
    /// # Errors
    /// As [`Self::info`].
    pub async fn file(&self, path: &str) -> Loaded<MappingFile> {
        let (state, token) = self.signed_in()?;
        loaded(state.bridge().mapping_file(path, token).await)
    }

    /// One mapping file, `GET /operator/mappings/{path}`, as text.
    ///
    /// # Errors
    /// As [`Self::info`].
    pub async fn opened(&self, path: &str) -> Loaded<OpenedFile> {
        self.file(path).await.map(|outcome| {
            outcome.map(|file| OpenedFile {
                text: String::from_utf8(file.bytes).ok(),
                etag: file.etag,
            })
        })
    }
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
