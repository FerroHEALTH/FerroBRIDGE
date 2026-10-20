// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The answers of the generated ITS-REST client's refusals and of a CDR token
//! that could not be renewed, by the rows of the parent table.

use ferrohealth_smart::client::RenewalError;
use http::StatusCode;
use openehr_its::rest::client::ClientError;

use super::Answer;
use super::CREDENTIALS;
use super::INTERNAL;
use super::UNAUTHORIZED;
use super::UNDOCUMENTED;
use super::UPSTREAM;
use super::diagnostics;
use crate::facade::outcome::Issue;

/// Returns the answer one refusal of the generated client maps to, with the
/// body the CDR sent when the call reached it.
pub(super) fn of_generated_error(error: &ClientError) -> Answer {
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
pub(super) fn renewal_answer(renewal: &RenewalError) -> Answer {
    Answer::new(
        CREDENTIALS,
        format!(
            "no credential for the openEHR service could be obtained: {}",
            crate::facade::outcome::chain(renewal)
        ),
    )
}
