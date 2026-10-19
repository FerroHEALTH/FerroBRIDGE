// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The media type a `POST [base]/{type}/_search` body travels in.

use http::HeaderMap;
use http::StatusCode;
use http::header;

use crate::facade::handlers::Refusal;
use crate::facade::outcome::Issue;
use crate::facade::outcome::IssueType;
use crate::facade::reply;

/// The media type of a `POST _search` body.
///
/// The parameters travel "as an application/x-www-form-urlencoded submission
/// for a POST" (<https://hl7.org/fhir/R4/search.html>, §3.1.1.2).
const FORM: &str = "application/x-www-form-urlencoded";

/// Checks that a `POST _search` body is a form.
pub(super) fn check_form(headers: &HeaderMap) -> Result<(), Refusal> {
    let found = headers
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .map(|text| {
            text.split(';')
                .next()
                .unwrap_or(text)
                .trim()
                .to_ascii_lowercase()
        });
    if found.as_deref() == Some(FORM) {
        return Ok(());
    }
    Err(reply::refusal(
        StatusCode::UNSUPPORTED_MEDIA_TYPE,
        Issue::error(IssueType::NotSupported).diagnosing(format!(
            "a POST _search carries its parameters as {FORM}, and this request sent {}",
            found.as_deref().unwrap_or("no Content-Type")
        )),
    ))
}
