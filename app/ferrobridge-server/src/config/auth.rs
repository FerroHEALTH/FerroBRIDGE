// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The resolution of `[auth]` into the settings of the resource server.

use std::sync::Arc;

use ferrohealth_smart::basic::PasswordStore;
use ferrohealth_smart::config::AuthSection;
use ferrohealth_smart::config::BasicGrants;
use ferrohealth_smart::config::ConfigError;

use super::error::Error;
use crate::smart::OPERATOR_SCOPE;
use crate::smart::SmartSettings;

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
/// none. Each Basic user is granted the scopes and roles `grants` holds
/// for its user-id. An issuer with `mode = "introspection"` is checked by RFC 7662 in
/// place of its key set. A section the crate resolves to Basic users alone
/// is refused: the bridge names an issuer's token endpoint in its discovery
/// document and its `CapabilityStatement`. The crate refuses a grant
/// naming a user `[[auth.basic.users]]` does not list, and a grant of a
/// `system/` scope, naming the user and the scope: a Basic credential is a
/// person's.
pub(super) fn resolve_auth(
    auth: &AuthSection,
    grants: &BasicGrants,
) -> Result<Option<SmartSettings>, Error> {
    let resolved = auth
        .resolve_with_tls(&ferrohealth_smart::config::ServerTls)
        .map_err(|source| Error::Auth {
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
    let options = resolved.smart_options();
    Ok(Some(SmartSettings {
        issuers: resolved.issuers,
        introspection: resolved.introspection,
        basic: granted(resolved.basic, grants)?.map(Arc::new),
        realm: resolved.realm,
        options,
        operator_scope: resolved
            .product_scope
            .unwrap_or_else(|| String::from(OPERATOR_SCOPE)),
    }))
}

/// Returns the Basic users of `store` with the scopes and roles of `grants`
/// granted, `None` when `[auth]` admits no Basic user and nothing is granted.
///
/// A grant naming a user the store does not admit is refused, also when
/// `[auth]` lists no Basic user at all, so a misspelt user-id never grants
/// silently (no specification governs this: our own design).
fn granted(
    store: Option<PasswordStore>,
    grants: &BasicGrants,
) -> Result<Option<PasswordStore>, Error> {
    let store = match store {
        Some(store) => store,
        None if grants.users.is_empty() => return Ok(None),
        None => PasswordStore::new(),
    };
    store
        .with_grants(grants)
        .map(Some)
        .map_err(|source| Error::BasicGrants { source })
}
