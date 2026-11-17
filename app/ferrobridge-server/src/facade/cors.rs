// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Cross-origin resource sharing on the FHIR base.
//!
//! SMART App Launch 2.2.0, App Launch, §Considerations for Cross-Origin
//! Resource Sharing (CORS) support: a server that supports purely
//! browser-based apps "SHALL enable" CORS "for requests from any origin" on
//! the public discovery endpoints (`.well-known/smart-configuration` and
//! `metadata`), and "for requests from a client's registered origin(s)" on the
//! FHIR REST API endpoints. FHIR R4, Security, §6.1.0.3 Communications, asks
//! the same of a server that serves browser clients. The facade is a resource
//! server and holds no client registry, so the registered origins are the
//! deployment's `[facade.cors] allowed_origins` (no specification governs the
//! list's home: our own design).
//!
//! The REST routes never answer `*`: they echo an origin the list names,
//! carry no `Access-Control-Allow-Credentials`, and vary on `Origin`. The
//! layer sits outside the guard, so a refusal the guard writes is readable
//! by an admitted origin, and it answers an admitted origin's CORS-preflight
//! request itself, which carries no credential (Fetch Standard, §4.8
//! CORS-preflight fetch).

use std::collections::BTreeSet;
use std::sync::Arc;

use axum::extract::Request;
use axum::extract::State;
use axum::middleware::Next;
use axum::response::IntoResponse;
use axum::response::Response;
use ferrohealth_smart::resource::configuration::SMART_CONFIGURATION;
use http::HeaderMap;
use http::HeaderValue;
use http::Method;
use http::StatusCode;
use http::header;

use crate::mount::BASE_PATH;

/// The methods a preflight on a REST route is answered with, those the
/// facade mounts.
const REST_METHODS: HeaderValue = HeaderValue::from_static("GET, POST, PUT, DELETE");

/// The methods a preflight on a discovery endpoint is answered with.
const DISCOVERY_METHODS: HeaderValue = HeaderValue::from_static("GET");

/// The request headers a preflight admits: the bearer token, the FHIR
/// content negotiation and the conditional headers of the R4 HTTP chapter
/// (<https://hl7.org/fhir/R4/http.html>), and the request id the stack
/// correlates on.
const ALLOWED_HEADERS: HeaderValue = HeaderValue::from_static(
    "Accept, Authorization, Content-Type, If-Match, If-Modified-Since, If-None-Exist, If-None-Match, Prefer, X-Request-Id",
);

/// The response headers a cross-origin script may read beyond the
/// CORS-safelisted ones: those the R4 HTTP chapter answers with, the
/// challenge of a refusal (RFC 6750 §3) and the request id.
const EXPOSED_HEADERS: HeaderValue = HeaderValue::from_static(
    "Content-Location, ETag, Last-Modified, Location, WWW-Authenticate, X-Request-Id",
);

/// How long a browser may cache a preflight answer, in seconds (no
/// specification governs the value: our own design).
const MAX_AGE: HeaderValue = HeaderValue::from_static("600");

/// The `Vary` of a REST answer, whose CORS headers depend on the origin.
const VARY_ORIGIN: HeaderValue = HeaderValue::from_static("Origin");

/// The `Vary` of a preflight answer.
const VARY_PREFLIGHT: HeaderValue = HeaderValue::from_static(
    "Origin, Access-Control-Request-Method, Access-Control-Request-Headers",
);

/// An origin a browser sends in `Origin`, in its ASCII serialization
/// (RFC 6454 §6.2): a scheme, a host and a port other than the scheme's
/// default.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Origin(String);

/// Why a configured value is no origin.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum OriginError {
    /// The value is no absolute URL.
    #[error("the value is no absolute URL")]
    Unparsable,
    /// The scheme is neither `http` nor `https`.
    #[error("the scheme is neither http nor https")]
    Scheme,
    /// The value carries a user, a path, a query or a fragment.
    #[error("an origin is a scheme, a host and a port, with no user, path, query or fragment")]
    Beyond,
}

impl Origin {
    /// Returns the origin `value` names, written as a URL with nothing after
    /// the authority but an optional `/`, in its ASCII serialization.
    ///
    /// # Errors
    ///
    /// Returns an [`OriginError`] when `value` names no `http` or `https`
    /// origin, which refuses `*` and `null` too.
    pub fn parse(value: &str) -> Result<Self, OriginError> {
        let url = url::Url::parse(value).map_err(|_unparsable| OriginError::Unparsable)?;
        if !matches!(url.scheme(), "http" | "https") {
            return Err(OriginError::Scheme);
        }
        if !url.username().is_empty()
            || url.password().is_some()
            || url.path() != "/"
            || url.query().is_some()
            || url.fragment().is_some()
        {
            return Err(OriginError::Beyond);
        }
        Ok(Self(url.origin().ascii_serialization()))
    }

    /// Returns the origin's ASCII serialization.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// The CORS policy of the FHIR base: any origin on the discovery endpoints,
/// the listed origins on the REST routes.
#[derive(Debug, Clone, Default)]
pub struct Cors {
    /// The origins the REST routes admit.
    origins: Arc<BTreeSet<Origin>>,
}

/// Which CORS rule a path falls under.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Scope {
    /// A public discovery endpoint, open to any origin.
    Discovery,
    /// A FHIR REST route, open to the listed origins.
    Rest,
}

impl Cors {
    /// Returns the policy admitting `origins` on the REST routes.
    #[must_use]
    pub fn new(origins: impl IntoIterator<Item = Origin>) -> Self {
        Self {
            origins: Arc::new(origins.into_iter().collect()),
        }
    }

    /// Returns the origins the REST routes admit.
    pub fn origins(&self) -> impl Iterator<Item = &Origin> {
        self.origins.iter()
    }

    /// Returns the `Access-Control-Allow-Origin` a request from `origin`
    /// under `scope` is answered with, `None` for an origin the scope does
    /// not admit.
    fn allow(&self, scope: Scope, origin: &HeaderValue) -> Option<HeaderValue> {
        match scope {
            Scope::Discovery => Some(HeaderValue::from_static("*")),
            Scope::Rest => {
                let origin_text = origin.to_str().ok()?;
                self.origins
                    .iter()
                    .any(|listed| listed.as_str() == origin_text)
                    .then(|| origin.clone())
            }
        }
    }
}

/// Returns the CORS rule `path` falls under, `None` outside the FHIR base and
/// the root discovery document.
fn scope(path: &str) -> Option<Scope> {
    let relative = path.strip_prefix(BASE_PATH);
    if path == SMART_CONFIGURATION
        || relative
            .is_some_and(|relative| relative == SMART_CONFIGURATION || relative == "/metadata")
    {
        return Some(Scope::Discovery);
    }
    relative
        .is_some_and(|relative| relative.is_empty() || relative.starts_with('/'))
        .then_some(Scope::Rest)
}

/// Returns whether `headers` on an `OPTIONS` request make it a
/// CORS-preflight request (Fetch Standard, §3.2.2 HTTP requests).
fn is_preflight(method: &Method, headers: &HeaderMap) -> bool {
    method == Method::OPTIONS
        && headers.contains_key(header::ORIGIN)
        && headers.contains_key(header::ACCESS_CONTROL_REQUEST_METHOD)
}

/// Answers the CORS-preflight request of an admitted origin.
fn preflight(scope: Scope, allow: HeaderValue) -> Response {
    let methods = match scope {
        Scope::Discovery => DISCOVERY_METHODS,
        Scope::Rest => REST_METHODS,
    };
    (
        StatusCode::NO_CONTENT,
        [
            (header::ACCESS_CONTROL_ALLOW_ORIGIN, allow),
            (header::ACCESS_CONTROL_ALLOW_METHODS, methods),
            (header::ACCESS_CONTROL_ALLOW_HEADERS, ALLOWED_HEADERS),
            (header::ACCESS_CONTROL_MAX_AGE, MAX_AGE),
            (header::VARY, VARY_PREFLIGHT),
        ],
    )
        .into_response()
}

/// Applies `cors` to a request: answers an admitted origin's preflight, and
/// adds the CORS headers to the answer of an admitted origin's request.
///
/// A request from an origin the rule does not admit runs as it would with no
/// CORS at all, so its answer carries no `Access-Control-Allow-Origin` and a
/// browser withholds it.
pub async fn apply(State(cors): State<Cors>, request: Request, next: Next) -> Response {
    let Some(scope) = scope(request.uri().path()) else {
        return next.run(request).await;
    };
    let allowed = request
        .headers()
        .get(header::ORIGIN)
        .and_then(|origin| cors.allow(scope, origin));
    let varies = scope == Scope::Rest && !cors.origins.is_empty();
    let Some(allow) = allowed else {
        let mut response = next.run(request).await;
        if varies {
            response.headers_mut().append(header::VARY, VARY_ORIGIN);
        }
        return response;
    };
    if is_preflight(request.method(), request.headers()) {
        return preflight(scope, allow);
    }
    let mut response = next.run(request).await;
    let headers = response.headers_mut();
    headers.insert(header::ACCESS_CONTROL_ALLOW_ORIGIN, allow);
    headers.insert(header::ACCESS_CONTROL_EXPOSE_HEADERS, EXPOSED_HEADERS);
    if varies {
        headers.append(header::VARY, VARY_ORIGIN);
    }
    response
}

#[cfg(test)]
mod tests {
    use super::{Origin, OriginError, Scope, scope};

    #[test]
    fn an_origin_is_read_in_its_serialization() {
        for (value, origin) in [
            ("https://app.example", "https://app.example"),
            ("https://App.Example/", "https://app.example"),
            ("https://app.example:443", "https://app.example"),
            ("http://localhost:4567", "http://localhost:4567"),
        ] {
            assert_eq!(
                Ok(origin),
                Origin::parse(value).as_ref().map(Origin::as_str)
            );
        }
    }

    #[test]
    fn a_value_that_is_no_origin_is_refused() {
        for (value, error) in [
            ("*", OriginError::Unparsable),
            ("null", OriginError::Unparsable),
            ("app.example", OriginError::Unparsable),
            ("ftp://app.example", OriginError::Scheme),
            ("https://app.example/fhir", OriginError::Beyond),
            ("https://app.example/?a=b", OriginError::Beyond),
            ("https://user@app.example", OriginError::Beyond),
            ("https://app.example/#", OriginError::Beyond),
        ] {
            assert_eq!(Err(error), Origin::parse(value), "{value}");
        }
    }

    #[test]
    fn the_discovery_endpoints_are_public_and_the_base_is_rest() {
        for path in [
            "/.well-known/smart-configuration",
            "/fhir/.well-known/smart-configuration",
            "/fhir/metadata",
        ] {
            assert_eq!(Some(Scope::Discovery), scope(path), "{path}");
        }
        for path in [
            "/fhir",
            "/fhir/Patient",
            "/fhir/Patient/p-1/_history",
            "/fhir/$tofhir",
        ] {
            assert_eq!(Some(Scope::Rest), scope(path), "{path}");
        }
        for path in ["/", "/health", "/fhirx", "/operator/mappings"] {
            assert_eq!(None, scope(path), "{path}");
        }
    }
}
