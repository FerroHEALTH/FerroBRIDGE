// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The console's HTTP service: the health route, the sign-in routes, the
//! Leptos pages, the form posts they send and the site bundle, under one
//! set of browser security headers.
//!
//! Every response carries a Content-Security-Policy whose script source is a
//! nonce minted for that response alone, `nosniff`, `DENY` framing and no
//! referrer, and every document is `no-store`. Only the site bundle is
//! compressed. A request that is not a safe method is taken only from the
//! console's own pages, and a session's access token is refreshed before
//! the request it arrives with is served. No specification governs the
//! console's HTTP surface: our own design.

use std::sync::Arc;

use axum::Extension;
use axum::extract::Request;
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use http::header::{
    CACHE_CONTROL, CONTENT_SECURITY_POLICY, CONTENT_TYPE, REFERRER_POLICY, X_CONTENT_TYPE_OPTIONS,
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
use crate::config::settings::{OidcSettings, Settings};
use crate::locale::messages::Msg;
use crate::locale::{catalog, negotiated, tr_in};
use crate::session::Sessions;
use ferrohealth_smart::metadata::{Http, ServerMetadata};
use ferrohealth_smart::oidc::{OidcError, Provider, RelyingParty};

/// The path of the console's own liveness route.
pub const HEALTH: &str = "/health";

/// The name cargo-leptos gives the site bundle's files under `pkg/`.
pub const OUTPUT_NAME: &str = "ferrobridge-console";

/// What every request handler shares.
#[derive(Debug, Clone)]
pub struct ConsoleState {
    settings: Arc<Settings>,
    sessions: Sessions,
    bridge: Bridge,
    relying_party: Option<RelyingParty>,
}

/// Why the console's state could not be built.
#[derive(Debug, thiserror::Error)]
pub enum StateError {
    /// A client of the bridge or of the OpenID Provider could not be built.
    #[error(transparent)]
    Client(#[from] BridgeError),
    /// The configured OpenID Provider endpoints are not ones a relying party
    /// can use.
    #[error("the OpenID Provider's endpoints cannot be used")]
    Provider {
        /// What the relying party refused.
        #[source]
        source: OidcError,
    },
}

impl ConsoleState {
    /// The state for `settings`, with an empty session store.
    ///
    /// # Errors
    /// Returns [`StateError::Client`] when the bridge client, or the client of
    /// the OpenID Provider, cannot be built, and [`StateError::Provider`]
    /// when the configured provider endpoints are not ones a relying party
    /// can use.
    pub fn new(settings: Settings) -> Result<Self, StateError> {
        let bridge = Bridge::new(&settings.bridge)?;
        let relying_party = settings
            .oidc
            .as_ref()
            .map(|oidc| relying_party(oidc, &settings))
            .transpose()?;
        Ok(Self {
            sessions: Sessions::new(settings.session),
            settings: Arc::new(settings),
            bridge,
            relying_party,
        })
    }

    /// The resolved configuration.
    #[must_use]
    pub fn settings(&self) -> &Settings {
        &self.settings
    }

    /// The session store.
    #[must_use]
    pub fn sessions(&self) -> &Sessions {
        &self.sessions
    }

    /// The bridge client.
    #[must_use]
    pub fn bridge(&self) -> &Bridge {
        &self.bridge
    }

    /// The relying party of the OpenID Provider (`ferrohealth-smart`), which
    /// the sign-in exchanges its code with and a session refreshes its access
    /// token with; `None` when no provider is configured.
    #[must_use]
    pub fn relying_party(&self) -> Option<&RelyingParty> {
        self.relying_party.as_ref()
    }
}

/// The relying party of the provider `oidc` names, calling it within the
/// timeout of a call to the bridge.
fn relying_party(oidc: &OidcSettings, settings: &Settings) -> Result<RelyingParty, StateError> {
    // NOTE: RFC 6749 §3.2: the token endpoint answers the request itself, so
    // the provider's client follows no redirect.
    let client = reqwest::Client::builder()
        .timeout(settings.bridge.timeout)
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|source| BridgeError::Transport { source })?;
    let metadata = ServerMetadata {
        issuer: oidc.issuer.clone(),
        authorization_endpoint: Some(oidc.authorization_endpoint.to_string()),
        token_endpoint: Some(oidc.token_endpoint.to_string()),
        jwks_uri: Some(oidc.jwks_uri.to_string()),
        end_session_endpoint: oidc.end_session_endpoint.as_ref().map(ToString::to_string),
        ..ServerMetadata::default()
    };
    let provider =
        Provider::from_metadata(&metadata).map_err(|source| StateError::Provider { source })?;
    let mut relying_party = RelyingParty::new(
        Http::from_client(client),
        provider,
        oidc.client_id.clone(),
        oidc.redirect_uri.clone(),
    )
    .with_scopes(oidc.scopes.iter().cloned());
    if let Some(secret) = &oidc.client_secret {
        relying_party = relying_party.with_client_secret(secret.clone());
    }
    if oidc.offline_access {
        relying_party = relying_party.with_offline_access();
    }
    if let Some(redirect) = &oidc.post_logout_redirect_uri {
        relying_party = relying_party.with_post_logout_redirect_uri(redirect.clone());
    }
    Ok(relying_party)
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
pub fn router(state: ConsoleState) -> axum::Router {
    let options = leptos_options(state.settings());
    let form_action = form_action(state.settings());
    let routes = leptos_axum::generate_route_list(crate::app::App);
    let context = page_context(state.clone(), options.clone());
    let pages = axum::Router::new()
        .route(HEALTH, get(health))
        .route(crate::app::STYLESHEET, get(stylesheet))
        .route(crate::app::THEME, get(theme))
        .route(crate::app::MARK_LIGHT, get(mark_light))
        .route(crate::app::MARK_DARK, get(mark_dark))
        .route(crate::pages::about::INFO_JSON, get(info_json))
        .route(
            crate::pages::search::ANSWER,
            get(crate::pages::search::answer_route),
        )
        .route(crate::app::PREFERENCE, post(crate::prefs::choose))
        .route(crate::app::RESET, post(crate::prefs::reset))
        .route(crate::app::SIDEBAR, post(crate::prefs::rail))
        .route(crate::pages::mappings::RELOAD, post(crate::posts::reload))
        .route(crate::app::NEW_FILE, post(crate::posts::edit))
        .route(
            &format!("{}/{{*path}}", crate::app::EDIT),
            post(crate::posts::edit),
        )
        .route(crate::app::PLAYGROUND, post(crate::posts::run))
        .route(
            &format!("{}/{{tab}}", crate::app::PLAYGROUND),
            post(crate::posts::run),
        )
        .route(crate::oidc::LOGIN, get(crate::oidc::login))
        .route(crate::oidc::CALLBACK, get(crate::oidc::callback))
        .route(
            crate::oidc::logout::LOGOUT,
            post(crate::oidc::logout::logout),
        )
        .leptos_routes_with_context(&options, routes, context.clone(), {
            let options = options.clone();
            move || crate::app::document(options.clone())
        })
        .fallback(leptos_axum::file_and_error_handler_with_context(
            context,
            crate::app::document,
        ))
        .layer(axum::middleware::from_fn(
            crate::oidc::refresh::refresh_session,
        ))
        .layer(axum::middleware::from_fn(same_origin_only))
        .layer(Extension(state));
    with_security_headers(pages.layer(bundle_compression()), form_action).with_state(options)
}

/// The fetch metadata header that names where a request comes from.
const SEC_FETCH_SITE: &str = "sec-fetch-site";

/// Refuses every request that is not a safe method, every form post and
/// the sign-out among them, unless it comes from the console's own pages
/// ([`same_origin`]), before anything reads the session or asks the bridge.
///
/// The session cookie is `SameSite=Lax` as well, so a cross-site `POST`
/// carries no session in the first place; this check holds without it.
async fn same_origin_only(
    Extension(state): Extension<ConsoleState>,
    request: Request,
    next: Next,
) -> Response {
    if request.method().is_safe() {
        return next.run(request).await;
    }
    let origin = state.settings().oidc.as_ref().map(OidcSettings::origin);
    if same_origin(request.headers(), origin.as_deref()) {
        return next.run(request).await;
    }
    tracing::warn!(
        method = %request.method(),
        "a request that did not come from the console's own pages was refused"
    );
    let locale = negotiated(request.headers(), state.settings().locale.clone());
    (
        StatusCode::FORBIDDEN,
        [(CACHE_CONTROL, "no-store")],
        tr_in(catalog(), &locale, Msg::ForeignRequestRefused),
    )
        .into_response()
}

/// Whether `headers` show a request sent from a page of the console at
/// `origin`, its own `scheme://host:port`.
///
/// `Sec-Fetch-Site` decides when the browser sends it, and only
/// `same-origin` passes. Without it `Origin` must be `origin`, and without
/// that the origin of `Referer` must be. A request that shows none of them,
/// or a console with no known origin, is refused, so an unsafe request is
/// never taken on trust (the W3C Fetch Metadata Request Headers; RFC 6454
/// §7).
///
/// The console answers with `Referrer-Policy: no-referrer`, under which a
/// browser sends a plain form post with `Origin: null` and no `Referer` (the
/// WHATWG Fetch standard, the request `Origin` header). The console's own
/// forms, the sign-out among them, therefore pass by `Sec-Fetch-Site`
/// alone, and a browser without fetch metadata is refused, which fails
/// closed. The `Origin` and `Referer` fallbacks serve a client that sends
/// them in place of fetch metadata.
#[must_use]
pub fn same_origin(headers: &http::HeaderMap, origin: Option<&str>) -> bool {
    if let Some(site) = headers.get(SEC_FETCH_SITE) {
        return site.as_bytes() == b"same-origin";
    }
    let Some(origin) = origin else {
        return false;
    };
    if let Some(sent) = headers.get(http::header::ORIGIN) {
        return sent.as_bytes() == origin.as_bytes();
    }
    // NOTE: RFC 9110 §10.1.3: a `Referer` that is no URL names no origin, which
    // refuses the request as a missing one does.
    headers
        .get(http::header::REFERER)
        .and_then(|referer| referer.to_str().ok())
        .and_then(|referer| url::Url::parse(referer).ok())
        .is_some_and(|referer| referer.origin().ascii_serialization() == origin)
}

/// The sources of the policy's `form-action`: the console itself, and the
/// origin of the provider's end-session endpoint when one is configured.
///
/// A browser holds every redirect a form submission follows to
/// `form-action` (Content Security Policy Level 3, `form-action`), and the
/// sign-out form is answered with a redirect to that endpoint (OpenID
/// Connect RP-Initiated Logout 1.0 §2), so without its origin the browser
/// blocks the sign-out on its way to the provider.
#[must_use]
pub fn form_action(settings: &Settings) -> String {
    let end_session = settings
        .oidc
        .as_ref()
        .and_then(|oidc| oidc.end_session_endpoint.as_ref())
        .map(url::Url::origin)
        .filter(url::Origin::is_tuple);
    match end_session {
        Some(origin) => format!("'self' {}", origin.ascii_serialization()),
        None => String::from("'self'"),
    }
}

/// The body of the liveness route.
#[derive(Debug, Serialize)]
struct Health {
    /// Always `up`: the route answers while the process serves.
    status: &'static str,
}

/// The brand mark the lockup draws in the light theme.
const MARK_LIGHT_SVG: &str = include_str!("../../../assets/brand/ferrobridge-icon.svg");

/// The brand mark the lockup draws in the dark theme.
const MARK_DARK_SVG: &str = include_str!("../../../assets/brand/ferrobridge-icon-dark.svg");

/// A stylesheet answer of `text`.
fn css(text: &'static str) -> Response {
    (
        [(
            CONTENT_TYPE,
            HeaderValue::from_static("text/css; charset=utf-8"),
        )],
        text,
    )
        .into_response()
}

/// Serves the `ferrohealth_ui` stylesheet, the roles and every component
/// class the console's views paint with.
async fn stylesheet() -> Response {
    css(ferrohealth_ui::style::STYLESHEET)
}

/// Serves the FerroBRIDGE theme of `ferrohealth_ui`: the brand's accents
/// and the two preview accents of Your preferences.
async fn theme() -> Response {
    css(crate::app::BRAND.theme())
}

/// An SVG image answer of `text`.
fn svg(text: &'static str) -> Response {
    (
        [(CONTENT_TYPE, HeaderValue::from_static("image/svg+xml"))],
        text,
    )
        .into_response()
}

/// Serves the bridge's `GET /health/info` document as a download, for a
/// signed-in operator: the About page's self-description.
async fn info_json(
    Extension(state): Extension<ConsoleState>,
    headers: http::HeaderMap,
) -> Response {
    let token = state
        .sessions()
        .cookie_in(&headers, crate::session::COOKIE)
        .and_then(|id| state.sessions().access_token(&id).ok().flatten());
    let Some(token) = token else {
        return StatusCode::UNAUTHORIZED.into_response();
    };
    match state
        .bridge()
        .health_info(&crate::bridge::AccessToken::new(token))
        .await
    {
        Ok(info) => (
            [(
                http::header::CONTENT_DISPOSITION,
                HeaderValue::from_static("attachment; filename=\"health-info.json\""),
            )],
            axum::Json(info),
        )
            .into_response(),
        Err(error) => {
            tracing::warn!(error = %crate::command::chain(&error), "the bridge's info could not be read");
            StatusCode::BAD_GATEWAY.into_response()
        }
    }
}

/// Serves the brand mark of the light theme.
async fn mark_light() -> Response {
    svg(MARK_LIGHT_SVG)
}

/// Serves the brand mark of the dark theme.
async fn mark_dark() -> Response {
    svg(MARK_DARK_SVG)
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
        .get(CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|media| BUNDLE_TYPES.iter().any(|bundle| media.starts_with(bundle)))
}

/// Compresses the site bundle with brotli or gzip, as the request's
/// `Accept-Encoding` chooses, and marks it `Vary: Accept-Encoding`.
///
/// Only the bundle is compressed. A document, the answer to a form post
/// among them, can carry what the operator entered beside data an attacker would want,
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

/// Wraps `router` in the browser security headers every response carries,
/// with `form_action` as the policy's `form-action` sources.
fn with_security_headers(
    router: axum::Router<LeptosOptions>,
    form_action: String,
) -> axum::Router<LeptosOptions> {
    let form_action: Arc<str> = Arc::from(form_action);
    router
        .layer(axum::middleware::from_fn(
            move |request: Request, next: Next| {
                content_security_policy(Arc::clone(&form_action), request, next)
            },
        ))
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
/// request, and answers with the policy that names it and `form_action`.
async fn content_security_policy(
    form_action: Arc<str>,
    mut request: Request,
    next: Next,
) -> Response {
    let nonce = Nonce::new();
    let policy = HeaderValue::from_str(&policy(&nonce, &form_action));
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

/// What every render of a page provides: the console's state, the Leptos
/// options, the request's nonce and a fresh set of reads from the bridge.
fn page_context(
    state: ConsoleState,
    options: LeptosOptions,
) -> impl Fn() + Clone + Send + Sync + 'static {
    move || {
        leptos::context::provide_context(state.clone());
        leptos::context::provide_context(options.clone());
        leptos::context::provide_context(crate::load::cached::Reads::new());
        provide_request_nonce();
    }
}

/// Draws the page at the address of `parts` with what `extra` provides
/// beside the page context: the answer of a form post.
pub async fn render(
    state: ConsoleState,
    parts: http::request::Parts,
    extra: impl Fn() + Clone + Send + Sync + 'static,
) -> Response {
    let options = leptos_options(state.settings());
    let context = page_context(state, options.clone());
    let handler = leptos_axum::render_app_async_with_context(
        move || {
            context();
            extra();
        },
        move || crate::app::document(options.clone()),
    );
    handler(Request::from_parts(parts, axum::body::Body::empty())).await
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

/// The Content-Security-Policy of one response, naming its script nonce and
/// the `form-action` sources [`form_action`] returns.
///
/// The WebAssembly bundle needs `'wasm-unsafe-eval'` to be compiled, which
/// admits no JavaScript `eval` (Content Security Policy Level 3). Scripts run
/// by nonce alone, and nothing admits `'unsafe-inline'`.
#[must_use]
pub fn policy(nonce: &Nonce, form_action: &str) -> String {
    format!(
        "default-src 'self'; \
         script-src 'self' 'wasm-unsafe-eval' 'nonce-{nonce}'; \
         style-src 'self'; \
         img-src 'self' data:; \
         connect-src 'self'; \
         object-src 'none'; \
         base-uri 'self'; \
         form-action {form_action}; \
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
        let policy = policy(&nonce, "'self'");
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
