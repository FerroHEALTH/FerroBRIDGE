// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The refusals the stack of `ferrohealth-server` writes in front of a FHIR
//! route, rendered as an `OperationOutcome`.
//!
//! The body ceiling, the request timeout and the overload bound refuse a
//! request before or around its handler, so their plain answers would reach a
//! FHIR client as a body it cannot read. R4 asks a server to return an
//! `OperationOutcome` with an error status
//! (<https://hl7.org/fhir/R4/http.html>, §3.1.0.4), so under [`BASE_PATH`]
//! [`body`] writes one; every other route keeps the family's plain answer.
//!
//! The statuses are our own design, since no specification governs them: the
//! ceiling stays `413 Content Too Large` (RFC 9110 §15.5.14) with the
//! `too-long` issue, and the timeout becomes `504 Gateway Timeout`
//! (RFC 9110 §15.6.5) with the `timeout` issue. The facade is a gateway to the
//! CDR and the timeout runs around the CDR call, so a `408` (RFC 9110
//! §15.5.9), which says the server never received the whole request and lets a
//! client repeat it, would invite a blind repeat of a write the CDR may
//! already have committed.

use std::sync::Arc;
use std::time::Duration;

use axum::extract::{Request, State};
use axum::middleware::Next;
use axum::response::Response;
use ferrohealth_server::refusal::RefusalBody;
use http::StatusCode;

use crate::facade::BASE_PATH;
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

    /// Returns the `OperationOutcome` a FHIR route answers in place of a
    /// refusal of `status`, or `None` for a status the edge leaves alone.
    #[must_use]
    pub fn outcome_of(&self, status: StatusCode) -> Option<Response> {
        match status {
            StatusCode::PAYLOAD_TOO_LARGE => Some(reply::issue(
                StatusCode::PAYLOAD_TOO_LARGE,
                Issue::error(IssueType::TooLong).diagnosing(format!(
                    "the request body is over the {}-byte ceiling of this server",
                    self.body_limit
                )),
            )),
            StatusCode::REQUEST_TIMEOUT => Some(reply::issue(
                StatusCode::GATEWAY_TIMEOUT,
                Issue::error(IssueType::Timeout).diagnosing(format!(
                    "the request did not complete within the {} ms this server allows; \
                     a write may have reached the openEHR service",
                    self.timeout.as_millis()
                )),
            )),
            StatusCode::TOO_MANY_REQUESTS | StatusCode::SERVICE_UNAVAILABLE => Some(reply::issue(
                status,
                Issue::error(IssueType::Transient)
                    .diagnosing("the server is busy; repeat the request later"),
            )),
            _ => None,
        }
    }
}

/// Returns the refusal body of a stack whose ceiling is `body_limit` bytes
/// and whose requests may take `timeout`: an `OperationOutcome` under the
/// FHIR base, the family's plain answer everywhere else.
#[must_use]
pub fn body(body_limit: usize, timeout: Duration) -> RefusalBody {
    let edge = Edge::new(body_limit, timeout);
    let plain = ferrohealth_server::refusal::default_body();
    Arc::new(move |uri, request_id, status, reason| {
        let outcome = on_fhir_route(uri.path())
            .then(|| edge.outcome_of(status))
            .flatten();
        match outcome {
            Some(mut outcome) => {
                let answered = outcome.status();
                if answered != status {
                    outcome.extensions_mut().insert(Answered(answered));
                }
                outcome
            }
            None => plain(uri, request_id, status, reason),
        }
    })
}

/// The status a FHIR refusal answers when it differs from the stack's.
///
/// The stack writes a refusal under its own status, so the body marks the one
/// the facade answers and [`fhir_refusals`] sets it outside the stack.
#[derive(Debug, Clone, Copy)]
struct Answered(StatusCode);

/// Finishes the refusals on a FHIR route outside the stack.
///
/// A refusal the edge wrote takes the status its body chose (a timeout
/// answers `504`), and a
/// `413` the body extractor raised inside the routes, which the stack leaves
/// in its plain form, becomes the `too-long` `OperationOutcome`.
pub async fn fhir_refusals(State(edge): State<Edge>, request: Request, next: Next) -> Response {
    let fhir = on_fhir_route(request.uri().path());
    let mut response = next.run(request).await;
    if let Some(Answered(status)) = response.extensions().get::<Answered>().copied() {
        *response.status_mut() = status;
        return response;
    }
    let plain = !response
        .headers()
        .get(http::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value.starts_with(crate::facade::media::FHIR_JSON));
    if fhir
        && plain
        && response.status() == StatusCode::PAYLOAD_TOO_LARGE
        && let Some(mut outcome) = edge.outcome_of(StatusCode::PAYLOAD_TOO_LARGE)
    {
        if let Some(id) = response
            .headers()
            .get(ferrohealth_server::request_id::HEADER)
        {
            outcome
                .headers_mut()
                .insert(ferrohealth_server::request_id::HEADER, id.clone());
        }
        return outcome;
    }
    response
}

/// Returns whether `path` is under [`BASE_PATH`].
pub(crate) fn on_fhir_route(path: &str) -> bool {
    path.strip_prefix(BASE_PATH)
        .is_some_and(|rest| rest.is_empty() || rest.starts_with('/'))
}

#[cfg(test)]
mod tests {
    use super::{Edge, on_fhir_route};
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
        let timed_out = edge().outcome_of(StatusCode::REQUEST_TIMEOUT);
        assert_eq!(
            Some(StatusCode::GATEWAY_TIMEOUT),
            timed_out.map(|answer| answer.status())
        );
        let oversized = edge().outcome_of(StatusCode::PAYLOAD_TOO_LARGE);
        assert_eq!(
            Some(StatusCode::PAYLOAD_TOO_LARGE),
            oversized.map(|answer| answer.status())
        );
    }

    #[test]
    fn any_other_answer_is_left_alone() {
        assert!(edge().outcome_of(StatusCode::NOT_FOUND).is_none());
    }
}
