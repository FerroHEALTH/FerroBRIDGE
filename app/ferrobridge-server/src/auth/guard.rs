// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The middleware in front of every FHIR route: the token check, the
//! type-level scope check and the audit line.
//!
//! The guard reads the interaction from the method and the matched route, so
//! it knows the resource type and the id before a handler runs. A request
//! without a valid token answers `401`, one whose token grants no scope for
//! the interaction `403` (RFC 6750 §3.1). What a scope reaches inside a type
//! (the patient compartment) and the entries of a transaction are checked by
//! the handler, which reads the [`Access`] the guard leaves on the request.
//! `GET [base]/metadata` and the SMART discovery document stay open: a client
//! reads both before it holds a token.

use std::sync::Arc;
use std::time::Instant;

use axum::extract::MatchedPath;
use axum::extract::Request;
use axum::extract::State;
use axum::middleware::Next;
use axum::response::Response;
use http::HeaderMap;
use http::Method;
use http::StatusCode;

use crate::auth::Access;
use crate::auth::Authority;
use crate::auth::Rejection;
use crate::auth::scope::Permission;
use crate::facade::BASE_PATH;
use crate::facade::outcome::Issue;
use crate::facade::outcome::IssueType;
use crate::facade::reply;
use crate::facade::reply::Refusal;

/// The audit line's `tracing` target, so a deployment can route it apart.
pub const AUDIT_TARGET: &str = "ferrobridge::audit";

/// The longest a logged resource id may be.
const MAX_ID_LENGTH: usize = 64;

/// The path of the SMART discovery document under a FHIR base.
///
/// "FHIR endpoints requiring authorization SHALL serve a JSON document at
/// the location formed by appending `/.well-known/smart-configuration` to
/// their base URL"
/// (<https://hl7.org/fhir/smart-app-launch/conformance.html#using-well-known>).
pub const SMART_CONFIGURATION: &str = "/.well-known/smart-configuration";

/// The state the guard runs over: the resource server, or none when inbound
/// authentication is off.
#[derive(Debug, Clone, Default)]
pub struct Gate {
    /// The resource server, when `[auth]` is on.
    authority: Option<Arc<Authority>>,
}

impl Gate {
    /// Returns the gate of a deployment that authenticates no caller.
    #[must_use]
    pub fn open() -> Self {
        Self { authority: None }
    }

    /// Returns the gate that checks every token against `authority`.
    #[must_use]
    pub const fn new(authority: Arc<Authority>) -> Self {
        Self {
            authority: Some(authority),
        }
    }
}

/// One FHIR interaction the facade serves.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Interaction {
    /// `GET [base]/metadata`.
    Capabilities,
    /// `POST [base]/{type}`.
    Create,
    /// `GET [base]/{type}/{id}`.
    Read,
    /// `GET [base]/{type}/{id}/_history/{vid}`.
    Vread,
    /// `PUT [base]/{type}/{id}`.
    Update,
    /// `POST [base]/{type}/$validate`.
    Validate,
    /// `POST [base]`.
    Transaction,
    /// A FHIRconnect operation or any other route under the base.
    Operation,
    /// The SMART discovery document.
    Discovery,
}

impl Interaction {
    /// Returns the name the audit line records.
    ///
    /// The REST interactions keep their R4 codes
    /// (<https://hl7.org/fhir/R4/valueset-type-restful-interaction.html>).
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::Capabilities => "capabilities",
            Self::Create => "create",
            Self::Read => "read",
            Self::Vread => "vread",
            Self::Update => "update",
            Self::Validate => "validate",
            Self::Transaction => "transaction",
            Self::Operation => "operation",
            Self::Discovery => "smart-configuration",
        }
    }

    /// Returns the permissions of which the type-level check needs one.
    ///
    /// `$validate` is a dry run of a create or an update, so either
    /// permission admits it (no specification governs the scope of an
    /// operation: our own design).
    const fn permissions(self) -> &'static [Permission] {
        match self {
            Self::Create => &[Permission::Create],
            Self::Read | Self::Vread => &[Permission::Read],
            Self::Update => &[Permission::Update],
            Self::Validate => &[Permission::Create, Permission::Update],
            Self::Capabilities | Self::Transaction | Self::Operation | Self::Discovery => &[],
        }
    }

    /// Returns whether a client reaches the route without a token.
    const fn public(self) -> bool {
        matches!(self, Self::Capabilities | Self::Discovery)
    }
}

/// What the guard learned about one request.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Target {
    /// The interaction.
    interaction: Interaction,
    /// The resource type the path names.
    resource_type: Option<String>,
    /// The logical id the path names.
    id: Option<String>,
}

/// Checks the token of `request`, checks its scope for the interaction, and
/// writes the audit line once the response is ready.
///
/// A route outside the FHIR base passes untouched.
pub async fn guard(State(gate): State<Gate>, mut request: Request, next: Next) -> Response {
    let started = Instant::now();
    let at = jiff::Timestamp::now();
    let template = request
        .extensions()
        .get::<MatchedPath>()
        .map(|matched| matched.as_str().to_owned());
    let Some(target) = template
        .as_deref()
        .and_then(|template| classify(request.method(), template, request.uri().path()))
    else {
        return next.run(request).await;
    };
    let method = request.method().clone();
    let request_id = crate::request_id::of(request.headers())
        .unwrap_or_default()
        .to_owned();
    let (access, response) = match authenticate(&gate, &target, request.headers()).await {
        Err(refusal) => (None, refusal.into_response()),
        Ok(access) => {
            if let Err(refusal) = authorize(&access, &target) {
                (Some(access), refusal.into_response())
            } else {
                request.extensions_mut().insert(access.clone());
                (Some(access), next.run(request).await)
            }
        }
    };
    audit(&Audited {
        at,
        started,
        access: access.as_ref(),
        method: &method,
        target: &target,
        status: response.status(),
        request_id: &request_id,
    });
    response
}

/// Returns who calls, or the refusal a missing or invalid token answers.
async fn authenticate(
    gate: &Gate,
    target: &Target,
    headers: &HeaderMap,
) -> Result<Access, Refusal> {
    let Some(authority) = gate.authority.as_ref() else {
        return Ok(Access::Open);
    };
    if target.interaction.public() {
        return Ok(Access::Open);
    }
    let caller = match bearer(headers) {
        None => Err(Rejection::Missing),
        Some(token) => authority.authenticate(token).await,
    }
    .map_err(|rejection| Refusal::new(rejected(&rejection)))?;
    Ok(Access::Caller(Arc::new(caller)))
}

/// Checks that `access` grants a permission the interaction needs on the
/// resource type the path names.
fn authorize(access: &Access, target: &Target) -> Result<(), Refusal> {
    let permissions = target.interaction.permissions();
    if let Some(resource_type) = target.resource_type.as_deref()
        && !permissions.is_empty()
        && !permissions
            .iter()
            .any(|permission| access.reach(*permission, resource_type).is_some())
    {
        return Err(forbidden());
    }
    Ok(())
}

/// Returns the bearer token `headers` carry (RFC 6750 §2.1), the scheme
/// compared case-insensitively (RFC 9110 §11.1).
///
/// A request with another scheme carries no bearer token.
fn bearer(headers: &HeaderMap) -> Option<&str> {
    let value = headers.get(http::header::AUTHORIZATION)?.to_str().ok()?;
    let (scheme, token) = value.split_once(' ')?;
    if !scheme.eq_ignore_ascii_case("Bearer") {
        return None;
    }
    Some(token.trim())
}

/// Returns the answer `rejection` gets.
///
/// A missing token is challenged without an error code and an invalid one
/// with `invalid_token` (RFC 6750 §3.1); neither body says which claim
/// failed. A key set that could not be read is `503`, because the token was
/// not checked at all.
fn rejected(rejection: &Rejection) -> Response {
    match rejection {
        Rejection::Missing => unauthorized("Bearer"),
        Rejection::Invalid { reason } => {
            tracing::debug!(reason, "an access token was refused");
            unauthorized("Bearer error=\"invalid_token\"")
        }
        Rejection::Unavailable { source } => {
            tracing::error!(
                error = crate::facade::outcome::chain(source.as_ref()),
                "the authorization server's key set could not be read"
            );
            reply::issue(
                StatusCode::SERVICE_UNAVAILABLE,
                Issue::error(IssueType::Transient).diagnosing(
                    "the access token could not be checked: the authorization server's keys could not be read",
                ),
            )
        }
    }
}

/// Returns the `401` every missing or invalid token answers.
fn unauthorized(challenge: &str) -> Response {
    reply::issues(
        StatusCode::UNAUTHORIZED,
        &[Issue::error(IssueType::Login)
            .diagnosing("the request carries no valid access token for this server")],
        &[reply::Header::challenge(challenge)],
    )
}

/// Returns the `403` of a token that does not permit the request.
///
/// The body and the challenge name no scope: RFC 6750 §3.1 lets the
/// challenge carry the scope needed, and this server leaves it out so a
/// refusal teaches a caller nothing about the grants it lacks (no
/// specification requires the omission: our own design).
#[must_use]
pub fn forbidden() -> Refusal {
    Refusal::new(reply::issues(
        StatusCode::FORBIDDEN,
        &[Issue::error(IssueType::Forbidden)
            .diagnosing("the access token does not permit this request")],
        &[reply::Header::challenge(
            "Bearer error=\"insufficient_scope\"",
        )],
    ))
}

/// Returns what `method` on `template` is, or `None` for a route outside the
/// FHIR base.
fn classify(method: &Method, template: &str, path: &str) -> Option<Target> {
    let relative = template.strip_prefix(BASE_PATH)?;
    if !relative.is_empty() && !relative.starts_with('/') {
        return None;
    }
    let mut segments = path
        .strip_prefix(BASE_PATH)
        .unwrap_or_default()
        .split('/')
        .filter(|segment| !segment.is_empty());
    let first = segments.next().map(str::to_owned);
    let second = segments.next().map(str::to_owned);
    let typed = |interaction, id| Target {
        interaction,
        resource_type: first.clone(),
        id,
    };
    let interaction = match (method, relative) {
        (&Method::GET, "/metadata") => Interaction::Capabilities,
        (&Method::GET, SMART_CONFIGURATION) => Interaction::Discovery,
        (&Method::POST, "") => Interaction::Transaction,
        (&Method::POST, "/{resource_type}") => return Some(typed(Interaction::Create, None)),
        (&Method::POST, "/{resource_type}/$validate") => {
            return Some(typed(Interaction::Validate, None));
        }
        (&Method::GET, "/{resource_type}/{id}") => {
            return Some(typed(Interaction::Read, second));
        }
        (&Method::PUT, "/{resource_type}/{id}") => {
            return Some(typed(Interaction::Update, second));
        }
        (&Method::GET, "/{resource_type}/{id}/_history/{vid}") => {
            return Some(typed(Interaction::Vread, second));
        }
        _ => Interaction::Operation,
    };
    Some(Target {
        interaction,
        resource_type: None,
        id: None,
    })
}

/// Everything one audit line records.
struct Audited<'a> {
    /// When the request arrived.
    at: jiff::Timestamp,
    /// When the request arrived, for the duration.
    started: Instant,
    /// What the caller was admitted with, `None` for a refused token.
    access: Option<&'a Access>,
    /// The method.
    method: &'a Method,
    /// The interaction and its resource.
    target: &'a Target,
    /// The status answered.
    status: StatusCode,
    /// The request id.
    request_id: &'a str,
}

/// Writes the one audit line of a request.
///
/// It names who called (the token's subject and client), what was asked
/// (method, interaction, resource type and id) and how it ended (status and
/// duration). It never carries the token, a claim beyond those two, a query
/// value or a body. No specification governs the line: our own design over
/// the EHDS Annex II 3.2(b) access record.
fn audit(audited: &Audited<'_>) {
    if audited.target.interaction == Interaction::Discovery {
        return;
    }
    let (subject, client) = match audited.access {
        Some(Access::Caller(caller)) => (caller.subject(), caller.client_id().unwrap_or("-")),
        Some(Access::Open) => ("-", "-"),
        None => ("unauthenticated", "-"),
    };
    let id: String = audited
        .target
        .id
        .as_deref()
        .unwrap_or("-")
        .chars()
        .take(MAX_ID_LENGTH)
        .collect();
    let resource_type: String = audited
        .target
        .resource_type
        .as_deref()
        .unwrap_or("-")
        .chars()
        .take(MAX_ID_LENGTH)
        .collect();
    tracing::info!(
        target: AUDIT_TARGET,
        at = %audited.at,
        subject,
        client,
        method = audited.method.as_str(),
        interaction = audited.target.interaction.code(),
        resource_type,
        resource_id = id,
        status = audited.status.as_u16(),
        duration_ms = audited.started.elapsed().as_secs_f64() * 1000.0,
        request_id = audited.request_id,
        "audit"
    );
}

#[cfg(test)]
mod tests {
    use super::{Interaction, classify};
    use http::Method;

    #[test]
    fn the_matched_route_names_the_interaction_type_and_id() {
        let read = classify(
            &Method::GET,
            "/fhir/{resource_type}/{id}",
            "/fhir/Condition/c-1",
        )
        .expect("a FHIR route");
        assert_eq!(Interaction::Read, read.interaction);
        assert_eq!(Some("Condition"), read.resource_type.as_deref());
        assert_eq!(Some("c-1"), read.id.as_deref());
        let vread = classify(
            &Method::GET,
            "/fhir/{resource_type}/{id}/_history/{vid}",
            "/fhir/Condition/c-1/_history/2",
        )
        .expect("a FHIR route");
        assert_eq!(Interaction::Vread, vread.interaction);
        assert_eq!(Some("c-1"), vread.id.as_deref());
    }

    #[test]
    fn a_route_outside_the_base_is_not_classified() {
        assert!(classify(&Method::GET, "/health/readiness", "/health/readiness").is_none());
        assert!(classify(&Method::GET, "/fhirx", "/fhirx").is_none());
    }

    #[test]
    fn an_unknown_route_under_the_base_needs_a_token() {
        let other = classify(&Method::POST, "/fhir/$tofhir", "/fhir/$tofhir").expect("FHIR");
        assert_eq!(Interaction::Operation, other.interaction);
        assert!(!other.interaction.public());
    }
}
