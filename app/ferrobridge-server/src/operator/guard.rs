// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The middleware in front of every operator route: the token, the operator
//! scope and the audit line.
//!
//! A request without a valid bearer token answers `401` and one whose token
//! does not carry `auth.operator_scope` answers `403` (RFC 6750 §3.1). The
//! audit line names the caller and the matched route, never a path value or
//! a body. No specification governs the operator surface: our own design.

use std::sync::Arc;
use std::time::Instant;

use axum::extract::MatchedPath;
use axum::extract::Request;
use axum::extract::State;
use axum::middleware::Next;
use axum::response::IntoResponse;
use axum::response::Response;
use http::StatusCode;
use http::header;

use crate::auth::Authority;
use crate::auth::Caller;
use crate::auth::Rejection;
use crate::auth::guard::AUDIT_TARGET;

/// The caller an operator request was admitted with, left on the request
/// for a handler that records who changed what.
#[derive(Debug, Clone)]
pub struct Operator(pub Arc<Caller>);

/// Checks the token and the operator scope of `request`, and writes the audit
/// line once the response is ready.
pub async fn guard(
    State(authority): State<Arc<Authority>>,
    mut request: Request,
    next: Next,
) -> Response {
    let started = Instant::now();
    let method = request.method().clone();
    let route = request
        .extensions()
        .get::<MatchedPath>()
        .map_or_else(String::new, |matched| matched.as_str().to_owned());
    let request_id = crate::request_id::of(request.headers())
        .unwrap_or_default()
        .to_owned();
    let token = crate::auth::guard::bearer(request.headers()).map(str::to_owned);
    let caller = match token {
        None => Err(Rejection::Missing),
        Some(token) => authority.authenticate(&token).await,
    };
    let (subject, response) = match caller {
        Err(rejection) => (None, rejected(&rejection)),
        Ok(caller) if !caller.is_operator() => (
            Some(caller.subject().to_owned()),
            refusal(
                StatusCode::FORBIDDEN,
                Some("Bearer error=\"insufficient_scope\""),
                "forbidden",
                "the access token does not carry the operator scope",
            ),
        ),
        Ok(caller) => {
            let subject = caller.subject().to_owned();
            request.extensions_mut().insert(Operator(Arc::new(caller)));
            (Some(subject), next.run(request).await)
        }
    };
    tracing::info!(
        target: AUDIT_TARGET,
        subject = subject.as_deref().unwrap_or("unauthenticated"),
        method = method.as_str(),
        route = route.as_str(),
        status = response.status().as_u16(),
        duration_ms = started.elapsed().as_secs_f64() * 1000.0,
        request_id = request_id.as_str(),
        "operator"
    );
    response
}

/// Returns the answer `rejection` gets.
fn rejected(rejection: &Rejection) -> Response {
    match rejection {
        Rejection::Missing => refusal(
            StatusCode::UNAUTHORIZED,
            Some("Bearer"),
            "unauthorized",
            "the request carries no valid access token for this server",
        ),
        Rejection::Invalid { reason } => {
            tracing::debug!(reason, "an operator access token was refused");
            refusal(
                StatusCode::UNAUTHORIZED,
                Some("Bearer error=\"invalid_token\""),
                "unauthorized",
                "the request carries no valid access token for this server",
            )
        }
        Rejection::Unavailable { source } => {
            tracing::error!(
                error = crate::facade::outcome::chain(source.as_ref()),
                "the authorization server's key set could not be read"
            );
            refusal(
                StatusCode::SERVICE_UNAVAILABLE,
                None,
                "unavailable",
                "the access token could not be checked: the authorization server's keys could not be read",
            )
        }
    }
}

/// Returns an operator refusal: `status`, an optional challenge and a JSON
/// body naming the error.
pub fn refusal(
    status: StatusCode,
    challenge: Option<&str>,
    error: &str,
    message: &str,
) -> Response {
    let body = axum::Json(serde_json::json!({ "error": error, "message": message }));
    let mut response = (status, body).into_response();
    if let Some(challenge) = challenge
        && let Ok(value) = http::HeaderValue::from_str(challenge)
    {
        response
            .headers_mut()
            .insert(header::WWW_AUTHENTICATE, value);
    }
    response
}
