// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The bridge's status as the signed-in operator reads it: the server
//! function that asks the bridge for its `CapabilityStatement` with the
//! operator's own access token.
//!
//! The server function is a public HTTP endpoint of the console as well as
//! a call the server makes while it renders, so it checks the session itself
//! before it asks the bridge anything (the Leptos book,
//! `server/25_server_functions`).

use leptos::prelude::*;

use crate::model::{BridgeView, Outcome, ViewError};

/// Reads what the bridge says of itself, `GET {base}fhir/metadata`, as the
/// signed-in operator.
///
/// # Errors
/// Returns [`ViewError::SignedOut`] without a live session and the console's
/// other faults as their [`ViewError`]; the bridge's refusal is an
/// [`Outcome::Refused`].
#[server(endpoint = "bridge-status")]
pub async fn bridge_status() -> Result<Outcome<BridgeView>, ViewError> {
    server::bridge_status().await
}

/// The server half of [`bridge_status`].
#[cfg(not(target_arch = "wasm32"))]
pub mod server {
    use fhir_types::r4::capability_statement::CapabilityStatement;

    use crate::model::{BridgeView, Outcome, ViewError};

    /// Checks the session, then asks the bridge as the operator.
    ///
    /// # Errors
    /// As [`crate::status::bridge_status`].
    pub async fn bridge_status() -> Result<Outcome<BridgeView>, ViewError> {
        let (state, token) = crate::session::request::signed_in()?;
        match state.bridge().capability_statement(&token).await {
            Ok(statement) => Ok(Outcome::Shown(status(&statement))),
            Err(error) => crate::model::server::refusal(&error).map(Outcome::Refused),
        }
    }

    /// The status `statement` describes.
    #[must_use]
    pub fn status(statement: &CapabilityStatement) -> BridgeView {
        let software = statement.software.as_ref();
        BridgeView {
            software: software.and_then(|software| software.name.value.clone()),
            version: software
                .and_then(|software| software.version.as_ref())
                .and_then(|version| version.value.clone()),
            fhir_version: statement.fhir_version.value.clone(),
        }
    }
}
