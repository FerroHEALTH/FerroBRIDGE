// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The resolution of `[auth]` and the operator scope into the settings of the
//! resource server.

use std::sync::Arc;

use ferrohealth_smart::config::AuthSection;

use super::error::Error;
use super::section::Operator;
use crate::smart::SmartSettings;

/// Returns the resource server settings `auth` describes, `None` when it is
/// off.
///
/// `ferrohealth-smart` checks the section: the issuer URL rule, at least one
/// audience, asymmetric algorithms only, the clock skew bound and every
/// Basic user. The bridge adds its own two rules: every capability is a
/// single scope token (RFC 6749 §3.3), and the operator scope is one too, in
/// a form SMART App Launch 2.2.0, Scopes and Launch Context, §Extensibility,
/// gives an extension scope: a full URI, or a string starting with `__`.
pub(super) fn resolve_auth(
    auth: &AuthSection,
    operator: &Operator,
) -> Result<Option<SmartSettings>, Error> {
    let resolved = auth.resolve().map_err(|source| Error::Auth { source })?;
    if !resolved.enabled {
        return Ok(None);
    }
    if !resolved
        .capabilities
        .iter()
        .all(|capability| is_scope_token(capability))
    {
        return Err(Error::Scope {
            key: String::from("auth.capabilities"),
        });
    }
    if !is_extension_scope(&operator.scope) {
        return Err(Error::Scope {
            key: String::from("operator.scope"),
        });
    }
    Ok(Some(SmartSettings {
        issuers: resolved.issuers,
        basic: resolved.basic.map(Arc::new),
        realm: resolved.realm,
        capabilities: resolved.capabilities,
        operator_scope: operator.scope.clone(),
    }))
}

/// Returns whether `token` is one scope token of RFC 6749 §3.3:
/// `1*( %x21 / %x23-5B / %x5D-7E )`.
pub(super) fn is_scope_token(token: &str) -> bool {
    !token.is_empty()
        && token
            .bytes()
            .all(|byte| matches!(byte, 0x21 | 0x23..=0x5B | 0x5D..=0x7E))
}

/// Returns whether `scope` is a scope token in an extension form SMART App
/// Launch 2.2.0, Scopes and Launch Context, §Extensibility, names: "a full
/// URI that you control", or "any string starting with `__`".
fn is_extension_scope(scope: &str) -> bool {
    let underscored = scope.len() > 2 && scope.starts_with("__");
    // NOTE: a scope that does not parse as a URL is legitimately no URI form,
    // so the failed parse is the answer and the `__` form decides.
    let uri = url::Url::parse(scope).is_ok_and(|url| url.has_host());
    is_scope_token(scope) && (underscored || uri)
}

#[cfg(test)]
mod tests {
    use super::is_extension_scope;

    #[test]
    fn an_extension_scope_is_a_full_uri_or_starts_with_two_underscores() {
        for scope in [
            "__ferrobridge.operator",
            "https://bridge.example.org/scope/operator",
        ] {
            assert!(is_extension_scope(scope), "{scope}");
        }
        for scope in [
            "ferrobridge.operator",
            "bridge:operator",
            "__",
            "__two words",
            "",
        ] {
            assert!(!is_extension_scope(scope), "{scope}");
        }
    }
}
