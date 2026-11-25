// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The resolution of `[omop_incremental]` into the settings of the
//! incremental OMOP load.
//!
//! No specification governs the load: our own design.

use super::change_feed::Consumer;
use crate::config::Config;
use crate::config::OmopIncrementalSettings;
use crate::config::error::Error;

/// Returns the load `config` turns on, refused without the change feed,
/// `[etl]`, `[cdm]` or an OMOCL directory it runs over, without a reader
/// name, or under the events lane's reader; `None` while it is off.
pub(in crate::config) fn resolve(
    config: &Config,
) -> Result<Option<OmopIncrementalSettings>, Error> {
    let section = &config.omop_incremental;
    if !section.enabled {
        return Ok(None);
    }
    super::needs(
        "omop_incremental.enabled",
        &[
            (config.change_feed.enabled, "change_feed.enabled"),
            (config.etl.is_some(), "[etl]"),
            (config.cdm.is_some(), "[cdm]"),
            (
                config.mappings.omocl.is_some()
                    || (config.operator.enabled && config.state.is_some()),
                "[mappings] omocl",
            ),
        ],
    )?;
    let reader = Consumer::OmopIncremental.reader(config)?;
    Ok(Some(OmopIncrementalSettings { reader }))
}
