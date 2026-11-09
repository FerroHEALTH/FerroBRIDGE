// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! How a client is pointed at one terminology server: its versioned base URL,
//! its wire version, its budget, and its credentials.
//!
//! No specification governs the timeout, the retry budget or the credential
//! form: our own design. FHIR leaves authentication to the deployment
//! (<https://hl7.org/fhir/R4/security.html>), so the credentials are optional.

#[cfg(feature = "client")]
use secrecy::SecretString;
use std::fmt;
use std::sync::Arc;
#[cfg(feature = "client")]
use std::time::Duration;
#[cfg(feature = "client")]
use url::Url;

/// The FHIR release a server answers in.
///
/// The release decides which generated module writes the request and reads
/// the `Parameters` and the `OperationOutcome` of a call. R4 to R6 declare
/// the same `in` and `out` parameters outside `$translate`, which each
/// release spells its own way. STU3 declares fewer, so a request naming a
/// parameter STU3 has no equivalent for is refused with
/// [`Error::UnsupportedParameter`](crate::error::Error::UnsupportedParameter)
/// before anything is sent. A release is a variant only when its feature of
/// this crate is on, so a release left out of the build cannot be named.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum WireVersion {
    /// FHIR STU3, release 3.0.2 (<https://hl7.org/fhir/STU3/>).
    #[cfg(feature = "stu3")]
    Stu3,
    /// FHIR R4 (<https://hl7.org/fhir/R4/>).
    #[cfg(feature = "r4")]
    R4,
    /// FHIR R4B (<https://hl7.org/fhir/R4B/>).
    #[cfg(feature = "r4b")]
    R4B,
    /// FHIR R5 (<https://hl7.org/fhir/R5/>).
    #[cfg(feature = "r5")]
    R5,
    /// The FHIR R6 ballot, 6.0.0-ballot5
    /// (<https://hl7.org/fhir/6.0.0-ballot5/>).
    #[cfg(feature = "r6")]
    R6,
}

impl WireVersion {
    /// Returns the release name as the FHIR specification spells it.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            #[cfg(feature = "stu3")]
            Self::Stu3 => "STU3",
            #[cfg(feature = "r4")]
            Self::R4 => "R4",
            #[cfg(feature = "r4b")]
            Self::R4B => "R4B",
            #[cfg(feature = "r5")]
            Self::R5 => "R5",
            #[cfg(feature = "r6")]
            Self::R6 => "R6",
        }
    }
}

/// The credentials the client presents on every request.
///
/// Every secret is a [`SecretString`], so neither a `Debug` rendering nor a
/// log line can carry it.
#[cfg(feature = "client")]
#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum Credentials {
    /// RFC 7617 basic authentication.
    Basic {
        /// The user name, which is not a secret.
        user: String,
        /// The password.
        password: SecretString,
    },
    /// An RFC 6750 bearer token the provider obtains with the OAuth 2.0
    /// client credentials grant (RFC 6749 §4.4), renewed once when the server
    /// answers `401`.
    OAuth(Arc<dyn TokenProvider>),
    /// An `Authorization` header value sent as it stands, such as the RFC
    /// 7617 basic credential `ferrohealth-smart`'s outbound client resolves;
    /// it is marked sensitive, so no log line carries it.
    Header(http::HeaderValue),
}

/// Why a [`TokenProvider`] obtained no token.
///
/// The provider's own error is its [`source`](std::error::Error::source), so
/// a caller walking the chain reaches that error's type.
#[derive(Debug, Clone)]
pub struct TokenFailure(Arc<dyn std::error::Error + Send + Sync>);

impl TokenFailure {
    /// Returns the failure the provider's error `cause` describes.
    #[must_use]
    pub fn new(cause: Arc<dyn std::error::Error + Send + Sync>) -> Self {
        Self(cause)
    }
}

impl fmt::Display for TokenFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("the token provider obtained no token")
    }
}

impl std::error::Error for TokenFailure {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&*self.0)
    }
}

/// The source of the tokens a [`Credentials::OAuth`] client sends.
///
/// The client asks for a token before every attempt. When the server answers
/// `401`, it hands the refused token back through
/// [`TokenProvider::invalidate`], asks once more, and re-sends the request
/// once, since a `401` means the server applied nothing (RFC 9110 §15.5.2).
#[cfg(feature = "client")]
#[async_trait::async_trait]
pub trait TokenProvider: fmt::Debug + Send + Sync {
    /// Returns the token to send.
    ///
    /// # Errors
    /// Returns the provider's own failure when no token can be obtained.
    async fn token(&self) -> Result<SecretString, TokenFailure>;

    /// Drops `stale`, a token the server refused, so the next
    /// [`TokenProvider::token`] obtains a new one. A token the provider has
    /// already replaced stays.
    fn invalidate(&self, stale: &SecretString);
}

/// How often, and how far apart, a call is retried.
///
/// The delay grows exponentially from `initial_backoff` and is capped at
/// `max_backoff`; `max_attempts` counts the first try, so `1` disables retry.
#[cfg(feature = "client")]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RetryPolicy {
    /// How many times the request is sent at most, the first try included.
    pub max_attempts: u32,
    /// The delay before the second attempt.
    pub initial_backoff: Duration,
    /// The ceiling every later delay is clamped to.
    pub max_backoff: Duration,
}

#[cfg(feature = "client")]
impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            max_attempts: 3,
            initial_backoff: Duration::from_millis(200),
            max_backoff: Duration::from_secs(5),
        }
    }
}

/// The form an operation is invoked in
/// (<https://hl7.org/fhir/R4/operations.html#request>).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[non_exhaustive]
pub enum Form {
    /// A `POST` whose body is a `Parameters` resource carrying every input.
    #[default]
    Post,
    /// A `GET` whose query carries every input, which the framework allows
    /// only when no input is a complex type or a resource; a request with
    /// such an input (an inline `ValueSet`, a `Coding`, a
    /// `CodeableConcept`) is sent as a `POST`.
    Get,
}

/// Everything one [`Client`](crate::client::Client) needs about one
/// terminology server.
///
/// `base_url` is the FHIR service base URL, the path `CodeSystem`,
/// `ConceptMap` and `ValueSet` live under
/// (<https://hl7.org/fhir/R4/http.html#root>). A server that serves several
/// releases side by side puts the release in that path, so the base URL and
/// `wire_version` are set together and must agree.
#[cfg(feature = "client")]
#[derive(Debug, Clone)]
pub struct Config {
    /// The FHIR service base URL.
    pub base_url: Url,
    /// The release the server answers in.
    pub wire_version: WireVersion,
    /// How long one request may take, connection included.
    pub timeout: Duration,
    /// The retry budget.
    pub retry: RetryPolicy,
    /// The credentials, when the deployment needs them.
    pub credentials: Option<Credentials>,
    /// The form each operation is invoked in.
    pub form: Form,
}

#[cfg(feature = "client")]
impl Config {
    /// Returns a configuration for the server whose base URL is `base_url`.
    ///
    /// The timeout and the retry budget take their defaults, and no
    /// credentials are set.
    #[must_use]
    pub fn new(base_url: Url, wire_version: WireVersion) -> Self {
        Self {
            base_url,
            wire_version,
            timeout: Duration::from_secs(30),
            retry: RetryPolicy::default(),
            credentials: None,
            form: Form::Post,
        }
    }

    /// Returns this configuration with `timeout` as its per-request budget.
    #[must_use]
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// Returns this configuration with `retry` as its retry budget.
    #[must_use]
    pub fn with_retry(mut self, retry: RetryPolicy) -> Self {
        self.retry = retry;
        self
    }

    /// Returns this configuration invoking each operation in `form`.
    #[must_use]
    pub const fn with_form(mut self, form: Form) -> Self {
        self.form = form;
        self
    }

    /// Returns this configuration with `credentials` presented on every call.
    #[must_use]
    pub fn with_credentials(mut self, credentials: Credentials) -> Self {
        self.credentials = Some(credentials);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::WireVersion;
    #[cfg(all(feature = "r4", feature = "client"))]
    use super::{Config, Credentials, RetryPolicy};
    #[cfg(all(feature = "r4", feature = "client"))]
    use secrecy::SecretString;
    #[cfg(all(feature = "r4", feature = "client"))]
    use std::time::Duration;

    #[cfg(all(feature = "r4", feature = "client"))]
    fn config() -> Config {
        Config::new(
            "http://tx.invalid/r4".parse().expect("a valid URL"),
            WireVersion::R4,
        )
    }

    #[cfg(all(feature = "r4", feature = "client"))]
    #[test]
    fn a_basic_password_is_not_in_the_debug_rendering() {
        let config = config().with_credentials(Credentials::Basic {
            user: "bridge".to_owned(),
            password: SecretString::from("s3cr3t-password"),
        });
        let rendered = format!("{config:?}");
        assert!(!rendered.contains("s3cr3t-password"));
        assert!(rendered.contains("bridge"));
    }

    #[cfg(all(feature = "r4", feature = "client"))]
    #[test]
    fn the_defaults_are_three_attempts_and_thirty_seconds() {
        let config = config();
        assert_eq!(Duration::from_secs(30), config.timeout);
        assert_eq!(RetryPolicy::default(), config.retry);
        assert_eq!(3, config.retry.max_attempts);
    }

    #[test]
    fn a_wire_version_names_its_release() {
        #[cfg(feature = "stu3")]
        assert_eq!("STU3", WireVersion::Stu3.as_str());
        #[cfg(feature = "r4")]
        assert_eq!("R4", WireVersion::R4.as_str());
        #[cfg(feature = "r4b")]
        assert_eq!("R4B", WireVersion::R4B.as_str());
        #[cfg(feature = "r5")]
        assert_eq!("R5", WireVersion::R5.as_str());
        #[cfg(feature = "r6")]
        assert_eq!("R6", WireVersion::R6.as_str());
    }
}
