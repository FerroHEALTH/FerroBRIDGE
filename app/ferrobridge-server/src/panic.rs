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
//! every other route answers an RFC 9457 `about:blank` problem with a fixed
//! detail.

use std::sync::Arc;

use ferrohealth_server::panic::PanicBody;
use ferrohealth_server::request_id::RequestIds;
use http::StatusCode;

use crate::facade::outcome::{Issue, IssueType};

/// Returns the panic body: an `OperationOutcome` under the FHIR base, an
/// RFC 9457 problem naming the server's request id everywhere else.
///
/// The outcome names the request id the response echoes in `X-Request-Id`,
/// and the server's own as well when the client sent its own, since the
/// server's is the one the panic line of the log names.
#[must_use]
pub fn body() -> PanicBody {
    let plain = ferrohealth_server::problem::panic_body();
    Arc::new(move |uri, ids| {
        if crate::facade::edge::on_fhir_route(uri.path()) {
            crate::facade::reply::issue(
                StatusCode::INTERNAL_SERVER_ERROR,
                Issue::error(IssueType::Exception).diagnosing(diagnostics(ids)),
            )
        } else {
            plain(uri, ids)
        }
    })
}

/// The diagnostics of the outcome a panicked FHIR request answers.
fn diagnostics(ids: RequestIds<'_>) -> String {
    // NOTE: no specification governs this: our own design; the client quotes the
    // id it sees, and the operator finds the log line by the server's.
    if ids.echoed == ids.server {
        format!(
            "the server failed while handling this request; request id {}",
            ids.server
        )
    } else {
        format!(
            "the server failed while handling this request; request id {}, \
             server request id {}",
            ids.echoed, ids.server
        )
    }
}

#[cfg(test)]
mod tests {
    use super::{body, diagnostics};
    use ferrohealth_server::request_id::RequestIds;
    use http::{StatusCode, Uri, header};

    const SERVER: &str = "67e55044-10b1-426f-9247-bb680e5fe0c8";

    #[test]
    fn the_diagnostics_name_the_echoed_id_and_the_server_s_when_they_differ() {
        let own = diagnostics(RequestIds::new(SERVER, None));
        assert!(own.ends_with(&format!("request id {SERVER}")), "{own}");
        assert_eq!(1, own.matches(SERVER).count(), "{own}");
        let client = diagnostics(RequestIds::new(SERVER, Some("corr-1")));
        assert!(
            client.contains("request id corr-1, server request id ") && client.ends_with(SERVER),
            "{client}"
        );
    }

    #[test]
    fn a_fhir_route_answers_an_operation_outcome_and_another_route_a_problem() {
        let render = body();
        let fhir = render(
            &Uri::from_static("/fhir/Patient/1"),
            RequestIds::new(SERVER, Some("id-1")),
        );
        assert_eq!(StatusCode::INTERNAL_SERVER_ERROR, fhir.status());
        assert!(
            fhir.headers()
                .get(header::CONTENT_TYPE)
                .and_then(|value| value.to_str().ok())
                .is_some_and(|value| value.starts_with("application/fhir+json")),
            "{:?}",
            fhir.headers()
        );
        let plain = render(
            &Uri::from_static("/operator/mappings"),
            RequestIds::new(SERVER, Some("id-2")),
        );
        assert_eq!(StatusCode::INTERNAL_SERVER_ERROR, plain.status());
        assert!(
            plain
                .headers()
                .get(header::CONTENT_TYPE)
                .and_then(|value| value.to_str().ok())
                .is_some_and(|value| value.starts_with("application/problem+json")),
            "{:?}",
            plain.headers()
        );
    }
}
