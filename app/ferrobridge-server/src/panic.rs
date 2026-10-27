// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The body of a request whose handler panicked, which the panic catch of
//! `ferrohealth-server` answers `500` with.
//!
//! The release profile pins `panic = "unwind"`, which is what lets
//! `std::panic::catch_unwind` turn an unwound handler into a response
//! (<https://doc.rust-lang.org/cargo/reference/profiles.html#panic>). The
//! panic message reaches neither the body nor the log, since a handler may
//! have panicked over clinical content. Under the FHIR base the body is an
//! `OperationOutcome` with the `exception` issue, because R4 asks a server to
//! answer an error with one (<https://hl7.org/fhir/R4/http.html>, §3.1.0.4);
//! every other route answers the family's JSON object.

use std::sync::Arc;

use ferrohealth_server::panic::PanicBody;
use http::StatusCode;

use crate::facade::outcome::{Issue, IssueType};

/// Returns the panic body: an `OperationOutcome` naming the request id under
/// the FHIR base, the family's JSON body everywhere else.
#[must_use]
pub fn body() -> PanicBody {
    let plain = ferrohealth_server::panic::default_body();
    Arc::new(move |uri, request_id| {
        if crate::facade::edge::on_fhir_route(uri.path()) {
            crate::facade::reply::issue(
                StatusCode::INTERNAL_SERVER_ERROR,
                Issue::error(IssueType::Exception).diagnosing(format!(
                    "the server failed while handling this request; request id {request_id}"
                )),
            )
        } else {
            plain(uri, request_id)
        }
    })
}

#[cfg(test)]
mod tests {
    use super::body;
    use http::{StatusCode, Uri, header};

    #[test]
    fn a_fhir_route_answers_an_operation_outcome_and_another_route_json() {
        let render = body();
        let fhir = render(&Uri::from_static("/fhir/Patient/1"), "id-1");
        assert_eq!(StatusCode::INTERNAL_SERVER_ERROR, fhir.status());
        assert!(
            fhir.headers()
                .get(header::CONTENT_TYPE)
                .and_then(|value| value.to_str().ok())
                .is_some_and(|value| value.starts_with("application/fhir+json")),
            "{:?}",
            fhir.headers()
        );
        let plain = render(&Uri::from_static("/operator/mappings"), "id-2");
        assert_eq!(StatusCode::INTERNAL_SERVER_ERROR, plain.status());
        assert!(
            plain
                .headers()
                .get(header::CONTENT_TYPE)
                .and_then(|value| value.to_str().ok())
                .is_some_and(|value| value.starts_with("application/json")),
            "{:?}",
            plain.headers()
        );
    }
}
