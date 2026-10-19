// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The rows from a Demographic API refusal to a facade answer, ITS-REST 1.1.0
//! Demographic API, DEVELOPMENT.
//!
//! No specification governs the mapping: our own design, with both sides
//! cited per row as the rest of the table is. `demographic-codegen.openapi.yaml`
//! documents `400`, `404`, `409`, `412` and `422` across the party operations
//! (`components.responses`), and FHIR R4 fixes what a FHIR server answers
//! (<https://hl7.org/fhir/R4/http.html>).

use http::StatusCode;
use openehr_its::rest::client::ErrorBody;

use crate::cdr::error::Upstream;
use crate::facade::outcome::IssueType;

use super::Answer;
use super::Row;
use super::diagnostics;

/// A Demographic API `400`: the request the bridge sent could not be parsed,
/// or the party it names is already deleted.
pub const DEMOGRAPHIC_BAD_REQUEST: Row = Row {
    status: StatusCode::INTERNAL_SERVER_ERROR,
    issue: IssueType::Exception,
    openehr: "demographic-codegen.openapi.yaml, components.responses 400 and 400_already_deleted",
    fhir: "hl7.org/fhir/R4/http.html, 500 Internal Server Error",
};

/// A Demographic API `404`: no party, or no version of it, at the address.
pub const DEMOGRAPHIC_NOT_FOUND: Row = Row {
    status: StatusCode::NOT_FOUND,
    issue: IssueType::NotFound,
    openehr: "demographic-codegen.openapi.yaml, components.responses 404",
    fhir: "hl7.org/fhir/R4/http.html, read: 404 Not Found",
};

/// A Demographic API `409`: the `uid_based_id` of a delete does not name the
/// latest version.
pub const DEMOGRAPHIC_CONFLICT: Row = Row {
    status: StatusCode::CONFLICT,
    issue: IssueType::Conflict,
    openehr: "demographic-codegen.openapi.yaml, components.responses 409_PERSON_with_uid_based_id",
    fhir: "hl7.org/fhir/R4/http.html, 409 Conflict",
};

/// A Demographic API `412`: `If-Match` does not name the latest version.
pub const DEMOGRAPHIC_PRECONDITION_FAILED: Row = Row {
    status: StatusCode::PRECONDITION_FAILED,
    issue: IssueType::Conflict,
    openehr: "demographic-codegen.openapi.yaml, components.responses 412_PERSON",
    fhir: "hl7.org/fhir/R4/http.html, update: 412 Precondition Failed",
};

/// A Demographic API `422`: the party is well formed and fails semantic
/// validation.
pub const DEMOGRAPHIC_UNPROCESSABLE: Row = Row {
    status: StatusCode::UNPROCESSABLE_ENTITY,
    issue: IssueType::Invalid,
    openehr: "demographic-codegen.openapi.yaml, components.responses 422",
    fhir: "hl7.org/fhir/R4/http.html, create: 422 Unprocessable Entity",
};

/// Returns the answer a documented Demographic API refusal maps to, or `None`
/// when `upstream` is no refusal the Demographic API documents.
///
/// A `409` and a `412` carry the latest `version_uid` in their `ETag`, which
/// the answer keeps.
#[must_use]
pub fn of_demographic_refusal(upstream: &Upstream) -> Option<Answer> {
    let row = match upstream.status() {
        StatusCode::BAD_REQUEST => DEMOGRAPHIC_BAD_REQUEST,
        StatusCode::NOT_FOUND => DEMOGRAPHIC_NOT_FOUND,
        StatusCode::CONFLICT => DEMOGRAPHIC_CONFLICT,
        StatusCode::PRECONDITION_FAILED => DEMOGRAPHIC_PRECONDITION_FAILED,
        StatusCode::UNPROCESSABLE_ENTITY => DEMOGRAPHIC_UNPROCESSABLE,
        _ => return None,
    };
    let body = ErrorBody::from_bytes(upstream.body().as_bytes().to_vec());
    let answer = Answer::new(row, diagnostics(upstream.status(), &body));
    let tagged = matches!(
        row.status,
        StatusCode::CONFLICT | StatusCode::PRECONDITION_FAILED
    );
    Some(match upstream.header("etag") {
        Some(tag) if tagged => answer.with_entity_tag(tag),
        _ => answer,
    })
}
