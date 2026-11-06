// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The resolution of `[auth]` into the settings of the resource server.

use std::sync::Arc;

use ferrohealth_smart::config::AuthSection;
use ferrohealth_smart::config::ConfigError;

use super::error::Error;
use crate::smart::OPERATOR_SCOPE;
use crate::smart::SmartSettings;

/// The context of a SMART backend-services scope, granted to a client with
/// no user (SMART App Launch 2.2.0, Scopes and Launch Context, §Scopes for
/// requesting clinical data).
const SYSTEM_CONTEXT: &str = "system/";

/// Returns the resource server settings `auth` describes, `None` when it is
/// off.
///
/// `ferrohealth-smart` checks the section: every `[[auth.issuer]]` with its
/// `kind`, its audience, its asymmetric algorithms and its profile, the
/// removed section-level keys and the flat `issuer` form, every Basic user,
/// every capability against SMART App Launch 2.2.0, Conformance,
/// §Capabilities, every listed scope, and the product scope in an extension
/// form of Scopes and Launch Context, §Extensibility. The product scope is
/// the bridge's operator scope, [`OPERATOR_SCOPE`] when the section names
/// none. An issuer with `mode = "introspection"` is checked by RFC 7662 in
/// place of its key set. A section the crate resolves to Basic users alone
/// is refused: the bridge names an issuer's token endpoint in its discovery
/// document and its `CapabilityStatement`. A Basic user holding a `system/`
/// scope is refused: the crate classifies every Basic credential as a
/// person, so such a scope would name a machine the caller is not.
pub(super) fn resolve_auth(auth: &AuthSection) -> Result<Option<SmartSettings>, Error> {
    let resolved = auth.resolve().map_err(|source| Error::Auth {
        source: Box::new(source),
    })?;
    if !resolved.enabled {
        return Ok(None);
    }
    // NOTE: SMART App Launch 2.2.0, Conformance, Metadata, `token_endpoint` is
    // required, and only an issuer's metadata names it for the bridge.
    if resolved.issuers.is_empty() && resolved.introspection.is_empty() {
        return Err(Error::Auth {
            source: Box::new(ConfigError::Missing { key: "auth.issuer" }),
        });
    }
    if let Some(basic) = &auth.basic {
        for user in &basic.users {
            if let Some(scope) = user
                .scopes
                .iter()
                .find(|scope| scope.starts_with(SYSTEM_CONTEXT))
            {
                return Err(Error::BasicSystemScope {
                    user: user.user.clone(),
                    scope: scope.clone(),
                });
            }
        }
    }
    let options = resolved.smart_options();
    Ok(Some(SmartSettings {
        issuers: resolved.issuers,
        introspection: resolved.introspection,
        basic: resolved.basic.map(Arc::new),
        realm: resolved.realm,
        options,
        operator_scope: resolved
            .product_scope
            .unwrap_or_else(|| String::from(OPERATOR_SCOPE)),
    }))
}
