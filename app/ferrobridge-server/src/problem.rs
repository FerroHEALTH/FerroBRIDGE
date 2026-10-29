// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The RFC 9457 problem details every error body outside the FHIR routes
//! answers with (<https://www.rfc-editor.org/rfc/rfc9457>).
//!
//! FerroBRIDGE's problem types are defined once, in [`types`], under
//! [`BASE`], the book page that documents each one (RFC 9457 §4): the
//! `type` of [`Kind::NotFound`] is `{BASE}not-found`. A refusal whose
//! status says all there is to say is `about:blank` ([`blank`]). A detail
//! is a `&'static str`, so no problem can quote the request, a path, a
//! mapping body or an upstream answer (§5). The FHIR facade and the
//! operations keep the `OperationOutcome` FHIR R4 prescribes (`http.html`
//! §3.1.0.4).

use std::collections::BTreeMap;
use std::sync::LazyLock;

use axum::response::{IntoResponse as _, Response};
use ferrohealth_server::problem::{Problem, ProblemType, ProblemTypes};
use http::{HeaderValue, StatusCode, header};

/// The base URI of every FerroBRIDGE problem type: the book page that
/// documents them, one heading per slug, as the console reads it.
pub const BASE: &str = ferrobridge_operator_api::problem::BASE;

/// A FerroBRIDGE problem type, each defined once in [`types`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Kind {
    /// `400`: the `Authorization` header carries no single credential.
    InvalidRequest,
    /// `400`: a save names no reason, or one that is not one short line.
    ReasonRequired,
    /// `400`: the path names no mapping file the console can write.
    BadPath,
    /// `401`: the request carries no valid credential.
    Unauthenticated,
    /// `403`: the access token does not carry the operator scope.
    NotPermitted,
    /// `404`: what the request names does not exist.
    NotFound,
    /// `409`: a provisioned file holds the path.
    Provisioned,
    /// `409`: no `[operator] mapping_dir` is configured.
    NotConfigured,
    /// `412`: the file changed since the `ETag` the save names.
    PreconditionFailed,
    /// `428`: a save sends no `If-Match` or `If-None-Match`.
    PreconditionRequired,
    /// `500`: a mapping directory could not be read.
    Unreadable,
    /// `500`: the mapping directory could not be written.
    Unwritable,
    /// `503`: the mapping set could not be reloaded.
    ReloadUnavailable,
    /// `503`: the access token could not be checked.
    AuthorizationUnavailable,
}

impl Kind {
    /// Every kind, in the order the book lists them.
    pub const ALL: [Self; 14] = [
        Self::InvalidRequest,
        Self::ReasonRequired,
        Self::BadPath,
        Self::Unauthenticated,
        Self::NotPermitted,
        Self::NotFound,
        Self::Provisioned,
        Self::NotConfigured,
        Self::PreconditionFailed,
        Self::PreconditionRequired,
        Self::Unreadable,
        Self::Unwritable,
        Self::ReloadUnavailable,
        Self::AuthorizationUnavailable,
    ];

    /// The slug, the fragment of the type under [`BASE`], its title and
    /// its status.
    #[must_use]
    pub const fn definition(self) -> (&'static str, &'static str, StatusCode) {
        match self {
            Self::InvalidRequest => (
                "invalid-request",
                "The Authorization header carries no single credential",
                StatusCode::BAD_REQUEST,
            ),
            Self::ReasonRequired => (
                "reason-required",
                "The save names no reason",
                StatusCode::BAD_REQUEST,
            ),
            Self::BadPath => (
                "bad-path",
                "The path names no mapping file the console can write",
                StatusCode::BAD_REQUEST,
            ),
            Self::Unauthenticated => (
                "unauthenticated",
                "The request carries no valid access token",
                StatusCode::UNAUTHORIZED,
            ),
            Self::NotPermitted => (
                "not-permitted",
                "The access token does not carry the operator scope",
                StatusCode::FORBIDDEN,
            ),
            Self::NotFound => (
                "not-found",
                "What the request names does not exist",
                StatusCode::NOT_FOUND,
            ),
            Self::Provisioned => (
                "provisioned",
                "A provisioned file holds this path",
                StatusCode::CONFLICT,
            ),
            Self::NotConfigured => (
                "not-configured",
                "No mapping directory is configured",
                StatusCode::CONFLICT,
            ),
            Self::PreconditionFailed => (
                "precondition-failed",
                "The file changed since the ETag the save names",
                StatusCode::PRECONDITION_FAILED,
            ),
            Self::PreconditionRequired => (
                "precondition-required",
                "The save sends no precondition",
                StatusCode::PRECONDITION_REQUIRED,
            ),
            Self::Unreadable => (
                "unreadable",
                "A mapping directory could not be read",
                StatusCode::INTERNAL_SERVER_ERROR,
            ),
            Self::Unwritable => (
                "unwritable",
                "The mapping directory could not be written",
                StatusCode::INTERNAL_SERVER_ERROR,
            ),
            Self::ReloadUnavailable => (
                "reload-unavailable",
                "The mapping set could not be reloaded",
                StatusCode::SERVICE_UNAVAILABLE,
            ),
            Self::AuthorizationUnavailable => (
                "authorization-unavailable",
                "The access token could not be checked",
                StatusCode::SERVICE_UNAVAILABLE,
            ),
        }
    }

    /// The problem type of this kind, as [`types`] defines it.
    ///
    /// # Panics
    ///
    /// Never: the registry defines every kind, which a unit test holds.
    #[must_use]
    #[expect(
        clippy::expect_used,
        reason = "registry() defines every Kind::ALL entry, which every_kind_is_defined_once checks"
    )]
    pub fn problem_type(self) -> &'static ProblemType {
        let (types, by_kind) = &*REGISTRY;
        by_kind
            .get(&self)
            .and_then(|slug| types.get(slug))
            .expect("every kind should be defined in the registry")
    }

    /// A problem of this kind with `detail`, a fixed sentence that quotes
    /// nothing from the request.
    #[must_use]
    pub fn problem(self, detail: &'static str) -> Problem {
        self.problem_type().problem().with_detail(detail)
    }
}

/// The registry and the slug of each kind.
static REGISTRY: LazyLock<(ProblemTypes, BTreeMap<Kind, &'static str>)> = LazyLock::new(registry);

/// Builds the registry of every [`Kind`].
#[expect(
    clippy::expect_used,
    reason = "BASE and every definition are constants that every_kind_is_defined_once builds"
)]
fn registry() -> (ProblemTypes, BTreeMap<Kind, &'static str>) {
    let mut types = ProblemTypes::new(BASE).expect("the problem base should be a legal base");
    let mut by_kind = BTreeMap::new();
    for kind in Kind::ALL {
        let (slug, title, status) = kind.definition();
        types
            .define(slug, title, status)
            .expect("every problem type should be defined once with a legal slug");
        by_kind.insert(kind, slug);
    }
    (types, by_kind)
}

/// FerroBRIDGE's problem types, in slug order.
///
/// # Panics
///
/// Never: [`BASE`] and every definition are constants a unit test builds.
#[must_use]
pub fn types() -> &'static ProblemTypes {
    &REGISTRY.0
}

/// Returns `problem` as its response, never kept by a cache, with the
/// `WWW-Authenticate` `challenge` when one is given.
#[must_use]
pub fn respond(problem: Problem, challenge: Option<&str>) -> Response {
    let mut response = problem.into_response();
    let headers = response.headers_mut();
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    if let Some(value) = challenge.and_then(|challenge| HeaderValue::from_str(challenge).ok()) {
        headers.insert(header::WWW_AUTHENTICATE, value);
    }
    response
}

/// Returns the response of a problem of `kind` with `detail`.
#[must_use]
pub fn refuse(kind: Kind, detail: &'static str) -> Response {
    respond(kind.problem(detail), None)
}

/// Returns the response of an `about:blank` problem of `status`, whose
/// status says what there is to say, with `detail` (RFC 9457 §4.2.1).
#[must_use]
pub fn blank(status: StatusCode, detail: &'static str) -> Response {
    respond(Problem::new(status).with_detail(detail), None)
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use http::StatusCode;

    use super::{BASE, Kind, types};

    #[test]
    fn every_kind_is_defined_once_under_the_book_page() {
        let mut slugs = BTreeSet::new();
        for kind in Kind::ALL {
            let defined = kind.problem_type();
            let (slug, title, status) = kind.definition();
            assert_eq!(format!("{BASE}{slug}"), defined.uri());
            assert_eq!(Some(title), defined.title());
            assert_eq!(status, defined.status());
            assert!(slugs.insert(slug), "{slug} is defined twice");
        }
        assert_eq!(Kind::ALL.len(), types().iter().count());
    }

    #[test]
    fn every_kind_has_a_section_on_the_book_page() {
        let page = include_str!("../../../website/book/src/operate/problems.md");
        for kind in Kind::ALL {
            let (slug, _, status) = kind.definition();
            let heading = format!("## {slug}\n");
            assert!(page.contains(&heading), "the book documents {slug}");
            assert!(
                page.contains(&format!("`{}`", status.as_u16())),
                "the book names the status of {slug}"
            );
        }
    }

    #[test]
    fn a_problem_answers_its_status_as_problem_json_no_cache_keeps() {
        let response = super::refuse(Kind::Provisioned, "the console never writes one");
        assert_eq!(StatusCode::CONFLICT, response.status());
        let header = |name| {
            response
                .headers()
                .get(name)
                .and_then(|value| value.to_str().ok())
                .map(str::to_owned)
        };
        assert_eq!(
            Some("application/problem+json".to_owned()),
            header(http::header::CONTENT_TYPE)
        );
        assert_eq!(
            Some("no-store".to_owned()),
            header(http::header::CACHE_CONTROL)
        );
    }
}
