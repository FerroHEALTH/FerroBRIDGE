// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The rows from a refused onward exchange to a facade answer: the caller's
//! token could not be exchanged for one the CDR accepts, so no CDR call was
//! made.
//!
//! The token endpoint side is RFC 6749 §5.2 and RFC 8693 §2.2.2
//! (`docs/specs/ietf-oauth/`), and the Microsoft identity platform's
//! "On-behalf-of flow", "Error response example", for `interaction_required`
//! (`docs/specs/entra-docs/`); FHIR R4 fixes what a FHIR server answers
//! (<https://hl7.org/fhir/R4/http.html>). Which refusal is the caller's and
//! which the bridge's is no specification's: our own design.

use ferrohealth_smart::client::onward::OnwardError;
use ferrohealth_smart::client::onward::Rejected;
use http::StatusCode;

use crate::cdr::onward::OnwardRefusal;
use crate::facade::outcome::Issue;
use crate::facade::outcome::IssueType;

use super::Answer;
use super::Row;

/// A caller the onward lane does not carry.
///
/// It is a kind its issuer issues no token for, a caller without a bearer
/// token, or a token the issuer may not exchange. The bridge's own credential is never sent instead.
pub const ONWARD_NOT_CARRIED: Row = Row {
    status: StatusCode::FORBIDDEN,
    issue: IssueType::Forbidden,
    openehr: "no CDR call was made: the caller is not carried onto the CDR",
    fhir: "hl7.org/fhir/R4/http.html, 403 Forbidden",
};

/// The token endpoint refused the exchange for a reason of the caller's
/// token or of the target (RFC 8693 §2.2.2, `invalid_request` and
/// `invalid_target`), so the facade answers the status it answered.
pub const ONWARD_REFUSED: Row = Row {
    status: StatusCode::BAD_REQUEST,
    issue: IssueType::Forbidden,
    openehr: "no CDR call was made: the token endpoint refused the exchange",
    fhir: "hl7.org/fhir/R4/http.html, the status of the refusal",
};

/// The caller must authenticate again: the on-behalf-of flow answered
/// `interaction_required`, or the caller's token expired before the
/// exchange.
pub const ONWARD_INTERACTION: Row = Row {
    status: StatusCode::UNAUTHORIZED,
    issue: IssueType::Login,
    openehr: "no CDR call was made: the caller must authenticate again",
    fhir: "hl7.org/fhir/R4/http.html, 401 Not Authorized",
};

/// The token endpoint failed, refused the bridge's own client
/// authentication, or issued a token the onward check refuses: a fault
/// upstream of the facade (RFC 9110 §15.6.3).
pub const ONWARD_UPSTREAM: Row = Row {
    status: StatusCode::BAD_GATEWAY,
    issue: IssueType::Exception,
    openehr: "no CDR call was made: no usable onward token was issued",
    fhir: "hl7.org/fhir/R4/http.html, the server is a gateway to the CDR",
};

/// The onward lane is configured so that no request can be sent.
pub const ONWARD_SETTINGS: Row = Row {
    status: StatusCode::INTERNAL_SERVER_ERROR,
    issue: IssueType::Exception,
    openehr: "no CDR call was made: the onward lane is misconfigured",
    fhir: "hl7.org/fhir/R4/http.html, 500 Internal Server Error",
};

/// Returns the answer a refused onward exchange renders as.
///
/// The diagnostics walk the refusal's cause chain, which names the status,
/// the OAuth error and the check that failed, never a token or a claim
/// value.
#[must_use]
pub fn of_onward_refusal(refusal: &OnwardRefusal) -> Answer {
    let detail = format!(
        "the request was not carried onto the CDR: {}",
        ferrohealth_server::run::chain(refusal)
    );
    let OnwardRefusal::Exchange(error) = refusal else {
        return Answer::new(ONWARD_NOT_CARRIED, detail);
    };
    match error {
        OnwardError::InteractionRequired { challenge, .. } => {
            let mut value = String::from("Bearer error=\"interaction_required\"");
            if let Some(claims) = challenge {
                // NOTE: RFC 9110 §5.6.4, a quoted-string escapes `"` and `\`; the vendored
                // on-behalf-of page fixes no encoding of the claims challenge.
                value.push_str(", claims=\"");
                value.push_str(&claims.replace('\\', "\\\\").replace('"', "\\\""));
                value.push('"');
            }
            challenged(ONWARD_INTERACTION, detail, value)
        }
        OnwardError::Rejected(Rejected::Expired) => challenged(
            ONWARD_INTERACTION,
            detail,
            String::from("Bearer error=\"invalid_token\""),
        ),
        OnwardError::NoSubjectToken
        | OnwardError::UserPrincipalRequired
        | OnwardError::SubjectAudience
        | OnwardError::Rejected(Rejected::NotMapped) => Answer::new(ONWARD_NOT_CARRIED, detail),
        OnwardError::Token(token) => match token.status() {
            // NOTE: RFC 6749 §5.2, `invalid_client` is a 401 on the bridge's own
            // authentication, so only another 4xx is the caller's refusal.
            Some(status) if status.is_client_error() && status != StatusCode::UNAUTHORIZED => {
                Answer::new(
                    Row {
                        status,
                        ..ONWARD_REFUSED
                    },
                    detail,
                )
            }
            _ => Answer::new(ONWARD_UPSTREAM, detail),
        },
        OnwardError::RefreshToken
        | OnwardError::IssuedTokenType { .. }
        | OnwardError::Rejected(_) => Answer::new(ONWARD_UPSTREAM, detail),
        _ => Answer::new(ONWARD_SETTINGS, detail),
    }
}

/// Returns the answer `row` decides, carrying `challenge` in
/// `WWW-Authenticate` (RFC 6750 §3).
fn challenged(row: Row, detail: String, challenge: String) -> Answer {
    Answer {
        row,
        issue: Issue::error(row.issue).diagnosing(detail),
        entity_tag: None,
        challenge: Some(challenge),
    }
}

#[cfg(test)]
mod tests {
    use super::{
        ONWARD_INTERACTION, ONWARD_NOT_CARRIED, ONWARD_SETTINGS, ONWARD_UPSTREAM, of_onward_refusal,
    };
    use crate::cdr::onward::OnwardRefusal;
    use ferrohealth_smart::client::OAuthError;
    use ferrohealth_smart::client::TokenError;
    use ferrohealth_smart::client::onward::OnwardError;
    use ferrohealth_smart::client::onward::Rejected;
    use http::StatusCode;

    fn oauth(error: &str) -> OAuthError {
        serde_json::from_value(serde_json::json!({ "error": error })).expect("an OAuth error")
    }

    fn refused(status: StatusCode, error: &str) -> OnwardRefusal {
        OnwardRefusal::Exchange(OnwardError::Token(TokenError::Refused {
            status,
            error: Some(oauth(error)),
        }))
    }

    #[test]
    fn a_caller_not_carried_is_forbidden() {
        let answer = of_onward_refusal(&OnwardRefusal::NotCarried { caller: "guest" });
        assert_eq!(ONWARD_NOT_CARRIED.status(), answer.status());
        assert_eq!(StatusCode::FORBIDDEN, answer.status());
        assert_eq!(
            StatusCode::FORBIDDEN,
            of_onward_refusal(&OnwardRefusal::NoBearer).status()
        );
    }

    #[test]
    fn an_invalid_request_keeps_the_upstream_status_and_names_the_error() {
        // RFC 8693 §2.2.2: an unacceptable subject token is `invalid_request`.
        let answer = of_onward_refusal(&refused(StatusCode::BAD_REQUEST, "invalid_request"));
        assert_eq!(StatusCode::BAD_REQUEST, answer.status());
        let words = answer.issue().diagnostics().unwrap_or_default();
        assert!(words.contains("400"), "{words}");
        assert!(words.contains("invalid_request"), "{words}");
    }

    #[test]
    fn the_bridge_client_refused_or_a_failing_endpoint_is_a_bad_gateway() {
        // RFC 6749 §5.2: `invalid_client` is the bridge's own authentication.
        for (status, error) in [
            (StatusCode::UNAUTHORIZED, "invalid_client"),
            (StatusCode::INTERNAL_SERVER_ERROR, "server_error"),
        ] {
            let answer = of_onward_refusal(&refused(status, error));
            assert_eq!(ONWARD_UPSTREAM.status(), answer.status());
        }
        let rejected = of_onward_refusal(&OnwardRefusal::Exchange(OnwardError::Rejected(
            Rejected::Audience,
        )));
        assert_eq!(StatusCode::BAD_GATEWAY, rejected.status());
    }

    #[test]
    fn interaction_required_is_a_401_carrying_the_claims_challenge() {
        let answer =
            of_onward_refusal(&OnwardRefusal::Exchange(OnwardError::InteractionRequired {
                status: StatusCode::BAD_REQUEST,
                error: oauth("interaction_required"),
                challenge: Some(String::from("{\"access_token\":{}}")),
            }));
        assert_eq!(ONWARD_INTERACTION.status(), answer.status());
        assert_eq!(
            Some("Bearer error=\"interaction_required\", claims=\"{\\\"access_token\\\":{}}\""),
            answer.challenge()
        );
    }

    #[test]
    fn a_settings_fault_is_the_bridge_s() {
        let answer = of_onward_refusal(&OnwardRefusal::Exchange(OnwardError::NoAudience));
        assert_eq!(ONWARD_SETTINGS.status(), answer.status());
    }
}
