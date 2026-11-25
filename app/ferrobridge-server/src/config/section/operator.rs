// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The `[operator]` section.
//!
//! No specification governs the configuration: our own design.

use serde::{Deserialize, Serialize};

/// The operator API: the engine state and the mapping files, read under the
/// operator scope, `[auth] product_scope`.
///
/// Off until `enabled` is set, and refused at boot while `[auth]` is off, so
/// the API is never served to an unauthenticated caller. No specification
/// governs the operator surface: our own design.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct Operator {
    /// Whether the operator routes are mounted under `/operator`; with
    /// `[state]`, they save the console's mapping files there.
    pub enabled: bool,
}
