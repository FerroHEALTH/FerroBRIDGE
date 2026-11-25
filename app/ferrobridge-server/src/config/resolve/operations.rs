// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The resolution of `[operations]` into the FHIRconnect operations lane.
//!
//! No specification governs the configuration: our own design.

use crate::config::Config;
use crate::config::OperationsSettings;

/// Returns the operations lane of `[operations]`.
///
/// The section is always resolved, on or off: `etl fhir` writes its
/// compositions under the same device, composer, language and territory
/// whether or not the two operations are served.
pub(in crate::config) fn resolve(config: &Config) -> OperationsSettings {
    let operations = &config.operations;
    OperationsSettings {
        enabled: operations.enabled,
        device_reference: operations.device_reference.clone(),
        composer: operations.composer.clone(),
        composition_language: operations.composition_language.clone(),
        composition_territory: operations.composition_territory.clone(),
    }
}
