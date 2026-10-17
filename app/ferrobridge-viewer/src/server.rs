// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The console's HTTP service: the health route, the Leptos pages and the
//! site bundle, under one set of browser security headers.
//!
//! Every response carries a Content-Security-Policy whose script source is a
//! nonce minted for that response alone, `nosniff`, `DENY` framing and no
//! referrer, and every document is `no-store`. Only the site bundle is
//! compressed. No specification governs the console's HTTP surface: our own
//! design.

use std::sync::Arc;

use axum::Extension;
use axum::extract::Request;
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use http::header::{
    CACHE_CONTROL, CONTENT_SECURITY_POLICY, REFERRER_POLICY, X_CONTENT_TYPE_OPTIONS,
    X_FRAME_OPTIONS,
};
use http::{HeaderValue, StatusCode};
use leptos::config::LeptosOptions;
use leptos::nonce::Nonce;
use leptos_axum::LeptosRoutes as _;
use serde::Serialize;
use tower_http::compression::CompressionLayer;
use tower_http::compression::predicate::{And, Predicate as _, SizeAbove};
use tower_http::set_header::SetResponseHeaderLayer;

use crate::bridge::{Bridge, BridgeError};
use crate::config::settings::Settings;

/// The path of the console's own liveness route.
pub const HEALTH: &str = "/health";

/// The name cargo-leptos gives the site bundle's files under `pkg/`.
pub const OUTPUT_NAME: &str = "ferrobridge-viewer";

/// What every request handler shares.
#[derive(Debug, Clone)]
pub struct ViewerState {
    settings: Arc<Settings>,
    bridge: Bridge,
}

impl ViewerState {
    /// The state for `settings`.
    ///
    /// # Errors
    /// Returns the [`BridgeError`] of a bridge client that cannot be built.
    pub fn new(settings: Settings) -> Result<Self, BridgeError> {
        let bridge = Bridge::new(&settings.bridge)?;
        Ok(Self {
            settings: Arc::new(settings),
            bridge,
        })
    }

    /// The resolved configuration.
    #[must_use]
    pub fn settings(&self) -> &Settings {
        &self.settings
    }

    /// The bridge client.
    #[must_use]
    pub fn bridge(&self) -> &Bridge {
        &self.bridge
    }
}

/// The Leptos options the console renders with: its site bundle under the
/// configured site root and its listen address.
#[must_use]
pub fn leptos_options(settings: &Settings) -> LeptosOptions {
    LeptosOptions::builder()
        .output_name(OUTPUT_NAME)
        .site_root(settings.site_root.to_string_lossy().into_owned())
        .site_pkg_dir("pkg")
        .site_addr(settings.listen)
        .build()
}

/// Builds the console's whole HTTP service over `state`.
pub fn router(state: ViewerState) -> axum::Router {
    let options = leptos_options(state.settings());
    let routes = leptos_axum::generate_route_list(crate::app::App);
    let context = {
        let state = state.clone();
        move || {
            leptos::context::provide_context(state.clone());
            provide_request_nonce();
        }
    };
    let pages = axum::Router::new()
        .route(HEALTH, get(health))
        .leptos_routes_with_context(&options, routes, context.clone(), {
            let options = options.clone();
            move || crate::app::shell(options.clone())
        })
        .fallback(leptos_axum::file_and_error_handler_with_context(
            context,
            crate::app::shell,
        ))
        .layer(Extension(state));
    with_security_headers(pages.layer(bundle_compression())).with_state(options)
}

/// The body of the liveness route.
#[derive(Debug, Serialize)]
struct Health {
    /// Always `up`: the route answers while the process serves.
    status: &'static str,
}

/// Serves `GET /health`: `200` while the process serves.
async fn health() -> Response {
    (StatusCode::OK, axum::Json(Health { status: "up" })).into_response()
}

/// The media types of the site bundle: the WebAssembly, its JavaScript glue
/// and the stylesheet.
const BUNDLE_TYPES: [&str; 4] = [
    "application/wasm",
    "text/javascript",
    "application/javascript",
    "text/css",
];

/// A test of a response, the shape the compression layer's predicate takes.
type ResponsePredicate = fn(StatusCode, http::Version, &http::HeaderMap, &http::Extensions) -> bool;

/// Whether a response carries a file of the site bundle, by its media type.
fn is_bundle(
    _status: StatusCode,
    _version: http::Version,
    headers: &http::HeaderMap,
    _extensions: &http::Extensions,
) -> bool {
    headers
        .get(http::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|media| BUNDLE_TYPES.iter().any(|bundle| media.starts_with(bundle)))
}

/// Compresses the site bundle with brotli or gzip, as the request's
/// `Accept-Encoding` chooses, and marks it `Vary: Accept-Encoding`.
///
/// Only the bundle is compressed. A document or a server function's answer
/// can carry what the operator entered beside data an attacker would want,
/// so it is never compressed, which keeps a compression side channel (BREACH)
/// shut. The Leptos book (`deployment/binary_size`) has a site serve its
/// WebAssembly compressed.
fn bundle_compression() -> CompressionLayer<And<SizeAbove, ResponsePredicate>> {
    let bundle: ResponsePredicate = is_bundle;
    CompressionLayer::new()
        .br(true)
        .gzip(true)
        .compress_when(SizeAbove::new(256).and(bundle))
}

/// Wraps `router` in the browser security headers every response carries.
fn with_security_headers(router: axum::Router<LeptosOptions>) -> axum::Router<LeptosOptions> {
    router
        .layer(axum::middleware::from_fn(content_security_policy))
        .layer(axum::middleware::from_fn(cache_control))
        .layer(SetResponseHeaderLayer::overriding(
            X_CONTENT_TYPE_OPTIONS,
            HeaderValue::from_static("nosniff"),
        ))
        .layer(SetResponseHeaderLayer::overriding(
            X_FRAME_OPTIONS,
            HeaderValue::from_static("DENY"),
        ))
        .layer(SetResponseHeaderLayer::overriding(
            REFERRER_POLICY,
            HeaderValue::from_static("no-referrer"),
        ))
}

/// Mints this response's script nonce, hands it to the renderer through the
/// request, and answers with the policy that names it.
async fn content_security_policy(mut request: Request, next: Next) -> Response {
    let nonce = Nonce::new();
    let policy = HeaderValue::from_str(&policy(&nonce));
    request.extensions_mut().insert(nonce);
    let mut response = next.run(request).await;
    match policy {
        Ok(policy) => {
            response
                .headers_mut()
                .insert(CONTENT_SECURITY_POLICY, policy);
            response
        }
        Err(_invalid) => {
            tracing::error!("the Content-Security-Policy could not be written as a header");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

/// Re-provides the request's nonce into the render, so the bootstrap script
/// carries the nonce the response header authorizes.
///
/// `leptos_axum` provides a nonce of its own before this runs, and the
/// request parts it provides carry the one [`content_security_policy`]
/// minted, which replaces it.
fn provide_request_nonce() {
    let Some(parts) = leptos::context::use_context::<http::request::Parts>() else {
        return;
    };
    if let Some(nonce) = parts.extensions.get::<Nonce>() {
        leptos::context::provide_context(nonce.clone());
    }
}

/// The Content-Security-Policy of one response, naming its script nonce.
///
/// The WebAssembly bundle needs `'wasm-unsafe-eval'` to be compiled, which
/// admits no JavaScript `eval` (Content Security Policy Level 3). Scripts run
/// by nonce alone, and nothing admits `'unsafe-inline'`.
#[must_use]
pub fn policy(nonce: &Nonce) -> String {
    format!(
        "default-src 'self'; \
         script-src 'self' 'wasm-unsafe-eval' 'nonce-{nonce}'; \
         style-src 'self'; \
         img-src 'self' data:; \
         connect-src 'self'; \
         object-src 'none'; \
         base-uri 'self'; \
         form-action 'self'; \
         frame-ancestors 'none'"
    )
}

/// Marks every document `no-store`, and the site bundle `no-cache`, so a
/// rebuilt bundle is fetched again (RFC 9111 §5.2.2).
async fn cache_control(request: Request, next: Next) -> Response {
    let bundle = request.uri().path().starts_with("/pkg/");
    let mut response = next.run(request).await;
    let value = if bundle && response.status().is_success() {
        "no-cache"
    } else {
        "no-store"
    };
    response
        .headers_mut()
        .insert(CACHE_CONTROL, HeaderValue::from_static(value));
    response
}

/// Serves `router` on `listener` until `shutdown` resolves.
///
/// # Errors
/// Returns the I/O error of a listener that fails.
pub async fn serve(
    listener: tokio::net::TcpListener,
    router: axum::Router,
    shutdown: impl Future<Output = ()> + Send + 'static,
) -> std::io::Result<()> {
    axum::serve(listener, router)
        .with_graceful_shutdown(shutdown)
        .await
}

#[cfg(test)]
mod tests {
    use super::policy;
    use leptos::nonce::Nonce;

    #[test]
    fn the_policy_allows_scripts_only_by_nonce_and_never_eval() {
        let nonce = Nonce::new();
        let policy = policy(&nonce);
        let script = policy
            .split(';')
            .map(str::trim)
            .find(|directive| directive.starts_with("script-src"))
            .expect("a script-src directive");
        assert_eq!(
            format!("script-src 'self' 'wasm-unsafe-eval' 'nonce-{nonce}'"),
            script
        );
        assert!(!policy.contains("'unsafe-eval'"), "{policy}");
        assert!(!policy.contains("'unsafe-inline'"), "{policy}");
    }
}
