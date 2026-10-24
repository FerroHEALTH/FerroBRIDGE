// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The client itself: one configured terminology server, one request at a
//! time.

use crate::config::{Config, Credentials, TokenFailure};
use crate::error::{Error, UpstreamError};
use crate::outcome::{
    BatchOutcome, Coded, ExpandOutcome, Expansion, LookupOutcome, Request, SubsumesOutcome,
    Subsumption, TranslateOutcome, Translation, ValidateOutcome, Validation,
};
use backon::{ExponentialBuilder, Retryable};
use http::header::{ACCEPT, CONTENT_TYPE};
use http::{Method, StatusCode};
use secrecy::ExposeSecret;
use std::sync::Arc;
use url::Url;

use crate::client::transport::challenge;
use crate::client::transport::dispatch;

mod transport;

/// The FHIR JSON media type (<https://hl7.org/fhir/R4/http.html#mime-type>).
pub const FHIR_JSON: &str = "application/fhir+json";

/// The path of the capability statement a FHIR server publishes
/// (<https://hl7.org/fhir/R4/http.html#capabilities>).
const METADATA: &str = "metadata";

/// One request, described so that every attempt builds the same thing.
struct Call {
    /// The operation, as `Resource/$code`, or the batch marker.
    operation: &'static str,
    /// The absolute URL.
    url: Url,
    /// The FHIR JSON request body of a `POST`; a `GET` carries none.
    body: Option<String>,
}

/// What the server answered, read whole.
pub(crate) struct Answer {
    /// The URL that was called.
    pub(crate) url: Url,
    /// The status the server answered with.
    pub(crate) status: StatusCode,
    /// The response body, as received.
    pub(crate) body: String,
}

/// A client for one FHIR terminology server.
///
/// The client is cheap to clone: the HTTP stack and the configuration are
/// shared, so a clone carries the same connection pool.
#[derive(Debug, Clone)]
pub struct Client {
    /// The HTTP stack, with the configured timeout.
    http: reqwest::Client,
    /// The server this client talks to.
    config: Arc<Config>,
}

impl Client {
    /// Returns a client for the server `config` describes.
    ///
    /// This constructor and [`Client::with_http`] are the only ways to
    /// obtain a client, so a deployment that configures no terminology
    /// server holds no [`Client`] at all and the absence is a compile-time
    /// `Option<Client>` rather than a runtime fallback.
    ///
    /// # Errors
    /// Returns [`Error::BaseUrl`] when the base URL cannot carry a path, and
    /// [`Error::ClientBuild`] when the HTTP stack refuses the configuration.
    pub fn new(config: Config) -> Result<Self, Error> {
        if config.base_url.cannot_be_a_base() {
            return Err(Error::BaseUrl {
                base: config.base_url.clone(),
            });
        }
        let http = reqwest::Client::builder()
            .timeout(config.timeout)
            .build()
            .map_err(|source| Error::ClientBuild { source })?;
        Ok(Self {
            http,
            config: Arc::new(config),
        })
    }

    /// Returns a client for the server `config` describes that sends every
    /// request through `http`, an HTTP client the caller built.
    ///
    /// The caller's client carries what the deployment's transport needs and
    /// [`Config`] does not hold: a client certificate for mutual TLS, trust
    /// roots beside the platform's, a proxy. FHIR leaves transport security
    /// to the deployment (<https://hl7.org/fhir/R4/security.html>). The
    /// configured timeout still bounds every request, and the configured
    /// credentials, retry budget and errors apply as they do for
    /// [`Client::new`]. No specification governs this constructor: our own
    /// design.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use ferrobridge_term::client::Client;
    /// use ferrobridge_term::config::{Config, WireVersion};
    ///
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// # let pem = b"";
    /// let http = reqwest::Client::builder()
    ///     .identity(reqwest::Identity::from_pem(pem)?)
    ///     .build()?;
    /// let config = Config::new("https://tx.example.org/r4".parse()?, WireVersion::R4);
    /// let client = Client::with_http(config, http)?;
    /// # let _ = client;
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// # Errors
    /// Returns [`Error::BaseUrl`] when the base URL cannot carry a path.
    pub fn with_http(config: Config, http: reqwest::Client) -> Result<Self, Error> {
        if config.base_url.cannot_be_a_base() {
            return Err(Error::BaseUrl {
                base: config.base_url.clone(),
            });
        }
        Ok(Self {
            http,
            config: Arc::new(config),
        })
    }

    /// Returns the configuration this client was built from.
    #[must_use]
    pub fn config(&self) -> &Config {
        &self.config
    }

    /// Checks that the configured release can carry `request`, sending
    /// nothing.
    ///
    /// A caller that sends several requests uses this to set aside one the
    /// release cannot carry, since a batch holding it is refused whole.
    ///
    /// # Errors
    /// Returns [`Error::UnsupportedParameter`] when `request` names a
    /// parameter the configured release does not declare: a concept map
    /// `url` or a value set version on an STU3 client. Returns
    /// [`Error::SystemRequired`] when it validates a code with no system on
    /// an STU3 client.
    pub fn check(&self, request: &Request) -> Result<(), Error> {
        let version = self.config.wire_version;
        if let Some(parameter) = crate::wire::unsupported(version, request) {
            return Err(Error::UnsupportedParameter {
                operation: request.operation(),
                parameter,
                wire_version: version,
            });
        }
        if crate::wire::requires_system(version, request) {
            return Err(Error::SystemRequired {
                operation: request.operation(),
                wire_version: version,
            });
        }
        Ok(())
    }

    /// Returns the status the server answers `GET [base]/metadata` with.
    ///
    /// Every FHIR REST server states its capabilities at that path
    /// (<https://hl7.org/fhir/R4/http.html#capabilities>), so a reachability
    /// question needs no terminology operation. The call is not retried, and
    /// every status is returned as it stands, `401` and `404` included; under
    /// [`Credentials::OAuth`] a refused token is renewed once first.
    ///
    /// # Errors
    /// Returns [`Error::BaseUrl`] when the base URL cannot carry a path,
    /// [`Error::Token`] when no token could be obtained, [`Error::Timeout`]
    /// when the server did not answer inside the configured budget, and
    /// [`Error::Transport`] when the request never reached it.
    pub async fn reachability(&self) -> Result<StatusCode, Error> {
        let url = self.url(METADATA)?;
        let (response, renewal) = self
            .send(&url, || {
                self.http
                    .request(Method::GET, url.clone())
                    .timeout(self.config.timeout)
                    .header(ACCEPT, FHIR_JSON)
            })
            .await?;
        if let Some(source) = renewal {
            return Err(Error::Token { url, source });
        }
        let status = response.status();
        tracing::debug!(path = url.path(), status = %status, "the terminology server answered a probe");
        Ok(status)
    }

    /// Returns the response to the request `build` makes under the configured
    /// credentials, with the provider's failure when the server refused an
    /// OAuth token and no new one could be obtained.
    ///
    /// A refused OAuth token is dropped and the request sent once more under a
    /// new one; a second `401` is returned as it stands.
    async fn send(
        &self,
        url: &Url,
        build: impl Fn() -> reqwest::RequestBuilder,
    ) -> Result<(reqwest::Response, Option<TokenFailure>), Error> {
        let provider = match self.config.credentials.as_ref() {
            None => return Ok((dispatch(url, build()).await?, None)),
            Some(Credentials::Basic { user, password }) => {
                let request = build().basic_auth(user, Some(password.expose_secret()));
                return Ok((dispatch(url, request).await?, None));
            }
            Some(Credentials::OAuth(provider)) => provider,
        };
        let token = provider.token().await.map_err(|source| Error::Token {
            url: url.clone(),
            source,
        })?;
        let response = dispatch(url, build().bearer_auth(token.expose_secret())).await?;
        // NOTE: RFC 9110 §15.5.2, a `401` means the request was not applied, so
        // sending it once more under a renewed token cannot apply it twice.
        if response.status() != StatusCode::UNAUTHORIZED {
            return Ok((response, None));
        }
        provider.invalidate(&token);
        match provider.token().await {
            Ok(renewed) => {
                tracing::debug!(
                    path = url.path(),
                    "the terminology server refused the token; re-sending once under a renewed one"
                );
                let request = build().bearer_auth(renewed.expose_secret());
                Ok((dispatch(url, request).await?, None))
            }
            Err(source) => Ok((response, Some(source))),
        }
    }

    /// Resolves the display of `code` in `system`.
    ///
    /// A server that does not know the code answers [`LookupOutcome::NotFound`]
    /// carrying its `OperationOutcome`; the display is never an empty string.
    ///
    /// # Errors
    /// Returns [`Error::Refused`] when the server refused the operation for
    /// another reason, and [`Error`] otherwise when the call did not reach an
    /// answer or the answer could not be decoded.
    pub async fn lookup(
        &self,
        system: &str,
        code: &str,
        version: Option<&str>,
    ) -> Result<LookupOutcome, Error> {
        self.send_lookup(Request::Lookup {
            system: system.to_owned(),
            code: code.to_owned(),
            version: version.map(str::to_owned),
            date: None,
        })
        .await
    }

    /// Resolves the display of `code` in `system` as the code system stood
    /// on `date`, a FHIR `dateTime` sent as the `date` input
    /// (<https://hl7.org/fhir/R4/codesystem-operation-lookup.html>).
    ///
    /// # Errors
    /// As [`Client::lookup`].
    pub async fn lookup_at(
        &self,
        system: &str,
        code: &str,
        version: Option<&str>,
        date: &str,
    ) -> Result<LookupOutcome, Error> {
        self.send_lookup(Request::Lookup {
            system: system.to_owned(),
            code: code.to_owned(),
            version: version.map(str::to_owned),
            date: Some(date.to_owned()),
        })
        .await
    }

    /// Returns the outcome of the `$lookup` `request`.
    async fn send_lookup(&self, request: Request) -> Result<LookupOutcome, Error> {
        let answer = self.operation(&request).await?;
        crate::decode::lookup(self.config.wire_version, &answer)
    }

    /// Expands the value set `expansion` names, with the page, filter, date
    /// and designations it asks for
    /// (<https://hl7.org/fhir/R4/valueset-operation-expand.html>).
    ///
    /// The answer keeps the nesting of `expansion.contains`. A value set the
    /// server does not know is [`ExpandOutcome::NotFound`] carrying its
    /// `OperationOutcome`, never an empty expansion.
    ///
    /// # Errors
    /// Returns [`Error::Refused`] when the server refused the expansion for
    /// another reason, `too-costly` among them, and [`Error`] otherwise when
    /// the call did not reach an answer or the answer could not be decoded.
    /// Returns [`Error::UnsupportedParameter`] when the expansion names a
    /// value set version on an STU3 client, and [`Error::ParameterRange`]
    /// when `offset` or `count` exceeds a FHIR `integer`; nothing is sent.
    pub async fn expand(&self, expansion: &Expansion) -> Result<ExpandOutcome, Error> {
        let request = Request::Expand(expansion.clone());
        let answer = self.operation(&request).await?;
        crate::decode::expand(self.config.wire_version, &answer)
    }

    /// Tests how the two codes `subsumption` names relate in their code
    /// system (<https://hl7.org/fhir/R4/codesystem-operation-subsumes.html>).
    ///
    /// # Errors
    /// Returns [`Error::Refused`] when the server could not determine the
    /// relationship, for example because it does not know a code, carrying
    /// the `OperationOutcome` and its `tx-issue-type` coding, and [`Error`]
    /// otherwise when the call did not reach an answer or the answer could
    /// not be decoded. An `outcome` outside the four codes is
    /// [`Error::Body`].
    pub async fn subsumes(&self, subsumption: &Subsumption) -> Result<SubsumesOutcome, Error> {
        let request = Request::Subsumes(subsumption.clone());
        let answer = self.operation(&request).await?;
        crate::decode::subsumes(self.config.wire_version, &answer)
    }

    /// Returns how many `CodeSystem` resources the server holds under the
    /// canonical URL `system`, asked as
    /// `GET [base]/CodeSystem?url=<system>&_summary=count`
    /// (<https://hl7.org/fhir/R4/search.html#summary>).
    ///
    /// A count of `0` says the server does not serve the code system, which
    /// tells an unknown code from an unserved system after a bare `404` from
    /// [`Client::lookup`].
    ///
    /// # Errors
    /// Returns [`Error::Refused`] when the server refused the search, and
    /// [`Error::Body`] when the answer is not a `searchset` `Bundle` stating
    /// its `total`; [`Error`] otherwise when the call did not reach an
    /// answer.
    pub async fn code_system_count(&self, system: &str) -> Result<u32, Error> {
        let mut url = self.url("CodeSystem")?;
        url.query_pairs_mut()
            .append_pair("url", system)
            .append_pair("_summary", "count");
        let answer = self
            .execute(Call {
                operation: crate::CODE_SYSTEM_SEARCH,
                url,
                body: None,
            })
            .await?;
        crate::decode::search_total(self.config.wire_version, &answer)
    }

    /// Translates the code `translation` names, through the concept map,
    /// into the target system and in the direction it names.
    ///
    /// `url`, `targetsystem` and `reverse` are each sent only when the
    /// translation sets them, as R4 declares each `0..1`
    /// (<https://hl7.org/fhir/R4/conceptmap-operation-translate.html>).
    /// Every match the server returned is carried with its equivalence, so the
    /// caller decides which ones count;
    /// [`TranslateOutcome::accepted`] yields the `equivalent` and `equal` ones.
    ///
    /// # Examples
    ///
    /// A deployment with no terminology server holds no client, and a
    /// translation it cannot make is a refusal:
    ///
    /// ```rust,no_run
    /// use ferrobridge_term::client::Client;
    /// use ferrobridge_term::outcome::Translation;
    ///
    /// # fn main() {
    /// # let terminology: Option<Client> = None;
    /// # let _call = async move {
    /// let Some(client) = terminology.as_ref() else {
    ///     return Err("no terminology server is configured".into());
    /// };
    /// let translation = Translation::new("http://example.org/source", "alpha")
    ///     .with_concept_map("http://example.org/ConceptMap/source-to-target");
    /// let outcome = client.translate(&translation).await?;
    /// let Some(translated) = outcome.accepted().next() else {
    ///     return Err("no equivalent translation".into());
    /// };
    /// let code = translated.concept.as_ref().and_then(|concept| concept.code.as_deref());
    /// assert!(code.is_some(), "an accepted match names its target code");
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// # };
    /// # }
    /// ```
    ///
    /// # Errors
    /// Returns [`Error::Refused`] when the server refused the operation, for
    /// example because it serves no such concept map, and [`Error`] otherwise
    /// when the call did not reach an answer or the answer could not be
    /// decoded. A refusal is never a missing translation. Returns
    /// [`Error::UnsupportedParameter`] when the translation names a concept
    /// map on an STU3 client, whose `$translate` has no `url`; nothing is
    /// sent.
    pub async fn translate(&self, translation: &Translation) -> Result<TranslateOutcome, Error> {
        let request = Request::Translate(translation.clone());
        let answer = self.operation(&request).await?;
        crate::decode::translate(self.config.wire_version, &answer)
    }

    /// Validates that `code` from `system` is a member of `value_set_url`.
    ///
    /// # Errors
    /// Returns [`Error::Refused`] when the server refused the operation, for
    /// example because it serves no such value set, and [`Error`] otherwise
    /// when the call did not reach an answer or the answer could not be
    /// decoded. A refusal is never an invalid code.
    pub async fn validate_code(
        &self,
        value_set_url: &str,
        system: &str,
        code: &str,
    ) -> Result<ValidateOutcome, Error> {
        self.validate(&Validation {
            value_set_url: value_set_url.to_owned(),
            value_set_version: None,
            date: None,
            coded: Coded::Code {
                system: system.to_owned(),
                code: code.to_owned(),
                version: None,
                display: None,
            },
        })
        .await
    }

    /// Validates that the coded value of `validation` is a member of its
    /// value set, in any of the three forms the operation accepts: a code
    /// with its system, version and display, a `Coding`, or a
    /// `CodeableConcept`
    /// (<https://hl7.org/fhir/R4/valueset-operation-validate-code.html>).
    ///
    /// # Errors
    /// Returns [`Error::Refused`] when the server refused the operation, and
    /// [`Error`] otherwise when the call did not reach an answer or the answer
    /// could not be decoded. A refusal is never an invalid code. Returns
    /// [`Error::UnsupportedParameter`] when the validation names a value set
    /// version on an STU3 client, whose `$validate-code` has no
    /// `valueSetVersion`; nothing is sent.
    pub async fn validate(&self, validation: &Validation) -> Result<ValidateOutcome, Error> {
        let request = Request::ValidateCode(validation.clone());
        let answer = self.operation(&request).await?;
        crate::decode::validate_code(self.config.wire_version, &answer)
    }

    /// Validates every one of `validations`, answering them by position: one
    /// call for one validation, one `batch` `Bundle` for several.
    ///
    /// An entry the server refused is that entry's own typed error and leaves
    /// the others alone, as in [`Client::batch`].
    ///
    /// # Errors
    /// Returns [`Error::BatchArity`] when a batch answer holds a different
    /// number of entries than were sent, and [`Error`] otherwise when the
    /// batch itself did not reach an answer or its `Bundle` could not be
    /// decoded. Returns [`Error::UnsupportedParameter`] when one of several
    /// validations names a parameter the configured release does not
    /// declare, and then no entry is sent.
    pub async fn validate_all(
        &self,
        validations: &[Validation],
    ) -> Result<Vec<Result<ValidateOutcome, Error>>, Error> {
        match validations {
            [] => Ok(Vec::new()),
            [one] => Ok(vec![self.validate(one).await]),
            several => {
                let requests: Vec<Request> =
                    several.iter().cloned().map(Request::ValidateCode).collect();
                let version = self.config.wire_version;
                Ok(self
                    .batch_answers(&requests)
                    .await?
                    .into_iter()
                    .map(|answer| crate::decode::validate_code(version, &answer))
                    .collect())
            }
        }
    }

    /// Sends `requests` as one `batch` `Bundle` and answers them by position.
    ///
    /// "The server processes each entry independently" and answers a
    /// `batch-response` whose entries are "in the same order as the request"
    /// (<https://hl7.org/fhir/R4/http.html#transaction>), so entry `n` is the
    /// answer to request `n`. An entry the server refused is that entry's own
    /// typed error and leaves the others alone.
    ///
    /// # Errors
    /// Returns [`Error::BatchArity`] when the answer holds a different number
    /// of entries than were sent, and [`Error`] otherwise when the batch
    /// itself did not reach an answer or its `Bundle` could not be decoded.
    /// Returns [`Error::UnsupportedParameter`] when a request names a
    /// parameter the configured release does not declare, and then no entry
    /// is sent.
    pub async fn batch(
        &self,
        requests: &[Request],
    ) -> Result<Vec<Result<BatchOutcome, Error>>, Error> {
        let version = self.config.wire_version;
        Ok(requests
            .iter()
            .zip(self.batch_answers(requests).await?)
            .map(|(request, answer)| crate::decode::batch_entry(version, request, &answer))
            .collect())
    }

    /// Sends `requests` as one `batch` `Bundle` and returns each entry's
    /// answer, in request order.
    async fn batch_answers(&self, requests: &[Request]) -> Result<Vec<Answer>, Error> {
        let version = self.config.wire_version;
        let body = crate::wire::batch_body(version, requests)
            .map_err(|source| source.into_error(crate::BATCH, version))?;
        let answer = self
            .execute(Call {
                operation: crate::BATCH,
                url: self.config.base_url.clone(),
                body: Some(body),
            })
            .await?;
        if answer.status != StatusCode::OK {
            return Err(self.refusal(crate::BATCH, &answer));
        }
        let entries =
            crate::wire::batch_entries(version, &answer.body).map_err(|source| Error::Body {
                operation: crate::BATCH,
                url: answer.url.clone(),
                source: Box::new(source),
            })?;
        if entries.len() != requests.len() {
            return Err(Error::BatchArity {
                sent: requests.len(),
                answered: entries.len(),
            });
        }
        Ok(entries
            .into_iter()
            .map(|entry| Answer {
                url: answer.url.clone(),
                status: entry.status,
                body: entry.body,
            })
            .collect())
    }

    /// Returns the answer to the `POST` form of `request`.
    ///
    /// The operations are invoked with a `Parameters` body, the form that
    /// carries every input whatever its type
    /// (<https://hl7.org/fhir/R4/operations.html>).
    async fn operation(&self, request: &Request) -> Result<Answer, Error> {
        let operation = request.operation();
        let version = self.config.wire_version;
        let body = crate::wire::request_body(version, request)
            .map_err(|source| source.into_error(operation, version))?;
        let url = self.url(operation)?;
        self.execute(Call {
            operation,
            url,
            body: Some(body),
        })
        .await
    }

    /// Returns the absolute URL of `operation` under the configured base.
    fn url(&self, operation: &str) -> Result<Url, Error> {
        let mut url = self.config.base_url.clone();
        {
            let mut path = url.path_segments_mut().map_err(|()| Error::BaseUrl {
                base: self.config.base_url.clone(),
            })?;
            path.pop_if_empty();
            path.extend(operation.split('/'));
        }
        Ok(url)
    }

    /// Returns the answer to `call`, retrying it when the budget allows.
    async fn execute(&self, call: Call) -> Result<Answer, Error> {
        let client = self;
        let request = &call;
        let attempt = move || async move { client.attempt(request).await };
        attempt
            .retry(self.backoff())
            .when(Error::is_retryable)
            .notify(|error: &Error, delay| {
                tracing::warn!(
                    error = error.kind(),
                    delay_ms = delay.as_millis(),
                    "retrying a terminology request"
                );
            })
            .await
    }

    /// Returns the retry schedule the configuration asks for.
    fn backoff(&self) -> ExponentialBuilder {
        let retries =
            usize::try_from(self.config.retry.max_attempts.saturating_sub(1)).unwrap_or(usize::MAX);
        ExponentialBuilder::new()
            .with_min_delay(self.config.retry.initial_backoff)
            .with_max_delay(self.config.retry.max_backoff)
            .with_max_times(retries)
    }

    /// Sends `call` once and reads the whole answer.
    async fn attempt(&self, call: &Call) -> Result<Answer, Error> {
        let (response, renewal) = self
            .send(&call.url, || match &call.body {
                Some(body) => self
                    .http
                    .request(Method::POST, call.url.clone())
                    .timeout(self.config.timeout)
                    .header(ACCEPT, FHIR_JSON)
                    .header(CONTENT_TYPE, FHIR_JSON)
                    .body(body.clone()),
                None => self
                    .http
                    .request(Method::GET, call.url.clone())
                    .timeout(self.config.timeout)
                    .header(ACCEPT, FHIR_JSON),
            })
            .await?;

        let status = response.status();
        let headers = response.headers().clone();
        let bytes = response.bytes().await.map_err(|source| Error::Transport {
            url: call.url.clone(),
            source,
        })?;
        let answer = Answer {
            url: call.url.clone(),
            status,
            body: String::from_utf8_lossy(&bytes).into_owned(),
        };
        // A code is a clinical fact, so the log carries the path and never the
        // body or the query.
        tracing::debug!(
            operation = call.operation,
            path = answer.url.path(),
            status = %status,
            "the terminology server answered"
        );

        if status == StatusCode::UNAUTHORIZED
            && let Some(source) = renewal
        {
            return Err(Error::Renewal {
                url: answer.url.clone(),
                upstream: Box::new(self.upstream(&answer)),
                source,
            });
        }
        if status == StatusCode::UNAUTHORIZED {
            return Err(Error::Unauthorized {
                url: answer.url.clone(),
                challenge: challenge(&headers),
                upstream: Box::new(self.upstream(&answer)),
            });
        }
        if status.is_server_error() {
            return Err(Error::ServerFailure {
                url: answer.url.clone(),
                upstream: Box::new(self.upstream(&answer)),
            });
        }
        Ok(answer)
    }

    /// Returns `answer` as an upstream refusal, read in the wire version.
    fn upstream(&self, answer: &Answer) -> UpstreamError {
        UpstreamError::new(self.config.wire_version, answer.status, answer.body.clone())
    }

    /// Returns the error for an answer the operation cannot read.
    fn refusal(&self, operation: &'static str, answer: &Answer) -> Error {
        Error::Refused {
            operation,
            url: answer.url.clone(),
            upstream: Box::new(self.upstream(answer)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Client;
    use crate::config::{Config, WireVersion};

    fn client(base: &str) -> Client {
        Client::new(Config::new(
            base.parse().expect("a valid URL"),
            WireVersion::R4,
        ))
        .expect("a buildable client")
    }

    #[test]
    fn an_operation_url_sits_under_the_versioned_base() {
        let url = client("http://tx.invalid/fhir/r4")
            .url("CodeSystem/$lookup")
            .expect("a URL under the base");
        assert_eq!("http://tx.invalid/fhir/r4/CodeSystem/$lookup", url.as_str());
    }

    #[test]
    fn a_trailing_slash_on_the_base_does_not_double() {
        let url = client("http://tx.invalid/r4b/")
            .url("ValueSet/$validate-code")
            .expect("a URL under the base");
        assert_eq!(
            "http://tx.invalid/r4b/ValueSet/$validate-code",
            url.as_str()
        );
    }
}
