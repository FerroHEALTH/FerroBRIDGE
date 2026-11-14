// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The rows from an access directive exchange to a facade answer.
//!
//! The CDR side is its `EHR_ACCESS` read, an extension shaped on ITS-REST
//! `ehr_status_get_at_time`, and the CONTRIBUTION that commits a new
//! version (`ehr-codegen.openapi.yaml`, `contribution_create`); FHIR R4 fixes
//! what a FHIR server answers (<https://hl7.org/fhir/R4/http.html>). No
//! specification governs the mapping: our own design, cited per row as the
//! rest of the table is.

use http::StatusCode;
use openehr_its::rest::client::ErrorBody;

use crate::cdr::ehr_access::AccessError;
use crate::cdr::error::Upstream;
use crate::consent::Refused;
use crate::consent::exchange::ExchangeError;
use crate::facade::outcome::IssueType;

use super::Answer;
use super::Row;
use super::diagnostics;

/// A CDR `404` on the `EHR_ACCESS` read or the commit: no such EHR.
pub const ACCESS_NOT_FOUND: Row = Row {
    status: StatusCode::NOT_FOUND,
    issue: IssueType::NotFound,
    openehr: "ehr-codegen.openapi.yaml, ehr_status_get_at_time and contribution_create 404",
    fhir: "hl7.org/fhir/R4/http.html, read: 404 Not Found",
};

/// A CDR `409` or `412` on the commit: another version came first.
pub const ACCESS_CONFLICT: Row = Row {
    status: StatusCode::CONFLICT,
    issue: IssueType::Conflict,
    openehr: "ehr-codegen.openapi.yaml, contribution_create 409",
    fhir: "hl7.org/fhir/R4/http.html, 409 Conflict (version conflict management)",
};

/// A CDR `400` or `422` on the commit: an invariant of the settings, or a
/// party a directive names that the demographic store does not hold.
pub const ACCESS_REFUSED: Row = Row {
    status: StatusCode::UNPROCESSABLE_ENTITY,
    issue: IssueType::BusinessRule,
    openehr: "ehr-codegen.openapi.yaml, contribution_create 400 and the CDR's 422",
    fhir: "hl7.org/fhir/R4/http.html, create: 422 Unprocessable Entity",
};

/// A CDR `403`: the CDR refused the caller the bridge reached it for.
pub const ACCESS_FORBIDDEN: Row = Row {
    status: StatusCode::FORBIDDEN,
    issue: IssueType::Forbidden,
    openehr: "ITS-REST 1.1.0, Requests and responses, HTTP status codes: 403",
    fhir: "hl7.org/fhir/R4/http.html, 403 Forbidden",
};

/// A document with a construct the directives cannot hold, refused whole
/// with the element named.
pub const DOCUMENT_REFUSED: Row = Row {
    status: StatusCode::UNPROCESSABLE_ENTITY,
    issue: IssueType::NotSupported,
    openehr: "no CDR call was made",
    fhir: "hl7.org/fhir/R4/http.html, create: 422 Unprocessable Entity",
};

/// A directive in force that the document asked for cannot state, refused
/// with the attribute named; the client sent no content.
pub const DIRECTIVE_UNSTATABLE: Row = Row {
    status: StatusCode::INTERNAL_SERVER_ERROR,
    issue: IssueType::Processing,
    openehr: "the CDR answered the EHR_ACCESS read",
    fhir: "hl7.org/fhir/R4/http.html §3.1.0.4, 500; operationoutcome.html, issue-type processing",
};

/// The `EHR_ACCESS` carries settings of another access control scheme.
pub const ACCESS_SCHEME: Row = Row {
    status: StatusCode::CONFLICT,
    issue: IssueType::Conflict,
    openehr: "the CDR answered the EHR_ACCESS read; RM EHR IM, ACCESS_CONTROL_SETTINGS",
    fhir: "hl7.org/fhir/R4/http.html, 409 Conflict",
};

/// The terminology server failed to translate an action code.
pub const TERMINOLOGY_FAILED: Row = Row {
    status: StatusCode::BAD_GATEWAY,
    issue: IssueType::Transient,
    openehr: "no CDR call was made",
    fhir: "hl7.org/fhir/R4/http.html, the server is a gateway to the terminology server",
};

/// Returns the answer a document refusal renders as.
#[must_use]
pub fn of_document_refusal(refused: &Refused) -> Answer {
    Answer::new(DOCUMENT_REFUSED, refused.to_string())
}

/// Returns the answer a directive the answer cannot state renders as.
#[must_use]
pub fn of_unstatable(refused: &Refused) -> Answer {
    Answer::new(DIRECTIVE_UNSTATABLE, refused.to_string())
}

/// Returns the answer an exchange that failed renders as.
#[must_use]
pub fn of_exchange_error(error: &ExchangeError) -> Answer {
    match error {
        ExchangeError::Access(access) => of_access_error(access),
        ExchangeError::Scheme(_) => {
            Answer::new(ACCESS_SCHEME, ferrohealth_server::run::chain(error))
        }
        ExchangeError::Terminology(_) => {
            Answer::new(TERMINOLOGY_FAILED, ferrohealth_server::run::chain(error))
        }
    }
}

/// Returns the answer one failed `EHR_ACCESS` or `ACCESS_POLICY` call
/// renders as.
#[must_use]
pub fn of_access_error(error: &AccessError) -> Answer {
    let row = match error {
        AccessError::Cdr(cdr) => return super::of_client_error(cdr),
        AccessError::NotFound { .. } => ACCESS_NOT_FOUND,
        AccessError::Conflict { .. } => ACCESS_CONFLICT,
        AccessError::Refused { .. } => ACCESS_REFUSED,
        AccessError::Denied { upstream } if upstream.status() == StatusCode::FORBIDDEN => {
            ACCESS_FORBIDDEN
        }
        AccessError::Denied { .. } => super::UNAUTHORIZED,
        AccessError::Undocumented { upstream } if upstream.status().is_server_error() => {
            super::UPSTREAM
        }
        AccessError::Undocumented { .. } => super::UNDOCUMENTED,
        _ => super::INTERNAL,
    };
    match error.upstream() {
        Some(upstream) => Answer::new(row, words(upstream)),
        None => Answer::new(row, ferrohealth_server::run::chain(error)),
    }
}

/// Returns the CDR's words for `upstream`.
fn words(upstream: &Upstream) -> String {
    let body = ErrorBody::from_bytes(upstream.body().as_bytes().to_vec());
    diagnostics(upstream.status(), &body)
}

#[cfg(test)]
mod tests {
    use super::{
        ACCESS_CONFLICT, ACCESS_FORBIDDEN, ACCESS_NOT_FOUND, ACCESS_REFUSED, of_access_error,
    };
    use crate::cdr::ehr_access::AccessError;
    use crate::cdr::error::Upstream;
    use http::{HeaderMap, StatusCode};

    fn upstream(status: StatusCode) -> Box<Upstream> {
        Box::new(Upstream::new(
            status,
            HeaderMap::new(),
            String::from(r#"{"message":"synthetic refusal"}"#),
        ))
    }

    #[test]
    fn each_cdr_refusal_has_its_row_and_carries_the_cdr_words() {
        let cases = [
            (
                AccessError::NotFound {
                    upstream: upstream(StatusCode::NOT_FOUND),
                },
                ACCESS_NOT_FOUND,
            ),
            (
                AccessError::Conflict {
                    upstream: upstream(StatusCode::CONFLICT),
                },
                ACCESS_CONFLICT,
            ),
            (
                AccessError::Refused {
                    upstream: upstream(StatusCode::UNPROCESSABLE_ENTITY),
                },
                ACCESS_REFUSED,
            ),
            (
                AccessError::Denied {
                    upstream: upstream(StatusCode::FORBIDDEN),
                },
                ACCESS_FORBIDDEN,
            ),
        ];
        for (error, row) in cases {
            let answer = of_access_error(&error);
            assert_eq!(answer.row(), row);
            assert!(
                answer
                    .issue()
                    .diagnostics()
                    .is_some_and(|text| text.contains("synthetic refusal")),
                "{error}"
            );
        }
    }
}
