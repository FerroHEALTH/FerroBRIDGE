// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The console's HTTP service: the health route, the sign-in routes, the
//! pages, the form posts they send and FerroUI's assets, under one set of
//! browser security headers.
//!
//! Every response carries the browser security headers of
//! `ferrohealth-server`'s `console` (`nosniff`, `DENY` framing, no referrer,
//! a same-origin opener, `no-store` on every document) under the policy of
//! a server-rendered console (`ConsoleHeaders::server_rendered`), whose
//! scripts and styles come from the console alone with no nonce. FerroUI's
//! assets keep the year-long cache their router sets, under a path that
//! names the library's version. A request that is not a safe method is taken
//! only from the console's own pages (`ferrohealth_server::same_origin`),
//! and a session's access token is refreshed before the request it arrives
//! with is served. The health routes are the family's. No specification
//! governs the console's HTTP surface: our own design.

use std::sync::Arc;

use axum::Extension;
use axum::extract::Request;
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use ferrohealth_health::lifecycle::Lifecycle;
use ferrohealth_health::registry::Registry;
use ferrohealth_server::console::{self, ConsoleHeaders};
use ferrohealth_server::health::Health;
use ferrohealth_server::same_origin::{self, SameOrigin};
use ferrohealth_server::section::ServerSection;
use ferrohealth_server::serve::Drained;
use http::header::CONTENT_TYPE;
use http::{HeaderValue, Method, StatusCode};

use crate::bridge::{Bridge, BridgeError};
use crate::config::settings::{OidcSettings, Settings};
use crate::locale::messages::Msg;
use crate::locale::{catalog, tr_in};
use crate::session::Sessions;
use crate::visit::Visit;
use ferrohealth_smart::metadata::{Http, ServerMetadata};
use ferrohealth_smart::oidc::{OidcError, Provider, RelyingParty};

/// The path of the console's summary health route.
pub const HEALTH: &str = ferrohealth_server::health::SUMMARY;

/// The largest form body a page takes: a draft, a run's input.
pub const POST_MAX: usize = 2 * 1024 * 1024;

/// What every request handler shares.
#[derive(Debug, Clone)]
pub struct ConsoleState {
    settings: Arc<Settings>,
    sessions: Sessions,
    bridge: Bridge,
    relying_party: Option<RelyingParty>,
    health: Health,
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
        // The settings are resolved and the clients built, so the console
        // serves from here.
        let lifecycle = Lifecycle::default();
        lifecycle.booted();
        Ok(Self {
            sessions: Sessions::new(settings.session),
            settings: Arc::new(settings),
            bridge,
            relying_party,
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

/// Builds the console's whole HTTP service over `state`.
pub fn router(state: ConsoleState) -> axum::Router {
    let form_action = form_action(state.settings());
    let health = ferrohealth_server::health::routes(state.health().clone());
    let same_origin = same_origin_check(state.settings());
    let pages = axum::Router::new()
        .merge(health)
        .nest(
            ferrohealth_ui::serve::PREFIX,
            ferrohealth_ui::serve::axum_router::router(),
        )
        .route(crate::app::MARK_LIGHT, get(mark_light))
        .route(crate::app::MARK_DARK, get(mark_dark))
        .route(crate::app::FAVICON, get(favicon))
        .route(crate::pages::about::INFO_JSON, get(info_json))
        .route(
            crate::pages::search::ANSWER,
            get(crate::pages::search::answer_route),
        )
        .route(crate::app::PREFERENCE, post(crate::prefs::choose))
        .route(crate::app::RESET, post(crate::prefs::reset))
        .route(crate::app::SIDEBAR, post(crate::prefs::rail))
        .route(crate::oidc::LOGIN, get(crate::oidc::login))
        .route(crate::oidc::CALLBACK, get(crate::oidc::callback))
        .route(
            crate::oidc::logout::LOGOUT,
            post(crate::oidc::logout::logout),
        )
        .fallback(page)
        .layer(axum::middleware::from_fn(
            crate::oidc::refresh::refresh_session,
        ))
        .layer(axum::middleware::from_fn(
            move |request: Request, next: Next| {
                same_origin::only(same_origin.clone(), request, next)
            },
        ))
        .layer(Extension(state));
    let headers = ConsoleHeaders::server_rendered(form_action);
    pages.layer(axum::middleware::from_fn(
        move |request: Request, next: Next| console::headers(headers.clone(), request, next),
    ))
}

/// Draws the page the request names: a `GET` reads it, and a `POST` to a
/// page that takes one draws it with the post's answer.
async fn page(Extension(state): Extension<ConsoleState>, request: Request) -> Response {
    let (parts, body) = request.into_parts();
    let visit = Visit::of(&state, &parts);
    match parts.method {
        Method::GET | Method::HEAD => {
            let drawn = crate::routes::draw(&visit).await;
            crate::app::render(&visit, drawn).await
        }
        Method::POST => {
            if visit.session != crate::model::SessionState::SignedIn {
                let drawn = crate::routes::draw(&visit)
                    .await
                    .with_status(StatusCode::UNAUTHORIZED);
                return crate::app::render(&visit, drawn).await;
            }
            let Ok(bytes) = axum::body::to_bytes(body, POST_MAX).await else {
                return StatusCode::PAYLOAD_TOO_LARGE.into_response();
            };
            match crate::routes::posted(&visit, &String::from_utf8_lossy(&bytes)).await {
                Some(answer) => answer,
                None => StatusCode::METHOD_NOT_ALLOWED.into_response(),
            }
        }
        _ => StatusCode::METHOD_NOT_ALLOWED.into_response(),
    }
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

/// The browser's icon the console ships, the brand mark.
const FAVICON_ICO: &[u8] = include_bytes!("../public/favicon.ico");

/// Serves the browser's icon.
async fn favicon() -> Response {
    (
        [(CONTENT_TYPE, HeaderValue::from_static("image/x-icon"))],
        FAVICON_ICO,
    )
        .into_response()
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
