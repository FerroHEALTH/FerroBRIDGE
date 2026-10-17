// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The check of one bearer access token.
//!
//! The header names an algorithm of the configured set and a key of the
//! issuer's set; the signature verifies with that key (RFC 7515 §5.2); `iss`
//! is the configured issuer and `aud` contains the configured audience
//! (RFC 9068 §4); `exp`, `nbf` and `iat` hold within the clock skew
//! (RFC 7519 §4.1.4, §4.1.5, §4.1.6). Every failure is the same
//! [`Rejection::Invalid`] on the wire; its reason reaches only the debug log.

use std::sync::Arc;

use jsonwebtoken::Validation;
use jsonwebtoken::errors::ErrorKind;
use serde::Deserialize;

use crate::auth::Authority;
use crate::auth::Caller;
use crate::auth::Rejection;
use crate::auth::TokenProfile;

/// The `typ` values RFC 9068 §4 accepts.
const ACCESS_TOKEN_TYPES: [&str; 2] = ["at+jwt", "application/at+jwt"];

/// The claims the resource server reads beside the registered ones the
/// validation checks.
#[derive(Deserialize)]
struct Claims {
    /// The subject.
    sub: Option<String>,
    /// The client the token was issued to (RFC 9068 §2.2, RFC 8693 §4.3).
    client_id: Option<String>,
    /// The issue time.
    iat: Option<i64>,
    /// The token identifier.
    jti: Option<String>,
    /// The granted scopes, space-delimited (RFC 9068 §2.2.3).
    scope: Option<String>,
    /// The SMART `patient` launch context.
    patient: Option<String>,
}

/// Returns the caller `token` authenticates under `authority`.
pub(super) async fn validate(authority: &Authority, token: &str) -> Result<Caller, Rejection> {
    let settings = authority.settings();
    let header = jsonwebtoken::decode_header(token).map_err(|_malformed| invalid("malformed"))?;
    if !settings.algorithms.contains(&header.alg) {
        return Err(invalid("algorithm not accepted"));
    }
    if settings.profile == TokenProfile::Rfc9068
        && !header.typ.as_deref().is_some_and(|typ| {
            ACCESS_TOKEN_TYPES
                .iter()
                .any(|accepted| typ.eq_ignore_ascii_case(accepted))
        })
    {
        return Err(invalid("typ is not at+jwt"));
    }
    let key = authority
        .keys
        .find(header.kid.as_deref(), header.alg)
        .await
        .map_err(|source| Rejection::Unavailable {
            source: Arc::new(source),
        })?
        .ok_or_else(|| invalid("no key of the set"))?;
    let mut validation = Validation::new(header.alg);
    validation.set_issuer(&[settings.issuer.as_str()]);
    validation.set_audience(&[settings.audience.as_str()]);
    validation.set_required_spec_claims(&["exp", "iss", "aud", "sub"]);
    validation.validate_nbf = true;
    validation.leeway = settings.clock_skew.as_secs();
    let data = jsonwebtoken::decode::<Claims>(token, &key, &validation)
        .map_err(|error| invalid(reason(error.kind())))?;
    let claims = data.claims;
    if let Some(issued) = claims.iat {
        let latest = jiff::Timestamp::now()
            .as_second()
            .saturating_add(i64::try_from(settings.clock_skew.as_secs()).unwrap_or(i64::MAX));
        if issued > latest {
            return Err(invalid("iat is in the future"));
        }
    }
    if settings.profile == TokenProfile::Rfc9068
        && (claims.iat.is_none() || claims.jti.is_none() || claims.client_id.is_none())
    {
        return Err(invalid("an RFC 9068 claim is missing"));
    }
    let subject = claims.sub.ok_or_else(|| invalid("sub is missing"))?;
    let scopeless = settings.scopeless_subjects.contains(&subject);
    let scope = claims.scope.as_deref().unwrap_or_default();
    // NOTE: RFC 6749 §3.3, the scope claim is space-delimited tokens; the
    // operator scope is one of them (no specification governs it: our own design).
    let operator = scope
        .split(' ')
        .any(|token| token == settings.operator_scope);
    Ok(
        Caller::new(subject, claims.client_id, scope, claims.patient, scopeless)
            .with_operator(operator),
    )
}

/// Returns the rejection of a token that does not hold for `reason`.
const fn invalid(reason: &'static str) -> Rejection {
    Rejection::Invalid { reason }
}

/// Returns the debug-log reason of a failed decode.
const fn reason(kind: &ErrorKind) -> &'static str {
    match kind {
        ErrorKind::InvalidSignature => "signature",
        ErrorKind::ExpiredSignature => "expired",
        ErrorKind::ImmatureSignature => "not yet valid",
        ErrorKind::InvalidIssuer => "issuer",
        ErrorKind::InvalidAudience => "audience",
        ErrorKind::MissingRequiredClaim(_) => "a required claim is missing",
        ErrorKind::InvalidAlgorithm => "algorithm",
        _ => "malformed",
    }
}
