// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The `[omop_incremental]` section: the OMOP load that follows the change
//! feed and commits each changed composition as `etl run` does.
//!
//! No specification governs the incremental load: our own design. It is off
//! until `enabled` is set, and it follows the `[change_feed]` source.

use serde::{Deserialize, Serialize};

/// The incremental OMOP load.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct OmopIncremental {
    /// Whether the load follows the change feed.
    pub enabled: bool,
    /// The reader name the load follows the change feed under.
    pub reader: String,
}
