// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The resolution of `[auth]` into the settings of the resource server.

use std::sync::Arc;

use ferrohealth_smart::config::AuthSection;

use super::error::Error;
use crate::smart::OPERATOR_SCOPE;
use crate::smart::SmartSettings;

/// Returns the resource server settings `auth` describes, `None` when it is
/// off.
///
/// `ferrohealth-smart` checks the section: the issuer URL rule, at least one
/// audience, asymmetric algorithms only, the clock skew bound, every Basic
/// user, every capability against SMART App Launch 2.2.0, Conformance,
/// §Capabilities, every listed scope, and the product scope in an extension
/// form of Scopes and Launch Context, §Extensibility. The product scope is
/// the bridge's operator scope, [`OPERATOR_SCOPE`] when the section names
/// none.
pub(super) fn resolve_auth(auth: &AuthSection) -> Result<Option<SmartSettings>, Error> {
    let resolved = auth.resolve().map_err(|source| Error::Auth {
        source: Box::new(source),
    })?;
    if !resolved.enabled {
        return Ok(None);
    }
    let options = resolved.smart_options();
    Ok(Some(SmartSettings {
        issuers: resolved.issuers,
        basic: resolved.basic.map(Arc::new),
        realm: resolved.realm,
        options,
        operator_scope: resolved
            .product_scope
            .unwrap_or_else(|| String::from(OPERATOR_SCOPE)),
    }))
}
