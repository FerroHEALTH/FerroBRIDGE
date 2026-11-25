// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The resolution of `[operator]` into the operator API settings.
//!
//! No specification governs the configuration: our own design.

use crate::config::Config;
use crate::config::error::Error;

/// Returns the operator API settings, refusing the API without `[auth]` so it
/// is never served to an unauthenticated caller.
///
/// # Errors
///
/// Returns [`Error::Needs`] when `[operator]` is on and `[auth]` is off.
pub(in crate::config) fn resolve(
    config: &Config,
) -> Result<Option<crate::config::OperatorSettings>, Error> {
    if !config.operator.enabled {
        return Ok(None);
    }
    super::needs("operator.enabled", &[(config.auth.enabled, "auth.enabled")])?;
    Ok(Some(crate::config::OperatorSettings))
}
