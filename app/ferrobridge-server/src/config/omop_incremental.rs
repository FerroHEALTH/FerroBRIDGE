// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The resolution of `[omop_incremental]` into the settings of the
//! incremental OMOP load.
//!
//! No specification governs the load: our own design.

use super::Config;
use super::error::Error;
use crate::change_feed::ReaderName;
use crate::etl::incremental::Settings;

/// Returns the load `config` turns on, refused without the change feed,
/// `[etl]`, `[cdm]` or an OMOCL directory it runs over, without a reader
/// name, or under the events lane's reader; `None` while it is off.
pub(super) fn resolve(config: &Config) -> Result<Option<Settings>, Error> {
    let section = &config.omop_incremental;
    if !section.enabled {
        return Ok(None);
    }
    let missing = [
        (config.change_feed.enabled, "change_feed.enabled"),
        (config.etl.is_some(), "[etl]"),
        (config.cdm.is_some(), "[cdm]"),
        (
            config.mappings.omocl.is_some()
                || (config.operator.enabled && config.operator.mapping_dir.is_some()),
            "[mappings] omocl",
        ),
    ]
    .into_iter()
    .find_map(|(on, needed)| (!on).then_some(needed));
    if let Some(needed) = missing {
        return Err(Error::Needs {
            key: String::from("omop_incremental.enabled"),
            needs: String::from(needed),
        });
    }
    if section.reader.is_empty() {
        return Err(Error::Missing {
            key: String::from("omop_incremental.reader"),
        });
    }
    let reader = ReaderName::new(&section.reader).map_err(|source| Error::ReaderName {
        key: String::from("omop_incremental.reader"),
        source,
    })?;
    if config.events.enabled && config.events.reader == section.reader {
        return Err(Error::Duplicate {
            key: String::from("omop_incremental.reader"),
            value: section.reader.clone(),
        });
    }
    Ok(Some(Settings { reader }))
}
