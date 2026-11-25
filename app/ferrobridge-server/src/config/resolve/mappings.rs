// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The resolution of `[mappings]` into the mapping-set paths.
//!
//! No specification governs the configuration: our own design.

use crate::config::Config;
use crate::config::MappingSettings;
use crate::config::error::Error;

/// Returns the mapping-set paths `mappings` names, when it names both.
///
/// A set with no `templates` resolves to `None` here and is carried by
/// [`Settings::mapping_directory`] alone: the facade and the two operations
/// then take their templates from the CDR at boot. A `templates` with no
/// `directory` names nothing to compile.
///
/// # Errors
///
/// Returns [`Error::Missing`] when `templates` is set and `directory` is not.
///
/// [`Settings::mapping_directory`]: crate::config::Settings::mapping_directory
pub(in crate::config) fn resolve(config: &Config) -> Result<Option<MappingSettings>, Error> {
    let mappings = &config.mappings;
    match (mappings.directory.as_ref(), mappings.templates.as_ref()) {
        (Some(directory), Some(templates)) => Ok(Some(MappingSettings {
            directory: directory.clone(),
            templates: templates.clone(),
        })),
        (None, Some(_)) => Err(Error::Missing {
            key: String::from("mappings.directory"),
        }),
        _ => Ok(None),
    }
}
