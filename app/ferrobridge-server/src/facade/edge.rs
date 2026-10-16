// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The answers the middleware stack authors in front of a FHIR route, rendered
//! as an `OperationOutcome`.
//!
//! The body ceiling and the request timeout refuse a request before or around
//! its handler, so their plain answers would reach a FHIR client as a body it
//! cannot read. R4 asks a server to return an `OperationOutcome` with an error
//! status (<https://hl7.org/fhir/R4/http.html>, §3.1.0.4), so under
//! [`BASE_PATH`] [`render`] replaces each with one; every other route keeps the
//! plain answer.
//!
//! The statuses are our own design, since no specification governs them: the
//! ceiling stays `413 Content Too Large` (RFC 9110 §15.5.14) with the
//! `too-long` issue, and the timeout becomes `504 Gateway Timeout`
//! (RFC 9110 §15.6.5) with the `timeout` issue. The facade is a gateway to the
//! CDR and the timeout runs around the CDR call, so a `408` (RFC 9110
//! §15.5.9), which says the server never received the whole request and lets a
//! client repeat it, would invite a blind repeat of a write the CDR may
//! already have committed.

use std::time::Duration;

use axum::extract::Request;
use axum::extract::State;
use axum::middleware::Next;
use axum::response::Response;
use http::StatusCode;
use http::header;

use crate::facade::BASE_PATH;
use crate::facade::media::FHIR_JSON;
use crate::facade::outcome::Issue;
use crate::facade::outcome::IssueType;
use crate::facade::reply;

/// The limits the edge answers name in their diagnostics.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Edge {
    /// The request body ceiling, in bytes.
    body_limit: usize,
    /// How long a request may take.
    timeout: Duration,
}

impl Edge {
    /// Returns the edge of a stack whose ceiling is `body_limit` bytes and
    /// whose requests may take `timeout`.
    #[must_use]
    pub const fn new(body_limit: usize, timeout: Duration) -> Self {
        Self {
            body_limit,
            timeout,
        }
    }

    /// Returns the `OperationOutcome` answer that replaces `response` on a
    /// FHIR route, or `response` itself when it is no edge answer.
    ///
    /// An answer that already carries the FHIR media type was authored by the
    /// facade and is left alone.
    #[must_use]
    pub fn outcome_of(&self, response: Response) -> Response {
        if carries_fhir(&response) {
            return response;
        }
        match response.status() {
            StatusCode::PAYLOAD_TOO_LARGE => reply::issue(
                StatusCode::PAYLOAD_TOO_LARGE,
                Issue::error(IssueType::TooLong).diagnosing(format!(
                    "the request body is over the {}-byte ceiling of this server",
                    self.body_limit
                )),
            ),
            StatusCode::REQUEST_TIMEOUT => reply::issue(
                StatusCode::GATEWAY_TIMEOUT,
                Issue::error(IssueType::Timeout).diagnosing(format!(
                    "the request did not complete within the {} ms this server allows; \
                     a write may have reached the openEHR service",
                    self.timeout.as_millis()
                )),
            ),
            _ => response,
        }
    }
}

/// Renders the ceiling's and the timeout's answer on a FHIR route as an
/// `OperationOutcome`.
pub async fn render(State(edge): State<Edge>, request: Request, next: Next) -> Response {
    let fhir = on_fhir_route(request.uri().path());
    let response = next.run(request).await;
    if fhir {
        edge.outcome_of(response)
    } else {
        response
    }
}

/// Returns whether `path` is under [`BASE_PATH`].
pub(crate) fn on_fhir_route(path: &str) -> bool {
    path.strip_prefix(BASE_PATH)
        .is_some_and(|rest| rest.is_empty() || rest.starts_with('/'))
}

/// Returns whether `response` declares the FHIR JSON media type.
fn carries_fhir(response: &Response) -> bool {
    response
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value.starts_with(FHIR_JSON))
}

#[cfg(test)]
mod tests {
    use super::{Edge, on_fhir_route};
    use axum::response::IntoResponse;
    use http::StatusCode;
    use std::time::Duration;

    fn edge() -> Edge {
        Edge::new(1024, Duration::from_millis(20))
    }

    #[test]
    fn only_the_fhir_base_and_its_children_are_fhir_routes() {
        assert!(on_fhir_route("/fhir"));
        assert!(on_fhir_route("/fhir/Condition"));
        assert!(!on_fhir_route("/fhirx"));
        assert!(!on_fhir_route("/health/liveness"));
    }

    #[test]
    fn a_timeout_becomes_a_504_and_the_ceiling_stays_a_413() {
        let timed_out = edge().outcome_of(StatusCode::REQUEST_TIMEOUT.into_response());
        assert_eq!(StatusCode::GATEWAY_TIMEOUT, timed_out.status());
        let oversized = edge().outcome_of(StatusCode::PAYLOAD_TOO_LARGE.into_response());
        assert_eq!(StatusCode::PAYLOAD_TOO_LARGE, oversized.status());
    }

    #[test]
    fn any_other_answer_is_left_alone() {
        let answer = edge().outcome_of(StatusCode::NOT_FOUND.into_response());
        assert_eq!(StatusCode::NOT_FOUND, answer.status());
        assert!(answer.headers().get(http::header::CONTENT_TYPE).is_none());
    }
}
