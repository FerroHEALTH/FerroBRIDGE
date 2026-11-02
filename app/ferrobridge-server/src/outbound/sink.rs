// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The FHIR endpoint the outbound lane delivers to: one R4 `transaction`
//! `Bundle` per event, `POST`ed to the service base.
//!
//! A transaction "is processed as a single atomic unit" and is sent to the
//! base (<https://hl7.org/fhir/R4/http.html#transaction>); its `PUT` entries
//! create or update each resource by its id and its `DELETE` entries retract
//! one, so a delivery sent again changes nothing (at least once). A `2xx` is
//! the endpoint's acceptance. A `4xx` other than `401`, `408` and `429`
//! refuses the content, which a second delivery would not change; every
//! other failure may pass. The Bundle names patients, so it never reaches a
//! log and no error carries it or the endpoint's answer.

use std::time::Duration;

use ferrohealth_smart::client::cache::TokenSource;
use http::StatusCode;
use http::header::AUTHORIZATION;
use http::header::CONTENT_TYPE;
use secrecy::ExposeSecret;
use secrecy::SecretString;
use url::Url;

/// The media type a Bundle is sent as (<https://hl7.org/fhir/R4/http.html#mime-type>).
const FHIR_JSON: &str = "application/fhir+json";

/// How the endpoint is authenticated.
#[derive(Debug, Clone)]
pub enum Credentials {
    /// No credential.
    None,
    /// RFC 7617 Basic authentication.
    Basic {
        /// The user-id.
        user: String,
        /// The password.
        password: SecretString,
    },
    /// The OAuth 2.0 client credentials grant (RFC 6749 §4.4), a bearer
    /// token (RFC 6750) renewed once on a `401`.
    Bearer(TokenSource),
}

/// Why a delivery was not accepted.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum SinkError {
    /// The endpoint refused the content with a `4xx` other than `401`, `408`
    /// or `429`: the same delivery would be refused again.
    #[error("the FHIR endpoint refused the delivery with {status}")]
    Refused {
        /// The status.
        status: StatusCode,
    },
    /// The endpoint answered a status that may pass: a `5xx`, `401`, `408`,
    /// `429`, or one a transaction gives no meaning to.
    #[error("the FHIR endpoint answered {status}")]
    Unavailable {
        /// The status.
        status: StatusCode,
    },
    /// The request could not be sent, or no answer arrived in time.
    #[error("the FHIR endpoint could not be reached")]
    Transport(#[source] reqwest::Error),
    /// No bearer token could be had.
    #[error("no token for the FHIR endpoint could be had")]
    Token(#[source] Box<dyn std::error::Error + Send + Sync>),
}

impl SinkError {
    /// Returns whether delivering the same Bundle again may succeed.
    #[must_use]
    pub const fn is_transient(&self) -> bool {
        !matches!(self, Self::Refused { .. })
    }
}

/// The endpoint.
#[derive(Debug, Clone)]
pub struct Sink {
    /// The FHIR service base the Bundle is posted to.
    base: Url,
    /// The HTTP client, which follows no redirect.
    http: reqwest::Client,
    /// How long one delivery may take.
    timeout: Duration,
    /// How the endpoint is authenticated.
    credentials: Credentials,
}

impl Sink {
    /// Returns the endpoint at `base`, each delivery bounded by `timeout`.
    ///
    /// # Errors
    /// Returns the `reqwest` error when the HTTP client cannot be built.
    pub fn new(
        base: Url,
        timeout: Duration,
        credentials: Credentials,
    ) -> Result<Self, reqwest::Error> {
        // NOTE: no specification governs this: our own design; the Bundle names patients, so
        // a redirect never sends it on to a location the configuration does not name.
        let http = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .build()?;
        Ok(Self {
            base,
            http,
            timeout,
            credentials,
        })
    }

    /// Returns the FHIR service base.
    #[must_use]
    pub const fn base(&self) -> &Url {
        &self.base
    }

    /// Posts `bundle`, a `transaction` Bundle as FHIR JSON.
    ///
    /// # Errors
    /// Returns [`SinkError`] for every answer but a `2xx`, and for no answer.
    pub async fn deliver(&self, bundle: &[u8]) -> Result<(), SinkError> {
        let status = self.post(bundle).await?;
        if status == StatusCode::UNAUTHORIZED
            && let Credentials::Bearer(source) = &self.credentials
        {
            // NOTE: RFC 6750 §3.1: a 401 for an invalid token is answered with a new token, once.
            let stale = source
                .token()
                .await
                .map_err(|error| SinkError::Token(Box::new(error)))?;
            source.invalidate(stale.header());
            return outcome(self.post(bundle).await?);
        }
        outcome(status)
    }

    /// Posts `bundle` once and returns the status the endpoint answered.
    async fn post(&self, bundle: &[u8]) -> Result<StatusCode, SinkError> {
        let mut request = self
            .http
            .post(self.base.clone())
            .header(CONTENT_TYPE, FHIR_JSON)
            .header(http::header::ACCEPT, FHIR_JSON)
            .header("Prefer", "return=minimal")
            .body(bundle.to_vec())
            .timeout(self.timeout);
        match &self.credentials {
            Credentials::None => {}
            Credentials::Basic { user, password } => {
                request = request.basic_auth(user, Some(password.expose_secret()));
            }
            Credentials::Bearer(source) => {
                let token = source
                    .token()
                    .await
                    .map_err(|error| SinkError::Token(Box::new(error)))?;
                let mut value = token.header().clone();
                value.set_sensitive(true);
                request = request.header(AUTHORIZATION, value);
            }
        }
        let answer = request
            .send()
            .await
            .map_err(|error| SinkError::Transport(error.without_url()))?;
        Ok(answer.status())
    }
}

/// Returns what the endpoint's answer `status` means for the delivery.
fn outcome(status: StatusCode) -> Result<(), SinkError> {
    if status.is_success() {
        return Ok(());
    }
    if status.is_client_error()
        && ![
            StatusCode::UNAUTHORIZED,
            StatusCode::REQUEST_TIMEOUT,
            StatusCode::TOO_MANY_REQUESTS,
        ]
        .contains(&status)
    {
        return Err(SinkError::Refused { status });
    }
    Err(SinkError::Unavailable { status })
}

#[cfg(test)]
mod tests {
    use http::StatusCode;

    use super::outcome;

    #[test]
    fn a_refusal_of_the_content_is_not_transient() {
        for status in [StatusCode::BAD_REQUEST, StatusCode::UNPROCESSABLE_ENTITY] {
            assert!(
                outcome(status).is_err_and(|error| !error.is_transient()),
                "{status}"
            );
        }
        for status in [
            StatusCode::UNAUTHORIZED,
            StatusCode::TOO_MANY_REQUESTS,
            StatusCode::SERVICE_UNAVAILABLE,
        ] {
            assert!(
                outcome(status).is_err_and(|error| error.is_transient()),
                "{status}"
            );
        }
        assert!(outcome(StatusCode::OK).is_ok());
    }
}
