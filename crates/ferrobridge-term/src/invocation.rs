// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The request a release writes and the answer it reads, with no transport.
//!
//! [`Client`](crate::client::Client) sends what [`Invocation::new`] writes
//! and reads what [`answer`] reads. A caller that sends requests through a
//! transport of its own, such as a server forwarding a signed-in user's
//! token, writes and reads through the same two functions, so its requests
//! and readings are the client's. No specification governs this split: our
//! own design.

use crate::config::{Form, WireVersion};
use crate::error::Error;
use crate::outcome::BatchOutcome;
use crate::request::Request;
use http::{Method, StatusCode};
use url::Url;

/// One operation, written in one release and one form.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Invocation {
    method: Method,
    segments: Vec<String>,
    query: Vec<(String, String)>,
    body: Option<String>,
}

impl Invocation {
    /// Returns `request` written in `version` and invoked in `form`.
    ///
    /// The query and the body carry each input under the name the release's
    /// own `OperationDefinition` gives it, in the order it declares them. In
    /// the [`Form::Get`] form a request with a complex input is written as a
    /// `POST`, since the `GET` form is open only when "there are no
    /// parameters with complex types"
    /// (<https://hl7.org/fhir/R4/operations.html#request>).
    ///
    /// # Errors
    ///
    /// Returns [`Error::UnsupportedOperation`],
    /// [`Error::UnsupportedParameter`] or [`Error::SystemRequired`] when the
    /// release cannot carry the request, and [`Error::ParameterRange`],
    /// [`Error::RequestBody`] or [`Error::RequestEncode`] when an input
    /// cannot be written.
    pub fn new(version: WireVersion, form: Form, request: &Request) -> Result<Self, Error> {
        let operation = request.operation();
        let fail = |source: crate::wire::RequestError| source.into_error(operation, version);
        let segments = request.segments().into_iter().map(str::to_owned).collect();
        if form == Form::Get
            && let Some(query) = crate::wire::request_query(version, request).map_err(fail)?
        {
            return Ok(Self {
                method: Method::GET,
                segments,
                query,
                body: None,
            });
        }
        Ok(Self {
            method: Method::POST,
            segments,
            query: Vec::new(),
            body: Some(crate::wire::request_body(version, request).map_err(fail)?),
        })
    }

    /// Returns the HTTP method, `GET` or `POST`.
    #[must_use]
    pub const fn method(&self) -> &Method {
        &self.method
    }

    /// Returns the path segments below the service base, unencoded:
    /// `[type, $code]` or `[type, id, $code]`.
    #[must_use]
    pub fn segments(&self) -> &[String] {
        &self.segments
    }

    /// Returns the query parameters of the `GET` form, in order, unencoded;
    /// empty for a `POST`.
    #[must_use]
    pub fn query(&self) -> &[(String, String)] {
        &self.query
    }

    /// Returns the `Parameters` body of the `POST` form, as FHIR JSON.
    #[must_use]
    pub fn body(&self) -> Option<&str> {
        self.body.as_deref()
    }

    /// Returns the address of the invocation below `base`, the FHIR service
    /// base URL, with every segment and query value percent-encoded.
    ///
    /// # Errors
    ///
    /// Returns [`Error::BaseUrl`] when `base` cannot be a base.
    pub fn url(&self, base: &Url) -> Result<Url, Error> {
        let mut url = base.clone();
        url.path_segments_mut()
            .map_err(|()| Error::BaseUrl { base: base.clone() })?
            .pop_if_empty()
            .extend(&self.segments);
        if !self.query.is_empty() {
            url.query_pairs_mut().extend_pairs(&self.query);
        }
        Ok(url)
    }
}

/// Returns what `request`, sent in `version` to `url`, answered with `status`
/// and `body`.
///
/// The answer is read exactly as [`Client`](crate::client::Client) reads
/// it: a `2xx` through the release's `out` contract, a refusal the
/// operation declares as an outcome (an unknown code on `$lookup`, an
/// unknown value set on `$expand`) as that outcome, and every other refusal
/// as an error carrying the upstream status and body.
///
/// # Errors
///
/// Returns [`Error::Body`] for a `2xx` body the contract does not read,
/// [`Error::Refused`], [`Error::Unauthorized`] or [`Error::ServerFailure`]
/// for a refusal, and [`Error::UnsupportedOperation`] for an operation the
/// release does not define.
pub fn answer(
    version: WireVersion,
    request: &Request,
    url: &Url,
    status: StatusCode,
    body: &str,
) -> Result<BatchOutcome, Error> {
    let answer = crate::client::Answer {
        url: url.clone(),
        status,
        body: body.to_owned(),
    };
    crate::decode::batch_entry(version, request, &answer)
}

/// Returns `segment` percent-encoded as one path segment: every byte outside
/// the `pchar` set of RFC 3986 §3.3 is written `%XX`.
pub(crate) fn path_segment(segment: &str) -> String {
    let mut written = String::with_capacity(segment.len());
    for byte in segment.bytes() {
        if byte.is_ascii_alphanumeric() || b"-._~!$&'()*+,;=:@".contains(&byte) {
            written.push(char::from(byte));
        } else {
            written.push('%');
            for nibble in [byte >> 4, byte & 0x0F] {
                written.extend(
                    char::from_digit(u32::from(nibble), 16).map(|digit| digit.to_ascii_uppercase()),
                );
            }
        }
    }
    written
}

#[cfg(test)]
mod tests {
    use super::path_segment;

    #[test]
    fn a_segment_keeps_its_pchars_and_encodes_the_rest() {
        assert_eq!("$validate-code", path_segment("$validate-code"));
        assert_eq!("a%2Fb%20c", path_segment("a/b c"));
        assert_eq!("%C3%A9", path_segment("é"));
    }
}
