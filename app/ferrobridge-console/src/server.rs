// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The console's HTTP service: the health route, the sign-in routes, the
//! Leptos pages, the form posts they send and the site bundle, under one
//! set of browser security headers.
//!
//! Every response carries the browser security headers of
//! `ferrohealth-server`'s `console`: a Content-Security-Policy whose script
//! source is a nonce minted for that response alone, `nosniff`, `DENY`
//! framing and no referrer, and every document is `no-store`. Only the site
//! bundle is compressed. A request that is not a safe method is taken only
//! from the console's own pages (`ferrohealth_server::same_origin`), and a
//! session's access token is refreshed before the request it arrives with is
//! served. The health routes are the family's. No specification governs the
//! console's HTTP surface: our own design.

use std::sync::Arc;

use axum::Extension;
use axum::extract::Request;
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use ferrohealth_health::lifecycle::Lifecycle;
use ferrohealth_health::registry::Registry;
use ferrohealth_server::console::{self, ConsoleHeaders, CspNonce};
use ferrohealth_server::health::Health;
use ferrohealth_server::same_origin::{self, SameOrigin};
use ferrohealth_server::section::ServerSection;
use ferrohealth_server::serve::Drained;
use http::header::CONTENT_TYPE;
use http::{HeaderValue, StatusCode};
use leptos::config::LeptosOptions;
use leptos::nonce::Nonce;
use leptos_axum::LeptosRoutes as _;
use tower_http::compression::CompressionLayer;
use tower_http::compression::predicate::{And, Predicate as _, SizeAbove};

use crate::bridge::{Bridge, BridgeError};
use crate::config::settings::Settings;
use crate::locale::messages::Msg;
use crate::locale::{catalog, tr_in};
use crate::session::Sessions;

/// The path of the console's summary health route.
pub const HEALTH: &str = ferrohealth_server::health::SUMMARY;

/// The path the site bundle is served under.
pub const BUNDLE: &str = "/pkg";

/// The name cargo-leptos gives the site bundle's files under `pkg/`.
pub const OUTPUT_NAME: &str = "ferrobridge-console";

/// What every request handler shares.
#[derive(Debug, Clone)]
pub struct ConsoleState {
    settings: Arc<Settings>,
    sessions: Sessions,
    bridge: Bridge,
    provider: reqwest::Client,
    health: Health,
}

impl ConsoleState {
    /// The state for `settings`, with an empty session store.
    ///
    /// # Errors
    /// Returns the [`BridgeError`] of a bridge client, or of the client of
    /// the OpenID Provider, that cannot be built.
    pub fn new(settings: Settings) -> Result<Self, BridgeError> {
        let bridge = Bridge::new(&settings.bridge)?;
        // NOTE: RFC 6749 §3.2: the token endpoint answers the request itself, so
        // the provider's client follows no redirect.
        let provider = reqwest::Client::builder()
            .timeout(settings.bridge.timeout)
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|source| BridgeError::Transport { source })?;
        // The settings are resolved and both clients built, so the console
        // serves from here.
        let lifecycle = Lifecycle::default();
        lifecycle.booted();
        Ok(Self {
            sessions: Sessions::new(settings.session),
            settings: Arc::new(settings),
            bridge,
            provider,
            health: Health::new(Registry::default(), lifecycle),
        })
    }

    /// The health routes' view: no indicator, and the lifecycle the drain
    /// moves.
    #[must_use]
    pub fn health(&self) -> &Health {
        &self.health
    }

    /// The lifecycle the health routes and the drain share.
    #[must_use]
    pub fn lifecycle(&self) -> &Lifecycle {
        self.health.lifecycle()
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

    /// The HTTP client of the OpenID Provider, which the sign-in exchanges
    /// its code with and a session refreshes its access token with.
    #[must_use]
    pub fn provider(&self) -> &reqwest::Client {
        &self.provider
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
pub fn router(state: ConsoleState) -> axum::Router {
    let options = leptos_options(state.settings());
    let form_action = form_action(state.settings());
    let routes = leptos_axum::generate_route_list(crate::app::App);
    let context = page_context(state.clone(), options.clone());
    let health = ferrohealth_server::health::routes(state.health().clone());
    let same_origin = same_origin_check(state.settings());
    let pages = axum::Router::new()
        .route_service(HEALTH, health.clone())
        .route_service(ferrohealth_server::health::LIVENESS, health.clone())
        .route_service(ferrohealth_server::health::READINESS, health)
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
        .layer(axum::middleware::from_fn(
            move |request: Request, next: Next| {
                same_origin::only(same_origin.clone(), request, next)
            },
        ))
        .layer(Extension(state));
    let headers = ConsoleHeaders::new(form_action).with_bundle(BUNDLE);
    pages
        .layer(bundle_compression())
        .layer(axum::middleware::from_fn(
            move |request: Request, next: Next| console::headers(headers.clone(), request, next),
        ))
        .with_state(options)
}

/// The same-origin check of every request that is not a safe method, every
/// form post and the sign-out among them, before anything reads the session
/// or asks the bridge: the console's own origin from the provider's
/// `redirect_uri`, and the refusal in the console's words.
///
/// The session cookie is `SameSite=Lax` as well, so a cross-site `POST`
/// carries no session in the first place; this check holds without it.
fn same_origin_check(settings: &Settings) -> SameOrigin {
    let locale = settings.locale.clone();
    let check = SameOrigin::new().with_body(Arc::new(move |_uri, _request_id, status, _reason| {
        (
            status,
            tr_in(catalog(), &locale, Msg::ForeignRequestRefused),
        )
            .into_response()
    }));
    let origin = settings
        .oidc
        .as_ref()
        .and_then(|oidc| url::Url::parse(&oidc.origin()).ok());
    match origin {
        // NOTE: no specification governs this: our own design; an origin that is
        // opaque names none, which refuses every unsafe request lacking fetch metadata.
        Some(origin) => check
            .with_origin(&origin)
            .unwrap_or_else(|_opaque| SameOrigin::new()),
        None => check,
    }
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
/// request parts it provides carry the one `ferrohealth_server::console`
/// minted, which replaces it.
fn provide_request_nonce() {
    let Some(parts) = leptos::context::use_context::<http::request::Parts>() else {
        return;
    };
    if let Some(nonce) = parts.extensions.get::<CspNonce>() {
        leptos::context::provide_context(Nonce::from_value(nonce.as_str()));
    }
}

/// Serves `router` on `listener` until `shutdown` resolves, then drains as
/// `ferrohealth-server` does: `lifecycle` turns draining, and the requests in
/// flight get the family's default shutdown bound.
///
/// # Errors
/// Returns the I/O error of a listener that fails.
pub async fn serve(
    listener: tokio::net::TcpListener,
    router: axum::Router,
    lifecycle: Lifecycle,
    shutdown: impl Future<Output = ()> + Send + 'static,
) -> std::io::Result<Drained> {
    let section = ServerSection::default();
    ferrohealth_server::serve::serve(
        listener,
        router,
        lifecycle,
        shutdown,
        section.drain_delay(),
        section.shutdown_timeout(),
    )
    .await
}
