// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The resolution of `[auth]` into the settings of the resource server.

use std::collections::BTreeSet;
use std::str::FromStr;
use std::time::Duration;

use jsonwebtoken::Algorithm;

use super::credentials::is_scope;
use super::error::Error;
use super::resolve::url_of;
use super::section::Auth;
use crate::auth::AuthSettings;
use crate::auth::TokenProfile;

/// Returns the resource server settings `auth` describes.
///
/// The issuer is an `https` URL free of a query and a fragment (RFC 8414
/// §2), the audience is set, every algorithm is an asymmetric JWS algorithm
/// (RFC 7518 §3.1: a symmetric key would be one the authorization server
/// shares with every resource server, and `none` signs nothing), and the
/// operator scope and each capability are single scope tokens (RFC 6749
/// §3.3).
pub(super) fn resolve_auth(auth: &Auth) -> Result<AuthSettings, Error> {
    let url = url_of("auth.issuer", &auth.issuer)?;
    if url.scheme() != "https" || url.query().is_some() || url.fragment().is_some() {
        return Err(Error::Issuer {
            key: String::from("auth.issuer"),
        });
    }
    if auth.audience.is_empty() {
        return Err(Error::Missing {
            key: String::from("auth.audience"),
        });
    }
    if auth.algorithms.is_empty() {
        return Err(Error::Missing {
            key: String::from("auth.algorithms"),
        });
    }
    let mut algorithms = Vec::new();
    for name in &auth.algorithms {
        let algorithm = Algorithm::from_str(name)
            .ok()
            .filter(|algorithm| algorithm.family() != jsonwebtoken::AlgorithmFamily::Hmac)
            .ok_or_else(|| Error::Algorithm {
                key: String::from("auth.algorithms"),
                value: name.clone(),
            })?;
        if !algorithms.contains(&algorithm) {
            algorithms.push(algorithm);
        }
    }
    let profile = match auth.token_profile.as_str() {
        "rfc9068" => TokenProfile::Rfc9068,
        "jwt" => TokenProfile::Jwt,
        _ => {
            return Err(Error::TokenProfile {
                key: String::from("auth.token_profile"),
                value: auth.token_profile.clone(),
            });
        }
    };
    let mut scopeless_subjects = BTreeSet::new();
    for subject in &auth.scopeless_subjects {
        if subject.is_empty() {
            return Err(Error::Missing {
                key: String::from("auth.scopeless_subjects"),
            });
        }
        scopeless_subjects.insert(subject.clone());
    }
    let single = |value: &str| is_scope(value) && !value.contains(' ');
    if !single(&auth.operator_scope) {
        return Err(Error::Scope {
            key: String::from("auth.operator_scope"),
        });
    }
    if !auth
        .capabilities
        .iter()
        .all(|capability| single(capability))
    {
        return Err(Error::Scope {
            key: String::from("auth.capabilities"),
        });
    }
    Ok(AuthSettings {
        issuer: auth.issuer.clone(),
        audience: auth.audience.clone(),
        algorithms,
        profile,
        clock_skew: Duration::from_secs(auth.clock_skew_seconds),
        jwks_refresh_interval: Duration::from_millis(auth.jwks_refresh_interval_ms),
        timeout: Duration::from_millis(auth.timeout_ms),
        scopeless_subjects,
        operator_scope: auth.operator_scope.clone(),
        capabilities: auth.capabilities.clone(),
    })
}
