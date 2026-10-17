// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The page tree the server renders and the browser hydrates.
//!
//! It holds the document in the operator's theme, the shell a signed-in
//! operator reads, the centred pages a visitor reads, the routes, and the
//! pages for a path the console does not serve and for a session store it
//! cannot read.

use leptos::either::Either;
use leptos::prelude::*;
use leptos_meta::{Stylesheet, Title, provide_meta_context};
use leptos_router::components::{Route, Router, Routes};
use leptos_router::{SsrMode, path};

use crate::model::SessionState;
use crate::shell::Bar;
use crate::styles;
use crate::theme::Theme;

/// The product name as an operator reads it.
pub const PRODUCT: &str = "FerroBRIDGE Console";

/// The console's version, which the footer and the About page name.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Who makes the console, which the footer and the About page name.
pub const MANUFACTURER: &str = "Cadasto B.V.";

/// The path of the stylesheet cargo-leptos writes into the site bundle.
pub const STYLESHEET: &str = "/pkg/ferrobridge-viewer.css";

/// The path of the sign-in route the server serves.
pub const SIGN_IN: &str = "/login";

/// The path of the sign-out route the server serves.
pub const SIGN_OUT: &str = "/logout";

/// The path of the About page, which every page's footer links to.
pub const ABOUT: &str = "/about";

/// The word the page shows for an operator the provider gave no name.
const OPERATOR: &str = "Operator";

/// The whole HTML document the server renders around [`App`], in the theme
/// the request's cookie names, so the page arrives in it before any script
/// runs.
#[cfg(not(target_arch = "wasm32"))]
#[must_use]
pub fn shell(options: LeptosOptions) -> impl IntoView {
    let theme = crate::theme::server::requested().key();
    view! {
        <!DOCTYPE html>
        <html lang="en" data-theme=theme>
            <head>
                <meta charset="utf-8" />
                <meta name="viewport" content="width=device-width, initial-scale=1" />
                <AutoReload options=options.clone() />
                <HydrationScripts options />
                <leptos_meta::MetaTags />
            </head>
            <body>
                <App />
            </body>
        </html>
    }
}

/// The root component: the stylesheet, the document title, and the shell
/// the request's session is drawn in.
#[component]
pub fn App() -> impl IntoView {
    provide_meta_context();
    // NOTE: the Leptos book, `ssr/22_life_cycle`: a value the server decides reaches
    // the hydrating page serialized, so both halves draw the same page.
    let session = *SharedValue::new_str(session_state);
    let operator = SharedValue::new_str(operator_name).into_inner();
    let theme = *SharedValue::new_str(theme_requested);
    let bridge = SharedValue::new_str(bridge_base).into_inner();
    let bar = (session == SessionState::SignedIn).then(|| Bar {
        theme,
        bridge: Resource::new_blocking(|| (), |()| crate::status::bridge_status()),
    });
    view! {
        <Stylesheet id="leptos" href=STYLESHEET />
        <Title text=PRODUCT />
        <Router>{layout(session, operator, bar, bridge)}</Router>
    }
}

/// The page for a request whose session is `session`: the shell of a
/// signed-in operator with its `bar`, or the page alone for a visitor and
/// for a session store the console cannot read.
fn layout(session: SessionState, operator: String, bar: Option<Bar>, bridge: String) -> AnyView {
    // NOTE: leptos_axum, `generate_route_list`: the server lists its routes by
    // rendering the app with no request, so every layout draws the same routes.
    let routes = routes(session, bridge);
    match bar {
        Some(bar) => signed_in(operator, bar, routes).into_any(),
        None => visitor(routes).into_any(),
    }
}

/// The shell of a signed-in operator: the top bar, the navigation beside
/// the page, and the footer.
fn signed_in(operator: String, bar: Bar, routes: AnyView) -> impl IntoView {
    view! {
        <div class=styles::PAGE>
            <a href="#main" class=styles::SKIP>
                "Skip to the page"
            </a>
            {crate::shell::topbar(operator, bar)}
            <div class=styles::BODY>
                {crate::shell::sidebar()} <main id="main" class=styles::MAIN>
                    <div class=styles::COLUMN>{routes}</div>
                </main>
            </div>
            {crate::shell::footer()}
        </div>
    }
}

/// The shell of a visitor: the page alone, and the footer.
fn visitor(page: AnyView) -> impl IntoView {
    view! {
        <div class=styles::PAGE>
            <main id="main" class="flex flex-1 flex-col px-4 py-8">
                {page}
            </main>
            {crate::shell::footer()}
        </div>
    }
}

/// The page `page` for a request whose session is `session`: the page for a
/// signed-in operator, the sign-in for a visitor, and the fault page when the
/// console cannot read its session store.
fn guarded<V>(session: SessionState, page: fn() -> V) -> impl Fn() -> AnyView + Clone + Send + Sync
where
    V: IntoView + 'static,
{
    move || match session {
        SessionState::SignedIn => page().into_any(),
        SessionState::SignedOut => view! { <SignInPage /> }.into_any(),
        SessionState::Unreadable => unreadable().into_any(),
    }
}

/// The routes, for a request whose session is `session`; `bridge` is the
/// bridge's base URL the About page shows a signed-in operator.
fn routes(session: SessionState, bridge: String) -> AnyView {
    // NOTE: the Leptos book, `ssr/23_ssr_modes`: a page the bridge fills renders
    // `Async`, whole in one sweep, so no content waits on a script.
    view! {
        <Routes fallback=move || {
            if session == SessionState::Unreadable {
                Either::Left(unreadable())
            } else {
                Either::Right(view! { <NotFound /> })
            }
        }>
            <Route
                path=path!("/")
                view=guarded(session, crate::engine::DashboardPage)
                ssr=SsrMode::Async
            />
            <Route
                path=path!("/capability")
                view=guarded(session, crate::engine::CapabilityPage)
                ssr=SsrMode::Async
            />
            <Route
                path=path!("/mappings")
                view=guarded(session, crate::engine::MappingsPage)
                ssr=SsrMode::Async
            />
            <Route
                path=path!("/playground")
                view=guarded(session, crate::tools::PlaygroundPage)
                ssr=SsrMode::Async
            />
            <Route
                path=path!("/validate")
                view=guarded(session, crate::tools::ValidatePage)
                ssr=SsrMode::Async
            />
            <Route
                path=path!("/files")
                view=guarded(session, crate::engine::FilesPage)
                ssr=SsrMode::Async
            />
            <Route
                path=path!("/files/*path")
                view=guarded(session, crate::engine::FilePage)
                ssr=SsrMode::Async
            />
            <Route
                path=path!("/about")
                view=move || {
                    view! { <crate::about::AboutPage bridge=bridge.clone() /> }
                }
                ssr=SsrMode::Async
            />
        </Routes>
    }
    .into_any()
}

/// The landing page of a visitor: the sign-in, a plain link to the server's
/// route, which the client router leaves alone.
#[component]
fn SignInPage() -> impl IntoView {
    view! {
        <Title text=PRODUCT />
        <div class=styles::CENTRED>
            <div class=styles::CARD>
                <span class=styles::MARK aria-hidden="true"></span>
                <h1>{PRODUCT}</h1>
                <p class="text-muted">
                    "The operator console of the FerroBRIDGE bridge. Sign in to continue."
                </p>
                <a href=SIGN_IN rel="external" class=styles::BUTTON_PRIMARY>
                    "Sign in"
                </a>
            </div>
        </div>
    }
}
/// The page a visitor reads when the console cannot read its session store:
/// it cannot tell whether they are signed in, so it offers neither the
/// sign-in nor the operator's pages, and answers `500`.
fn unreadable() -> impl IntoView {
    #[cfg(not(target_arch = "wasm32"))]
    if let Some(response) = use_context::<leptos_axum::ResponseOptions>() {
        response.set_status(http::StatusCode::INTERNAL_SERVER_ERROR);
    }
    view! {
        <Title text="Fault" />
        <div class=styles::CENTRED>
            <div class=styles::CARD>
                <h1>"The console cannot read its sessions"</h1>
                <p role="alert">
                    "500: the console cannot read its session store, so it cannot tell whether you "
                    "are signed in. Ask the console's operator to restart it."
                </p>
            </div>
        </div>
    }
}

/// The page for a path the console does not serve: `404`, and the way back.
#[component]
fn NotFound() -> impl IntoView {
    #[cfg(not(target_arch = "wasm32"))]
    if let Some(response) = use_context::<leptos_axum::ResponseOptions>() {
        response.set_status(http::StatusCode::NOT_FOUND);
    }
    view! {
        <Title text="Not found" />
        <div class=styles::CENTRED>
            <div class=styles::CARD>
                <h1>"Not found"</h1>
                <p class="text-muted">"404: the console has no page at this address."</p>
                <a href="/" class=styles::BUTTON_SECONDARY>
                    "Back to the console"
                </a>
            </div>
        </div>
    }
}

/// The state of the request's session, which the page is drawn for.
fn session_state() -> SessionState {
    #[cfg(not(target_arch = "wasm32"))]
    {
        crate::session::request::state()
    }
    // NOTE: the browser reads the value the server decided; this runs only if
    // that value is missing, and then draws the page a visitor sees.
    #[cfg(target_arch = "wasm32")]
    {
        SessionState::SignedOut
    }
}

/// The name the page shows for a signed-in operator: the provider's name for
/// them, or [`OPERATOR`] when it gave none.
fn operator_name() -> String {
    #[cfg(not(target_arch = "wasm32"))]
    {
        crate::session::request::operator().unwrap_or_else(|| String::from(OPERATOR))
    }
    // NOTE: the browser reads the value the server decided; this runs only if
    // that value is missing, and then shows the word a nameless sign-in shows.
    #[cfg(target_arch = "wasm32")]
    {
        String::from(OPERATOR)
    }
}

/// The theme the request's cookie names.
fn theme_requested() -> Theme {
    #[cfg(not(target_arch = "wasm32"))]
    {
        crate::theme::server::requested()
    }
    // NOTE: the browser reads the value the server decided; this runs only if
    // that value is missing, and then marks the theme that follows the system.
    #[cfg(target_arch = "wasm32")]
    {
        Theme::default()
    }
}

/// The bridge's base URL, which the About page shows a signed-in operator,
/// and empty for a visitor.
fn bridge_base() -> String {
    #[cfg(not(target_arch = "wasm32"))]
    {
        let signed_in = session_state() == SessionState::SignedIn;
        use_context::<crate::server::ViewerState>()
            .filter(|_state| signed_in)
            .map(|state| state.settings().bridge.base.to_string())
            .unwrap_or_default()
    }
    // NOTE: the browser reads the value the server decided; this runs only if
    // that value is missing, and then shows no bridge.
    #[cfg(target_arch = "wasm32")]
    {
        String::new()
    }
}

#[cfg(test)]
mod tests {
    use crate::model::SessionState;

    #[test]
    fn the_state_the_page_carries_reads_back_as_itself() {
        for state in [
            SessionState::SignedIn,
            SessionState::SignedOut,
            SessionState::Unreadable,
        ] {
            assert_eq!(Ok(state), state.to_string().parse());
        }
        assert!("signed in".parse::<SessionState>().is_err());
    }
}
