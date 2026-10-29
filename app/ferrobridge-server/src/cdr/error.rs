// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! What a CDR call answered beside its outcome, and what went wrong when it
//! reached no documented answer.
//!
//! A status the governing ITS-REST 1.1.0 operation documents is a variant of
//! the generated outcome enum of that call. Everything here is the other half,
//! and every refusal keeps the upstream status and body instead of an empty
//! value.

use http::HeaderMap;
use http::StatusCode;
use openehr_its::rest::client::ClientError;
use serde::Deserialize;

use crate::cdr::commit::HeaderError;
use crate::cdr::ids::ContributionUid;
use crate::cdr::ids::IdError;

/// The error body an openEHR service returns beside a `4xx` or `5xx` status.
///
/// Two shapes exist in ITS-REST 1.1.0 and this reader takes either. The
/// `OpenAPI` `Error` schema requires `message` and `validationErrors`
/// (`ehr-codegen.openapi.yaml`, `components.schemas.Error`); the prose example
/// under §Requests and responses/HTTP status codes carries `message`, `code`
/// and `errors` instead. The generated `Error` DTO requires both members of the
/// first shape, so it refuses the second.
// TODO(#104): read the generated `Error` DTO once the two error shapes agree
// upstream, and drop this reader.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(default)]
#[non_exhaustive]
pub struct ErrorBody {
    /// The human-readable message both shapes carry.
    pub message: Option<String>,
    /// The `validationErrors` of the `OpenAPI` `Error` schema.
    #[serde(rename = "validationErrors")]
    pub validation_errors: Vec<String>,
    /// The `code` of the prose shape.
    pub code: Option<i64>,
    /// The `errors` of the prose shape, whose example members are
    /// `DV_CODED_TEXT` values.
    pub errors: Vec<serde_json::Value>,
}

/// The RFC 9457 problem details a CDR answers a refusal with.
///
/// A CDR that follows Problem Details for HTTP APIs
/// (<https://www.rfc-editor.org/rfc/rfc9457>) answers its errors as
/// `application/problem+json` with the §3.1 members `type`, `title`,
/// `status`, `detail` and `instance`. The reference CDR adds the extension
/// member `errors`, each a `{path, detail}` violation, to a refused write that
/// asked for `Prefer: return=representation`. ITS-REST 1.1.0 names neither
/// form, so this reader sits beside [`ErrorBody`] and a body that is neither
/// keeps its raw text.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(default)]
#[non_exhaustive]
pub struct ProblemDetails {
    /// The problem type URI (§3.1.1).
    #[serde(rename = "type")]
    pub kind: Option<String>,
    /// The short summary of the problem type (§3.1.3).
    pub title: Option<String>,
    /// The HTTP status the CDR names (§3.1.2).
    pub status: Option<u16>,
    /// The explanation of this occurrence (§3.1.4).
    pub detail: Option<String>,
    /// The URI of this occurrence (§3.1.5).
    pub instance: Option<String>,
    /// The violations of a refused write, an extension member (§3.2).
    pub errors: Vec<ProblemViolation>,
}

/// One violation the `errors` extension member of a problem names.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(default)]
#[non_exhaustive]
pub struct ProblemViolation {
    /// The path of the node that violates a constraint.
    pub path: Option<String>,
    /// What the constraint says.
    pub detail: Option<String>,
}

impl ProblemDetails {
    /// Returns the problem details `body` holds, or `None` when it holds
    /// none.
    ///
    /// A body that is no JSON object, or one with neither a `type` nor a
    /// `title` nor a `detail`, is not a problem, so the caller keeps reading
    /// it the way it read it before.
    #[must_use]
    pub fn of(body: &[u8]) -> Option<Self> {
        // NOTE: RFC 9457 §3 makes every member optional and ITS-REST 1.1.0 names no
        // problem body, so a body that does not decode is legitimately not one.
        let decoded = serde_json::from_slice::<Self>(body).ok()?;
        (decoded.kind.is_some() || decoded.title.is_some() || decoded.detail.is_some())
            .then_some(decoded)
    }

    /// Returns the words a refusal carries: the title, the detail, and each
    /// violation as `path: detail`, joined by `; `.
    #[must_use]
    pub fn words(&self) -> String {
        let mut parts: Vec<String> = Vec::new();
        match (self.title.as_deref(), self.detail.as_deref()) {
            (Some(title), Some(detail)) => parts.push(format!("{title}: {detail}")),
            (Some(one), None) | (None, Some(one)) => parts.push(one.to_owned()),
            (None, None) => {}
        }
        for violation in &self.errors {
            match (violation.path.as_deref(), violation.detail.as_deref()) {
                (Some(path), Some(detail)) => parts.push(format!("{path}: {detail}")),
                (Some(one), None) | (None, Some(one)) => parts.push(one.to_owned()),
                (None, None) => {}
            }
        }
        if let Some(kind) = self.kind.as_deref().filter(|kind| *kind != "about:blank") {
            parts.push(format!("({kind})"));
        }
        parts.join("; ")
    }
}

/// The status, headers and body one CDR answer carried.
///
/// The body can be clinical content, so the `Debug` rendering names its length
/// and never its text.
#[derive(Clone)]
pub struct Upstream {
    /// The HTTP status the service answered with.
    status: StatusCode,
    /// The response headers.
    headers: HeaderMap,
    /// The response body, as received.
    body: String,
    /// The decoded error body, when the body was one.
    error: Option<ErrorBody>,
    /// The decoded problem details, when the body was a problem, boxed so
    /// the answer stays small inside the errors that carry it.
    problem: Option<Box<ProblemDetails>>,
}

impl Upstream {
    /// Returns the upstream answer `status`, `headers` and `body` describe.
    #[must_use]
    pub fn new(status: StatusCode, headers: HeaderMap, body: String) -> Self {
        // NOTE: the OpenAPI documents make an error body optional ("The
        // response body MAY contain error details"), so a body that is not one
        // is legitimately absent rather than defective, and the raw body stays.
        let error = serde_json::from_str::<ErrorBody>(&body)
            .ok()
            .filter(|decoded| decoded != &ErrorBody::default());
        let problem = ProblemDetails::of(body.as_bytes()).map(Box::new);
        Self {
            status,
            headers,
            body,
            error,
            problem,
        }
    }

    /// Returns the HTTP status the service answered with.
    #[must_use]
    pub const fn status(&self) -> StatusCode {
        self.status
    }

    /// Returns the value of response header `name`, when it is usable text.
    #[must_use]
    pub fn header(&self, name: &str) -> Option<String> {
        // NOTE: RFC 9110 §5.5 admits opaque octets in a field value; one that is
        // not visible ASCII is legitimately unusable as text, not defective.
        self.headers
            .get(name)
            .and_then(|value| value.to_str().ok())
            .map(str::to_owned)
    }

    /// Returns the response body, as received.
    #[must_use]
    pub fn body(&self) -> &str {
        &self.body
    }

    /// Returns the decoded error body, when the service sent one.
    #[must_use]
    pub const fn error(&self) -> Option<&ErrorBody> {
        self.error.as_ref()
    }

    /// Returns the decoded problem details, when the service sent them.
    #[must_use]
    pub fn problem(&self) -> Option<&ProblemDetails> {
        self.problem.as_deref()
    }
}

impl std::fmt::Debug for Upstream {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Upstream")
            .field("status", &self.status)
            .field("body_len", &self.body.len())
            .finish_non_exhaustive()
    }
}

impl std::fmt::Display for Upstream {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if let Some(message) = self.error.as_ref().and_then(|error| error.message.as_ref()) {
            return write!(f, "{} ({message})", self.status);
        }
        match self.problem.as_deref().map(ProblemDetails::words) {
            Some(words) if !words.is_empty() => write!(f, "{} ({words})", self.status),
            _ => write!(f, "{}", self.status),
        }
    }
}

/// A response body the bridge could not decode as the shape its route
/// declares.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum BodyError {
    /// Canonical JSON that is not the RM value the route declares.
    #[error(transparent)]
    CanonicalJson(#[from] openehr_its::json::JsonParseError),
    /// JSON that is not the `OpenAPI` schema the route declares.
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    /// XML that is not a canonical OPT 1.4 operational template.
    #[error(transparent)]
    Xml(#[from] openehr_its::xml::runtime::XmlError),
}

/// A CDR call that did not reach a documented answer the bridge can use.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum CdrError {
    /// The HTTP engine could not be built from the configuration.
    #[error("the HTTP engine of the CDR client could not be built")]
    Build {
        /// What `reqwest` reported.
        #[source]
        source: reqwest::Error,
    },
    /// The HTTP engine of the token client could not be built.
    #[error("the token client of the CDR client could not be built")]
    TokenClient {
        /// What the token client reported.
        #[source]
        source: ferrohealth_smart::client::TokenError,
    },
    /// The generated client refused the call, or the call reached no
    /// documented answer.
    #[error(
        "the CDR call did not reach a documented answer{}",
        answered_status(upstream.as_deref())
    )]
    Client {
        /// What the generated client reported.
        #[source]
        source: Box<ClientError>,
        /// The answer the service sent, when the call reached it.
        upstream: Option<Box<Upstream>>,
    },
    /// The CDR refused the token (`401`) and no new one could be obtained,
    /// so the call ended without being asked again.
    #[error("the CDR call ended without a usable credential")]
    Renewal {
        /// Why the new token could not be obtained.
        #[source]
        source: ferrohealth_smart::client::RenewalError,
        /// The `401` the service sent.
        upstream: Option<Box<Upstream>>,
    },
    /// The commit metadata cannot travel in a header.
    #[error("the commit metadata cannot travel in a header")]
    Commit(#[from] HeaderError),
    /// A value the bridge supplied cannot travel in an HTTP header.
    #[error("the value for the {header} header is not a legal header value")]
    HeaderValue {
        /// The header that was being built.
        header: &'static str,
        /// What the HTTP stack reported.
        #[source]
        source: http::header::InvalidHeaderValue,
    },
    /// A response header the answer relies on was absent.
    #[error("the {status} answer of {operation} carries no {header} header")]
    MissingHeader {
        /// The ITS-REST `operationId`.
        operation: &'static str,
        /// The status that was answered.
        status: StatusCode,
        /// The header that was expected.
        header: &'static str,
    },
    /// A response header carried something other than what it declares.
    #[error("the {header} header of the {operation} answer is not usable")]
    MalformedHeader {
        /// The ITS-REST `operationId`.
        operation: &'static str,
        /// The header that was read.
        header: &'static str,
        /// Why its value could not be used.
        #[source]
        source: Box<IdError>,
    },
    /// A documented body could not be decoded as the shape its preference or
    /// route selects.
    #[error("the body of the {operation} answer could not be decoded")]
    Body {
        /// The ITS-REST `operationId`.
        operation: &'static str,
        /// What the decoder reported.
        #[source]
        source: Box<BodyError>,
    },
    /// The service committed a contribution and its `201` body is no schema
    /// the operation admits, so the uid is all the answer carries.
    #[error(
        "the openEHR service committed contribution {contribution_uid} and its answer body could not be decoded"
    )]
    CommittedBody {
        /// The contribution the `ETag` named.
        contribution_uid: ContributionUid,
        /// What the decoder reported.
        #[source]
        source: Box<dyn std::error::Error + Send + Sync>,
    },
    /// The ADL 2 route answered a body that is not an operational template.
    #[error("the adl2 route answered a body whose _type is {found:?}, not OPERATIONAL_TEMPLATE")]
    NotOperationalTemplate {
        /// The `_type` the body declared, when it declared one.
        found: Option<String>,
    },
    /// The engine read an answer and kept no record of it.
    #[error("the CDR answer was read and not recorded")]
    Unrecorded,
}

impl CdrError {
    /// Returns the refusal `source` is, with the answer the service sent.
    #[must_use]
    pub fn client(source: ClientError, upstream: Option<Upstream>) -> Self {
        Self::Client {
            source: Box::new(source),
            upstream: upstream.map(Box::new),
        }
    }

    /// Returns the variant's name as a short kebab-case label, for a log line
    /// or a diagnostic that must name the failure class and never a body.
    #[must_use]
    pub fn kind(&self) -> &'static str {
        match self {
            Self::Build { .. } => "client-build",
            Self::TokenClient { .. } => "token-client-build",
            Self::Client { source, .. } => client_kind(source),
            Self::Renewal { .. } => "token-renewal",
            Self::Commit(HeaderError::Attribute { .. }) => "header-attribute",
            Self::Commit(_) | Self::HeaderValue { .. } => "header-value",
            Self::MissingHeader { .. } => "missing-header",
            Self::MalformedHeader { .. } => "malformed-header",
            Self::Body { .. } => "body",
            Self::CommittedBody { .. } => "committed-body",
            Self::NotOperationalTemplate { .. } => "not-operational-template",
            Self::Unrecorded => "unrecorded",
        }
    }

    /// Returns whether asking again may succeed: the call reached no answer,
    /// or the service answered a failure the generated client counts
    /// retryable ([`ClientError::is_retryable`]).
    #[must_use]
    pub fn is_retryable(&self) -> bool {
        matches!(self, Self::Client { source, .. } if source.is_retryable())
    }
}

/// Returns the status clause of a refusal's display, when the call reached
/// the service.
fn answered_status(upstream: Option<&Upstream>) -> String {
    upstream.map_or_else(String::new, |upstream| {
        format!(": the CDR answered {}", upstream.status())
    })
}

/// Returns the kebab-case label of one generated-client refusal.
pub(crate) fn client_kind(error: &ClientError) -> &'static str {
    match error {
        ClientError::BaseUrl { .. } => "base-url",
        ClientError::DeadlineElapsed { .. } => "deadline-elapsed",
        ClientError::DpopProof { .. } => "dpop-proof",
        ClientError::Credentials { .. } => "credentials",
        ClientError::InvalidCredentials { .. } => "invalid-credentials",
        ClientError::Transport { .. } => "transport",
        ClientError::Build { .. } => "request-build",
        ClientError::HeaderName { .. } | ClientError::HeaderValue { .. } => "header-value",
        ClientError::UnsupportedMediaType { .. } => "media-type",
        ClientError::Serialize { .. } | ClientError::Body { .. } => "body",
        ClientError::Unauthorized { .. } => "unauthorized",
        ClientError::Forbidden { .. } => "forbidden",
        ClientError::ServiceFailure { .. } => "service-failure",
        ClientError::UndocumentedStatus { .. } => "undocumented-status",
        ClientError::AnswerTooLarge { .. } => "answer-too-large",
    }
}

#[cfg(test)]
mod tests {
    use super::{ErrorBody, Upstream};
    use http::{HeaderMap, StatusCode};

    fn upstream(status: StatusCode, body: &str) -> Upstream {
        Upstream::new(status, HeaderMap::new(), body.to_owned())
    }

    #[test]
    fn the_openapi_error_shape_decodes() {
        let upstream = upstream(
            StatusCode::UNPROCESSABLE_ENTITY,
            r#"{"message":"no","validationErrors":["/content missing"]}"#,
        );
        let error = upstream.error().expect("the body is an error body");
        assert_eq!(Some("no"), error.message.as_deref());
        assert_eq!(vec!["/content missing".to_owned()], error.validation_errors);
    }

    #[test]
    fn the_prose_error_shape_decodes() {
        let upstream = upstream(
            StatusCode::BAD_REQUEST,
            r#"{"message":"no","code":90000,"errors":[{"_type":"DV_CODED_TEXT"}]}"#,
        );
        let error = upstream.error().expect("the body is an error body");
        assert_eq!(Some(90_000), error.code);
        assert_eq!(1, error.errors.len());
    }

    #[test]
    fn a_body_that_is_not_an_error_body_stays_raw() {
        let upstream = upstream(StatusCode::NOT_FOUND, "not found");
        assert_eq!(None, upstream.error());
        assert_eq!("not found", upstream.body());
    }

    #[test]
    fn an_empty_json_object_is_not_an_error_body() {
        assert_eq!(None, upstream(StatusCode::NOT_FOUND, "{}").error());
    }

    #[test]
    fn an_error_body_display_carries_the_message() {
        let upstream = upstream(StatusCode::BAD_REQUEST, r#"{"message":"bad path"}"#);
        assert_eq!("400 Bad Request (bad path)", upstream.to_string());
        assert_eq!(
            Some(&ErrorBody {
                message: Some("bad path".to_owned()),
                validation_errors: Vec::new(),
                code: None,
                errors: Vec::new(),
            }),
            upstream.error()
        );
    }

    #[test]
    fn the_debug_rendering_never_carries_the_body() {
        let upstream = upstream(
            StatusCode::UNPROCESSABLE_ENTITY,
            "synthetic-clinical-marker",
        );
        assert!(!format!("{upstream:?}").contains("synthetic-clinical-marker"));
    }
}
