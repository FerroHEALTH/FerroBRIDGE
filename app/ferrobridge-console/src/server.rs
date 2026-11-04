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
//! only from the console's own pages (`ferrohealth_server::same_origin`).
//! The sign-in, callback, sign-out and session routes, and the middleware
//! that refreshes a session's access token before the request it arrives
//! with is served and hands the request its `Held`, are
//! `ferrohealth-session`'s, which draws a stopped sign-in through
//! [`crate::stopped::SignInPages`]. The health routes are the family's. No
//! specification governs the console's HTTP surface: our own design.

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
use ferrohealth_session::cookies::{CookieNames, HOST_PREFIX};
use ferrohealth_session::record::{Credential, Held};
use ferrohealth_session::routes::{SignIn, keep_session};
use ferrohealth_session::sessions::Sessions;
use ferrohealth_smart::oidc::RelyingParty;
use http::header::CONTENT_TYPE;
use http::{HeaderValue, Method, StatusCode};

use crate::bridge::{AccessToken, Bridge, BridgeError};
use crate::config::settings::Settings;
use crate::locale::messages::Msg;
use crate::locale::{catalog, tr_in};
use crate::stopped::SignInPages;
use crate::visit::Visit;

/// The path of the console's summary health route.
pub const HEALTH: &str = ferrohealth_server::health::SUMMARY;

/// The largest form body a page takes: a draft, a run's input.
pub const POST_MAX: usize = 2 * 1024 * 1024;

/// The base name of every cookie the console sets.
///
/// `ferrobridge_console_session` carries the session,
/// `ferrobridge_console_sign_in` a pending sign-in, and
/// [`PREFERENCE_PREFIX`] and a preference's name each preference.
pub const COOKIES: &str = "ferrobridge_console";

/// What the name of every preference cookie starts with, before the
/// preference's own name and under the `__Host-` prefix when the cookie is
/// `Secure`, such as `ferrobridge_console_theme`.
pub const PREFERENCE_PREFIX: &str = "ferrobridge_console_";

/// What every request handler shares.
#[derive(Debug, Clone)]
pub struct ConsoleState {
    settings: Arc<Settings>,
    sign_in: SignIn<SignInPages>,
    bridge: Bridge,
    health: Health,
}

/// Why the console's state could not be built.
#[derive(Debug, thiserror::Error)]
pub enum StateError {
    /// A client of the bridge could not be built.
    #[error(transparent)]
    Client(#[from] BridgeError),
    /// The relying party at the OpenID Provider could not be built: its
    /// client, its discovery, or endpoints it cannot use.
    #[error("the OpenID Provider cannot be used")]
    Provider {
        /// What the relying party refused.
        #[source]
        source: Box<ferrohealth_session::config::ConfigError>,
    },
}

impl ConsoleState {
    /// The state for `settings`, with an empty session store; the relying
    /// party discovers the provider's endpoints first when the configuration
    /// does not name them.
    ///
    /// # Errors
    /// Returns [`StateError::Client`] when the bridge client cannot be built,
    /// and [`StateError::Provider`] for a relying party that cannot be built
    /// or a provider that cannot be read.
    pub async fn new(settings: Settings) -> Result<Self, StateError> {
        let bridge = Bridge::new(&settings.bridge)?;
        let party =
            match &settings.oidc {
                Some(oidc) => Some(oidc.checked.relying_party().await.map_err(|source| {
                    StateError::Provider {
                        source: Box::new(source),
                    }
                })?),
                None => None,
            };
        let sessions = Sessions::new(settings.session.clone(), CookieNames::for_console(COOKIES));
        let sign_in = SignIn::new(sessions, party, SignInPages::new(settings.locale.clone()));
        // The settings are resolved and the clients built, so the console
        // serves from here.
        let lifecycle = Lifecycle::default();
        lifecycle.booted();
        Ok(Self {
            settings: Arc::new(settings),
            sign_in,
            bridge,
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

    /// The sign-in: the session store, the relying party and the pages.
    #[must_use]
    pub fn sign_in(&self) -> &SignIn<SignInPages> {
        &self.sign_in
    }

    /// The session store.
    #[must_use]
    pub fn sessions(&self) -> &Sessions {
        self.sign_in.sessions()
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
        self.sign_in.party()
    }

    /// The name `base` takes on this console: under the `__Host-` prefix
    /// when the cookies are `Secure`, and `base` alone when they are not.
    #[must_use]
    pub fn cookie_name(&self, base: &str) -> String {
        if self.settings.session.secure_cookie() {
            format!("{HOST_PREFIX}{base}")
        } else {
            base.to_owned()
        }
    }

    /// The name of the cookie that carries the session on this console.
    #[must_use]
    pub fn session_cookie(&self) -> String {
        self.cookie_name(&CookieNames::for_console(COOKIES).session())
    }
}

/// Builds the console's whole HTTP service over `state`.
pub fn router(state: ConsoleState) -> axum::Router {
    let form_action = form_action(&state);
    let health = ferrohealth_server::health::routes(state.health().clone());
    let same_origin = same_origin_check(state.settings());
    let pages = axum::Router::new()
        .merge(health)
        .nest(
            ferrohealth_ui::serve::PREFIX,
            ferrohealth_ui::serve::axum_router::router(),
        )
        .route(
            crate::app::MARK_LIGHT,
            get(|| async { svg(MARK_LIGHT_SVG) }),
        )
        .route(crate::app::MARK_DARK, get(|| async { svg(MARK_DARK_SVG) }))
        .route(crate::app::FAVICON, get(favicon))
        .route(crate::pages::about::INFO_JSON, get(info_json))
        .route(
            crate::pages::mappings::archive::EXPORT,
            get(crate::pages::mappings::archive::export),
        )
        .route(
            crate::pages::search::ANSWER,
            get(crate::pages::search::answer_route),
        )
        .route(crate::app::PREFERENCE, post(crate::prefs::choose))
        .route(crate::app::RESET, post(crate::prefs::reset))
        .route(crate::app::SIDEBAR, post(crate::prefs::rail))
        .fallback(page)
        .layer(axum::middleware::from_fn_with_state(
            state.sign_in().clone(),
            keep_session::<SignInPages>,
        ))
        .merge(ferrohealth_session::routes::router(state.sign_in().clone()))
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
            let content_type = parts
                .headers
                .get(CONTENT_TYPE)
                .and_then(|value| value.to_str().ok());
            match crate::routes::posted(&visit, &bytes, content_type).await {
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
pub fn form_action(state: &ConsoleState) -> String {
    let end_session = state
        .relying_party()
        .and_then(|party| party.provider().end_session_endpoint())
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

/// The access token of the session the request holds, for a route outside
/// the pages that reads the bridge, such as a download; `None` without a
/// live session.
pub(crate) fn session_token(held: &Held) -> Option<AccessToken> {
    match held {
        Held::SignedIn(session) => match session.credential() {
            Credential::Bearer(token) => Some(AccessToken::new(token.to_secret_string())),
            // NOTE: no specification governs this: our own design; the console signs in through
            // its OpenID Provider alone, so a session carries a bearer token or no credential.
            Credential::Basic { .. } => None,
        },
        Held::TokenExpired | Held::Ended | Held::Absent => None,
    }
}

/// Serves the bridge's `GET /health/info` document as a download, for a
/// signed-in operator: the About page's self-description.
async fn info_json(
    Extension(state): Extension<ConsoleState>,
    Extension(held): Extension<Held>,
) -> Response {
    let Some(token) = session_token(&held) else {
        return StatusCode::UNAUTHORIZED.into_response();
    };
    match state.bridge().health_info(&token).await {
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
