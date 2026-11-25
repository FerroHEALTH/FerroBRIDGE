// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The `[operations]` section.
//!
//! No specification governs the configuration: our own design.

use serde::{Deserialize, Serialize};

/// The FHIRconnect operations lane.
///
/// The two operations are pure transformations that reach no CDR on a
/// request, so they are served whenever `[mappings] directory` is set and its
/// templates load, from `[mappings] templates` or, at boot, from the `[cdr]`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct Operations {
    /// Whether the two operations are served.
    pub enabled: bool,
    /// The `Device` reference the `Provenance` `agent.who` carries when a call
    /// supplies no `context.who`.
    pub device_reference: String,
    /// The composer an inbound run fills in when no mapping does.
    pub composer: String,
    /// The composition language, as an ISO 639-1 code.
    pub composition_language: Option<String>,
    /// The composition territory, as an ISO 3166-1 alpha-2 code.
    pub composition_territory: Option<String>,
}

impl Default for Operations {
    fn default() -> Self {
        Self {
            enabled: true,
            device_reference: format!(
                "Device/{}-{}",
                crate::build_info::PRODUCT.to_ascii_lowercase(),
                crate::build_info::VERSION
            ),
            composer: String::from("FHIRconnect"),
            composition_language: None,
            composition_territory: None,
        }
    }
}
