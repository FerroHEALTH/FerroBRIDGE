// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The one table from a CDR outcome to a facade answer.
//!
//! No specification governs the mapping: it is FerroBRIDGE's own design with
//! both sides cited per row. ITS-REST 1.1.0
//! fixes what the CDR may answer (`ehr-codegen.openapi.yaml`, the status list
//! of each operation) and FHIR R4 fixes what a FHIR server may answer
//! (<https://hl7.org/fhir/R4/http.html>). Every row below is one [`Row`]
//! constant carrying both citations, and one wire test asserts each.
//!
//! The rows, in the order the architecture states them:
//!
//! - a CDR `422` (template validation) is a facade `422` with the
//!   `validationErrors` verbatim in `issue.diagnostics`;
//! - a CDR `412` is a `412` carrying the current `ETag`;
//! - a composition the CDR reports deleted (a `204` on the read) is a `410`;
//! - a CDR `401` propagates `WWW-Authenticate` and never becomes a `403`;
//! - a CDR `405` or `415` is a `500`, because the bridge chose a call the CDR
//!   does not offer;
//! - any CDR `5xx` is a `502` carrying the upstream status;
//! - a CDR credential the token endpoint would not issue is a `502` carrying
//!   the token endpoint's refusal;
//! - a CDR `400` on a search query is a `500`, because the bridge wrote the
//!   AQL, and a CDR `408` on one is a `502`;
//! - a connect failure or a timeout is a `502`;
//! - a composition the engine refuses to render on a read, vread or search is
//!   a `500` with `processing`, because the client sent no content.
//!
//! The Demographic API rows, ITS-REST 1.1.0 DEVELOPMENT, are [`demographic`].

pub mod demographic;

use ferrohealth_smart::client::RenewalError;
use http::StatusCode;
use openehr_its::rest::client::ClientError;
use openehr_its::rest::client::ErrorBody;

use crate::cdr::error::CdrError;
use crate::facade::outcome::Issue;
use crate::facade::outcome::IssueType;

/// One row of the table, with the citation of each side.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Row {
    /// The status the facade answers with.
    status: StatusCode,
    /// The `issue-type` code the `OperationOutcome` carries.
    issue: IssueType,
    /// Where ITS-REST 1.1.0 states the CDR side.
    openehr: &'static str,
    /// Where FHIR R4 states the facade side.
    fhir: &'static str,
}

impl Row {
    /// Returns the status the facade answers with.
    #[must_use]
    pub const fn status(&self) -> StatusCode {
        self.status
    }

    /// Returns the `issue-type` code of the authored outcome.
    #[must_use]
    pub const fn issue(&self) -> IssueType {
        self.issue
    }

    /// Returns the ITS-REST 1.1.0 citation of this row.
    #[must_use]
    pub const fn openehr_citation(&self) -> &'static str {
        self.openehr
    }

    /// Returns the FHIR R4 citation of this row.
    #[must_use]
    pub const fn fhir_citation(&self) -> &'static str {
        self.fhir
    }
}

/// A CDR `422`: the content is well formed and fails semantic validation.
pub const UNPROCESSABLE: Row = Row {
    status: StatusCode::UNPROCESSABLE_ENTITY,
    issue: IssueType::Invalid,
    openehr: "ehr-codegen.openapi.yaml, composition_create 422",
    fhir: "hl7.org/fhir/R4/http.html, create: 422 Unprocessable Entity",
};

/// A CDR `400`: the CDR could not parse or accept the request the bridge sent.
pub const BAD_REQUEST: Row = Row {
    status: StatusCode::INTERNAL_SERVER_ERROR,
    issue: IssueType::Exception,
    openehr: "ehr-codegen.openapi.yaml, composition_create 400",
    fhir: "hl7.org/fhir/R4/http.html, 500 Internal Server Error",
};

/// A CDR `412`: `If-Match` does not name the latest version.
pub const PRECONDITION_FAILED: Row = Row {
    status: StatusCode::PRECONDITION_FAILED,
    issue: IssueType::Conflict,
    openehr: "ehr-codegen.openapi.yaml, composition_update 412",
    fhir: "hl7.org/fhir/R4/http.html, update: 412 Precondition Failed",
};

/// A composition the CDR reports deleted.
///
/// A `GET` answers `204` "when the composition was deleted at the requested
/// time" (`ehr-codegen.openapi.yaml`, `composition_get`), which R4 renders as
/// `410 Gone` (<https://hl7.org/fhir/R4/http.html>, read).
pub const GONE: Row = Row {
    status: StatusCode::GONE,
    issue: IssueType::Deleted,
    openehr: "ehr-codegen.openapi.yaml, composition_get 204",
    fhir: "hl7.org/fhir/R4/http.html, read: 410 Gone",
};

/// A CDR `404`: no EHR, or no version of the resource the bridge addressed.
pub const NOT_FOUND: Row = Row {
    status: StatusCode::NOT_FOUND,
    issue: IssueType::NotFound,
    openehr: "ehr-codegen.openapi.yaml, composition_get 404",
    fhir: "hl7.org/fhir/R4/http.html, read: 404 Not Found",
};

/// A CDR `401`: the CDR refused the bridge's credentials.
///
/// The row answers `401` and never `403`: the CDR reported that it does not
/// know who is calling, which R4 states as `401 Not Authorized`
/// (<https://hl7.org/fhir/R4/http.html>). The `WWW-Authenticate` challenge is
/// propagated so a caller sees the scheme the CDR asked for.
pub const UNAUTHORIZED: Row = Row {
    status: StatusCode::UNAUTHORIZED,
    issue: IssueType::Login,
    openehr: "ITS-REST 1.1.0, Requests and responses, HTTP status codes: 401",
    fhir: "hl7.org/fhir/R4/http.html, 401 Not Authorized",
};

/// A status the CDR answered that its operation does not document.
///
/// A `405` or a `415` means the bridge chose a call the CDR does not offer, so
/// the fault is the bridge's and the answer is a `500`
/// (<https://hl7.org/fhir/R4/http.html>).
pub const UNDOCUMENTED: Row = Row {
    status: StatusCode::INTERNAL_SERVER_ERROR,
    issue: IssueType::Exception,
    openehr: "ITS-REST 1.1.0, Requests and responses, HTTP status codes",
    fhir: "hl7.org/fhir/R4/http.html, 500 Internal Server Error",
};

/// The CDR failed, or the call never reached it.
pub const UPSTREAM: Row = Row {
    status: StatusCode::BAD_GATEWAY,
    issue: IssueType::Transient,
    openehr: "ITS-REST 1.1.0, Requests and responses, HTTP status codes: 5xx",
    fhir: "hl7.org/fhir/R4/http.html, the server is a gateway to the CDR",
};

/// The credential for a CDR call could not be obtained.
///
/// The token endpoint the bridge fetches its CDR credential from was
/// unreachable, refused the bridge's own client credentials, or answered no
/// usable token, either before the CDR call or when the CDR refused the token
/// (`401`) and the bridge renewed it once. That is a fault of a server upstream
/// of the bridge, so the answer is a `502` (RFC 9110 §15.6.3) and never a `401`
/// to a caller whose own credentials were fine. No ITS-REST status governs a
/// credential the bridge could not obtain: no specification governs this: our
/// own design.
pub const CREDENTIALS: Row = Row {
    status: StatusCode::BAD_GATEWAY,
    issue: IssueType::Exception,
    openehr: "no usable CDR call: the credential for it could not be obtained or renewed",
    fhir: "hl7.org/fhir/R4/http.html, the server is a gateway to the CDR",
};

/// A CDR `400` on a search query: the CDR refused the AQL the bridge built.
///
/// The facade writes every query a search runs, so a query the CDR cannot
/// accept is the bridge's fault and a `500`, never an empty searchset.
pub const QUERY_REFUSED: Row = Row {
    status: StatusCode::INTERNAL_SERVER_ERROR,
    issue: IssueType::Exception,
    openehr: "query-codegen.openapi.yaml, query_execute_adhoc_query_body 400",
    fhir: "hl7.org/fhir/R4/http.html, 500 Internal Server Error",
};

/// A CDR `400` on a query of Cadasto's experimental AQL demographic
/// extension: the CDR does not serve the extension, or the configured query
/// is outside the part of it the CDR serves.
///
/// Neither ITS-REST nor the extension's guide defines how a CDR without the
/// extension answers, so the two cannot be told apart, and both are a fact of
/// the deployment: the server "does not support the functionality required
/// to fulfill the request" (RFC 9110 §15.6.2). No specification governs the
/// extension: our own design.
pub const EXTENSION_REFUSED: Row = Row {
    status: StatusCode::NOT_IMPLEMENTED,
    issue: IssueType::NotSupported,
    openehr: "query-codegen.openapi.yaml, query_execute_adhoc_query_body 400",
    fhir: "hl7.org/fhir/R4/operationoutcome.html, issue-type not-supported; RFC 9110 §15.6.2, 501",
};

/// A CDR `408` on a search query: the query did not finish in the CDR's time.
pub const QUERY_TIMEOUT: Row = Row {
    status: StatusCode::BAD_GATEWAY,
    issue: IssueType::Transient,
    openehr: "query-codegen.openapi.yaml, query_execute_adhoc_query_body 408",
    fhir: "hl7.org/fhir/R4/http.html, the server is a gateway to the CDR",
};

/// The identity store or another part of the bridge refused.
pub const INTERNAL: Row = Row {
    status: StatusCode::INTERNAL_SERVER_ERROR,
    issue: IssueType::Exception,
    openehr: "no CDR call was made",
    fhir: "hl7.org/fhir/R4/http.html, 500 Internal Server Error",
};

/// A stored composition the engine refuses to render as FHIR.
///
/// The CDR answered the read, vread or search with a composition, and the
/// outbound run refused it (a mapping that cannot carry it, or a produced
/// resource that breaks a base constraint). The client sent no content and
/// cannot fix it, so the fault is the server's: a `500` whose
/// `OperationOutcome` carries `processing` (<https://hl7.org/fhir/R4/http.html>
/// §3.1.0.4, <https://hl7.org/fhir/R4/valueset-issue-type.html>). A write
/// whose posted content the engine refuses keeps its `422`.
pub const OUTBOUND_REFUSED: Row = Row {
    status: StatusCode::INTERNAL_SERVER_ERROR,
    issue: IssueType::Processing,
    openehr: "ehr-codegen.openapi.yaml, composition_get 200 and query-codegen.openapi.yaml 200: the CDR answered",
    fhir: "hl7.org/fhir/R4/http.html §3.1.0.4, 500; operationoutcome.html, issue-type processing",
};

/// Returns the answer a read, vread or search gives when the engine refuses
/// to render the composition the CDR returned.
///
/// The diagnostics are the refusal's cause chain, which names the context,
/// the mapping, the element and the constraint, never a value of either
/// document.
#[must_use]
pub fn of_outbound_refusal(error: &fhirconnect::engine::traverse::error::EngineError) -> Answer {
    Answer::new(OUTBOUND_REFUSED, crate::facade::outcome::chain(error))
}

/// Returns the answer a CDR refusal of an extension query renders as.
///
/// The query is one of Cadasto's experimental AQL demographic extension,
/// whichever search issued it: the [`EXTENSION_REFUSED`] row for a `400`, the
/// [`QUERY_TIMEOUT`] row for a `408`.
///
/// The diagnostics name the extension and `query`, the search that issued
/// it, and carry the CDR's words. One deployment fact gets one answer (no
/// specification governs the extension: our own design).
#[must_use]
pub fn of_extension_refusal(upstream: &crate::cdr::error::Upstream, query: &str) -> Answer {
    let body = ErrorBody::from_bytes(upstream.body().as_bytes().to_vec());
    let words = diagnostics(upstream.status(), &body);
    let extension = crate::facade::search::identifier::EXTENSION;
    if upstream.status() == StatusCode::REQUEST_TIMEOUT {
        return Answer::new(
            QUERY_TIMEOUT,
            format!("the {extension} {query} query did not finish: {words}"),
        );
    }
    Answer::new(
        EXTENSION_REFUSED,
        format!(
            "the CDR refused the {extension} {query} query, so it does not serve the extension \
             or this query of it: {words}"
        ),
    )
}

/// What the facade answers, with the row that decided it.
#[derive(Debug, Clone)]
pub struct Answer {
    /// The row that decided the status.
    row: Row,
    /// The issue the `OperationOutcome` carries.
    issue: Issue,
    /// The `ETag` the answer carries, when the row states one.
    entity_tag: Option<String>,
    /// The `WWW-Authenticate` challenge the answer propagates, when the CDR
    /// sent one.
    challenge: Option<String>,
}

impl Answer {
    /// Returns the answer `row` decides, diagnosing `detail`.
    #[must_use]
    pub fn new(row: Row, detail: String) -> Self {
        Self {
            row,
            issue: Issue::error(row.issue).diagnosing(detail),
            entity_tag: None,
            challenge: None,
        }
    }

    /// Returns this answer carrying `tag` as its `ETag`.
    #[must_use]
    pub fn with_entity_tag(mut self, tag: impl Into<String>) -> Self {
        self.entity_tag = Some(tag.into());
        self
    }

    /// Returns the row that decided the status.
    #[must_use]
    pub const fn row(&self) -> Row {
        self.row
    }

    /// Returns the status the facade answers with.
    #[must_use]
    pub const fn status(&self) -> StatusCode {
        self.row.status
    }

    /// Returns the issue the `OperationOutcome` carries.
    #[must_use]
    pub const fn issue(&self) -> &Issue {
        &self.issue
    }

    /// Returns the `ETag` the answer carries.
    #[must_use]
    pub fn entity_tag(&self) -> Option<&str> {
        self.entity_tag.as_deref()
    }

    /// Returns the `WWW-Authenticate` challenge the answer propagates.
    #[must_use]
    pub fn challenge(&self) -> Option<&str> {
        self.challenge.as_deref()
    }
}

/// Returns the diagnostics text of one `status` answer whose body is `body`.
///
/// The openEHR error body travels verbatim: the `message` and every
/// `validationErrors` entry the CDR sent, and the raw body when the CDR sent
/// something that is not an error body. It never reaches the wire as its own
/// document (no specification governs this: our own design).
#[must_use]
pub fn diagnostics(status: StatusCode, body: &ErrorBody) -> String {
    let mut text = format!("the openEHR service answered {status}");
    match body.message() {
        Some(message) => {
            text.push_str(": ");
            text.push_str(message);
            for entry in body.validation_errors() {
                text.push_str("; ");
                text.push_str(entry);
            }
        }
        None if !body.is_empty() => {
            text.push_str(": ");
            text.push_str(&String::from_utf8_lossy(body.raw()));
        }
        None => {}
    }
    text
}

/// Returns the answer one CDR call that reached no usable documented answer
/// maps to.
///
/// Every variant here is a bridge-side, transport-side or CDR-side fault the
/// generated client or the bridge reported, and none of them is flattened into
/// an empty value.
#[must_use]
pub fn of_client_error(error: &CdrError) -> Answer {
    match *error {
        CdrError::Client { ref source, .. } => of_generated_error(source),
        CdrError::Renewal { ref source, .. } => renewal_answer(source),
        _ => Answer::new(
            INTERNAL,
            format!("the openEHR client refused this call ({})", error.kind()),
        ),
    }
}

/// Returns the answer a documented refusal of the AQL query maps to.
///
/// The query operation documents `400` and `408`
/// (`query-codegen.openapi.yaml`, `query_execute_adhoc_query_body`); either
/// one reaches the wire as its row with the CDR's own words, never as an
/// empty result.
#[must_use]
pub fn of_query_refusal(upstream: &crate::cdr::error::Upstream) -> Answer {
    let row = if upstream.status() == StatusCode::REQUEST_TIMEOUT {
        QUERY_TIMEOUT
    } else {
        QUERY_REFUSED
    };
    let body = ErrorBody::from_bytes(upstream.body().as_bytes().to_vec());
    Answer::new(row, diagnostics(upstream.status(), &body))
}

/// Returns the answer one refusal of the generated client maps to, with the
/// body the CDR sent when the call reached it.
fn of_generated_error(error: &ClientError) -> Answer {
    match *error {
        ClientError::Unauthorized {
            ref challenge,
            ref body,
            ..
        } => Answer {
            row: UNAUTHORIZED,
            issue: Issue::error(UNAUTHORIZED.issue)
                .diagnosing(diagnostics(StatusCode::UNAUTHORIZED, body)),
            entity_tag: None,
            challenge: challenge.clone(),
        },
        ClientError::ServiceFailure {
            status, ref body, ..
        } => Answer::new(UPSTREAM, diagnostics(status, body)),
        // The diagnostics walk the provider's error chain, whose token
        // errors carry the status and the OAuth error and never the secret.
        ClientError::Credentials { ref source, .. } => Answer::new(
            CREDENTIALS,
            format!(
                "no credential for the openEHR service could be obtained: {}",
                crate::facade::outcome::chain(source)
            ),
        ),
        ClientError::Transport { .. } => Answer::new(
            UPSTREAM,
            String::from("the openEHR service could not be reached"),
        ),
        ClientError::AnswerTooLarge { status, limit, .. } => Answer::new(
            UPSTREAM,
            format!(
                "the openEHR service answered {status} with a body over the {limit}-byte limit"
            ),
        ),
        ClientError::UndocumentedStatus {
            status, ref body, ..
        } => Answer::new(UNDOCUMENTED, diagnostics(status, body)),
        // NOTE: ITS-REST 1.1.0 §Requests and responses/HTTP status codes lists
        // `403` for every operation and no operation documents it, so the row is
        // the one every status outside an operation's set takes.
        ClientError::Forbidden { ref body, .. } => {
            Answer::new(UNDOCUMENTED, diagnostics(StatusCode::FORBIDDEN, body))
        }
        _ => Answer::new(
            INTERNAL,
            format!(
                "the openEHR client refused this call ({})",
                crate::cdr::error::client_kind(error)
            ),
        ),
    }
}

/// Returns the answer a refused CDR token that could not be renewed maps to.
///
/// The diagnostics walk the token error, which carries the status and the
/// OAuth error and never the secret.
fn renewal_answer(renewal: &RenewalError) -> Answer {
    Answer::new(
        CREDENTIALS,
        format!(
            "no credential for the openEHR service could be obtained: {}",
            crate::facade::outcome::chain(renewal)
        ),
    )
}

#[cfg(test)]
mod tests {
    use super::{
        BAD_REQUEST, CREDENTIALS, EXTENSION_REFUSED, GONE, NOT_FOUND, PRECONDITION_FAILED,
        QUERY_REFUSED, QUERY_TIMEOUT, UNAUTHORIZED, UNDOCUMENTED, UNPROCESSABLE, UPSTREAM,
        diagnostics, of_client_error, of_query_refusal,
    };
    use crate::cdr::error::CdrError;
    use http::{Method, StatusCode};
    use openehr_its::rest::client::{ClientError, ErrorBody};

    fn body(text: &str) -> ErrorBody {
        ErrorBody::from_bytes(text.as_bytes().to_vec())
    }

    #[test]
    fn every_row_states_both_sides() {
        for row in [
            UNPROCESSABLE,
            BAD_REQUEST,
            PRECONDITION_FAILED,
            GONE,
            NOT_FOUND,
            UNAUTHORIZED,
            UNDOCUMENTED,
            UPSTREAM,
            QUERY_REFUSED,
            EXTENSION_REFUSED,
            QUERY_TIMEOUT,
            CREDENTIALS,
        ] {
            assert!(
                !row.openehr_citation().is_empty() && !row.fhir_citation().is_empty(),
                "a row of the status table carries no citation"
            );
        }
    }

    #[test]
    fn the_table_maps_each_documented_cdr_answer() {
        assert_eq!(StatusCode::UNPROCESSABLE_ENTITY, UNPROCESSABLE.status());
        assert_eq!(
            StatusCode::PRECONDITION_FAILED,
            PRECONDITION_FAILED.status()
        );
        assert_eq!(StatusCode::GONE, GONE.status());
        assert_eq!(StatusCode::NOT_FOUND, NOT_FOUND.status());
        assert_eq!(StatusCode::BAD_GATEWAY, UPSTREAM.status());
        assert_eq!(StatusCode::INTERNAL_SERVER_ERROR, UNDOCUMENTED.status());
        assert_eq!(StatusCode::INTERNAL_SERVER_ERROR, BAD_REQUEST.status());
    }

    #[test]
    fn a_401_stays_a_401_and_propagates_its_challenge() {
        let error = CdrError::client(
            ClientError::Unauthorized {
                method: Method::GET,
                path: String::from("/ehr"),
                challenge: Some(String::from("Bearer realm=\"cdr\"")),
                body: body(""),
            },
            None,
        );
        let answer = of_client_error(&error);
        assert_eq!(StatusCode::UNAUTHORIZED, answer.status());
        assert_ne!(StatusCode::FORBIDDEN, answer.status());
        assert_eq!(Some("Bearer realm=\"cdr\""), answer.challenge());
    }

    #[test]
    fn a_5xx_becomes_a_502_carrying_the_upstream_status() {
        let error = CdrError::client(
            ClientError::ServiceFailure {
                method: Method::GET,
                path: String::from("/ehr"),
                status: StatusCode::SERVICE_UNAVAILABLE,
                body: body(""),
            },
            None,
        );
        let answer = of_client_error(&error);
        assert_eq!(StatusCode::BAD_GATEWAY, answer.status());
        let built = answer.issue().build();
        assert!(
            built
                .diagnostics
                .and_then(|text| text.value)
                .is_some_and(|text| text.contains("503")),
            "the 502 does not carry the upstream status"
        );
    }

    #[test]
    fn a_credential_that_cannot_be_obtained_is_a_502_naming_the_provider_failure() {
        let error = CdrError::client(
            ClientError::Credentials {
                method: Method::GET,
                path: String::from("/ehr"),
                source: openehr_its::rest::client::CredentialsError::new(
                    "the token endpoint answered 401 Unauthorized: invalid_client",
                ),
            },
            None,
        );
        let answer = of_client_error(&error);
        assert_eq!(CREDENTIALS, answer.row());
        assert_eq!(StatusCode::BAD_GATEWAY, answer.status());
        assert!(
            answer
                .issue()
                .diagnostics()
                .is_some_and(|text| text.contains("invalid_client")),
            "the 502 does not carry the token endpoint's refusal"
        );
    }

    #[test]
    fn a_failed_renewal_after_a_cdr_401_is_the_credentials_row() {
        let oauth = serde_json::from_value(serde_json::json!({ "error": "invalid_grant" }))
            .expect("an RFC 6749 §5.2 error body");
        let refused = ferrohealth_smart::client::TokenError::Refused {
            status: StatusCode::BAD_REQUEST,
            error: Some(oauth),
            body: String::new(),
        };
        let error = CdrError::Renewal {
            source: ferrohealth_smart::client::RenewalError::new(std::sync::Arc::new(refused)),
            upstream: None,
        };
        let answer = of_client_error(&error);
        assert_eq!(CREDENTIALS, answer.row());
        let diagnostics = answer.issue().diagnostics().unwrap_or_default();
        assert!(
            diagnostics.contains("400") && diagnostics.contains("invalid_grant"),
            "{diagnostics}"
        );
    }

    #[test]
    fn a_transport_failure_without_a_renewal_stays_the_upstream_row() {
        let error = CdrError::client(
            ClientError::Transport {
                method: Method::GET,
                path: String::from("/ehr"),
                source: openehr_its::rest::client::TransportError::Send {
                    source: Box::new(std::io::Error::other("connection refused")),
                },
            },
            None,
        );
        assert_eq!(UPSTREAM, of_client_error(&error).row());
    }

    #[test]
    fn an_answer_over_the_body_limit_becomes_a_502_naming_status_and_limit() {
        let error = CdrError::client(
            ClientError::AnswerTooLarge {
                method: Method::POST,
                path: String::from("/query/aql"),
                status: StatusCode::OK,
                limit: 1024,
            },
            None,
        );
        let answer = of_client_error(&error);
        assert_eq!(StatusCode::BAD_GATEWAY, answer.status());
        let built = answer.issue().build();
        assert!(
            built
                .diagnostics
                .and_then(|text| text.value)
                .is_some_and(|text| text.contains("200") && text.contains("1024")),
            "the 502 does not name the upstream status and the limit"
        );
    }

    #[test]
    fn a_403_is_a_status_outside_the_documented_set() {
        let error = CdrError::client(
            ClientError::Forbidden {
                method: Method::POST,
                path: String::from("/ehr"),
                body: body(r#"{"message":"no write grant","validationErrors":[]}"#),
            },
            None,
        );
        let answer = of_client_error(&error);
        assert_eq!(UNDOCUMENTED, answer.row());
        let built = answer.issue().build();
        assert!(
            built
                .diagnostics
                .and_then(|text| text.value)
                .is_some_and(|text| text.contains("403") && text.contains("no write grant")),
            "the 500 does not carry the upstream status and message"
        );
    }

    #[test]
    fn the_validation_errors_travel_verbatim() {
        let text = diagnostics(
            StatusCode::UNPROCESSABLE_ENTITY,
            &body(
                r#"{"message":"the composition is invalid","validationErrors":["/content[0] is required"]}"#,
            ),
        );
        assert!(text.contains("the composition is invalid"), "{text}");
        assert!(text.contains("/content[0] is required"), "{text}");
    }

    #[test]
    fn a_refused_query_is_a_500_and_a_timed_out_one_a_502_with_the_cdr_words() {
        let refused = crate::cdr::error::Upstream::new(
            StatusCode::BAD_REQUEST,
            http::HeaderMap::new(),
            String::from(r#"{"message":"unknown path c/x","validationErrors":[]}"#),
        );
        let answer = of_query_refusal(&refused);
        assert_eq!(QUERY_REFUSED, answer.row());
        assert_eq!(StatusCode::INTERNAL_SERVER_ERROR, answer.status());
        assert!(
            answer
                .issue()
                .diagnostics()
                .is_some_and(|text| text.contains("400") && text.contains("unknown path c/x")),
            "the 500 does not carry the CDR's refusal"
        );
        let timed_out = crate::cdr::error::Upstream::new(
            StatusCode::REQUEST_TIMEOUT,
            http::HeaderMap::new(),
            String::new(),
        );
        let answer = of_query_refusal(&timed_out);
        assert_eq!(QUERY_TIMEOUT, answer.row());
        assert_eq!(StatusCode::BAD_GATEWAY, answer.status());
    }

    #[test]
    fn an_extension_query_refused_is_a_501_and_timed_out_a_502_whatever_the_search() {
        let refused = crate::cdr::error::Upstream::new(
            StatusCode::BAD_REQUEST,
            http::HeaderMap::new(),
            String::from(r#"{"message":"FROM source class `PERSON` is not in scope"}"#),
        );
        for query in ["identifier", "party"] {
            let answer = super::of_extension_refusal(&refused, query);
            assert_eq!(EXTENSION_REFUSED, answer.row());
            assert_eq!(StatusCode::NOT_IMPLEMENTED, answer.status());
            assert!(
                answer.issue().diagnostics().is_some_and(|text| text
                    .contains(crate::facade::search::identifier::EXTENSION)
                    && text.contains(query)
                    && text.contains("not in scope")),
                "{query}: the 501 does not name the extension and carry the CDR's words"
            );
        }
        let timed_out = crate::cdr::error::Upstream::new(
            StatusCode::REQUEST_TIMEOUT,
            http::HeaderMap::new(),
            String::new(),
        );
        assert_eq!(
            QUERY_TIMEOUT,
            super::of_extension_refusal(&timed_out, "party").row()
        );
    }

    #[test]
    fn a_body_that_is_no_error_document_still_reaches_the_diagnostics() {
        let refusal = body("a plain-text refusal");
        assert!(
            diagnostics(StatusCode::UNPROCESSABLE_ENTITY, &refusal)
                .contains("a plain-text refusal")
        );
    }

    #[test]
    fn an_error_body_member_outside_the_openehr_shape_stays_out_of_the_diagnostics() {
        let text = diagnostics(
            StatusCode::CONFLICT,
            &body(r#"{"message":"an EHR exists","trace":"ferrobridge-stack-trace-0001"}"#),
        );
        assert!(text.contains("an EHR exists"), "{text}");
        assert!(!text.contains("ferrobridge-stack-trace-0001"), "{text}");
    }
}
