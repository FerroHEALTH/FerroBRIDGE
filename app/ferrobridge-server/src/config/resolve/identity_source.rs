// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The resolution of `[identity_source]` into the identity source every EHR
//! subject is asked of (`PIXm` 3.1.0 ITI-83).
//!
//! No specification governs the configuration: our own design.

use std::sync::Arc;

use crate::config::Config;
use crate::config::error::Error;
use crate::identity_source::IdentitySource;

/// Returns the identity source `config` names, refused while the facade is
/// on without one; `None` without the section.
pub(in crate::config) fn resolve(config: &Config) -> Result<Option<Arc<IdentitySource>>, Error> {
    match config.identity_source.as_ref() {
        Some(section) => {
            crate::config::identity_source::resolve(section).map(|source| Some(Arc::new(source)))
        }
        None if config.facade.enabled => Err(Error::Needs {
            key: String::from("facade.enabled"),
            needs: String::from("[identity_source]"),
        }),
        None => Ok(None),
    }
}
