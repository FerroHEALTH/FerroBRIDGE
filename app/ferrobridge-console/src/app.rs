// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The page tree the server renders and the browser hydrates.
//!
//! It holds the document in the operator's theme and the request's locale
//! (`crate::locale`), the shell a signed-in
//! operator reads, the centred pages a visitor reads, the routes, and the
//! pages for a path the console does not serve and for a session store it
//! cannot read.

use ferrohealth_ui::i18n::{Locale, provide_locale};
use ferrohealth_ui::icon::Glyph;
use ferrohealth_ui::theme::Theme;
use ferrohealth_ui::view::button::{Variant, label};
use ferrohealth_ui::view::shell::{MAIN_ID, Shell};
use ferrohealth_ui::view::sign_in::{Provider, SignIn, sign_in};
use leptos::either::Either;
use leptos::prelude::*;
use leptos_meta::{Stylesheet, Title, provide_meta_context};
use leptos_router::components::{Route, Router, Routes};
use leptos_router::{SsrMode, path};

use crate::locale::{Msg, tr};
use crate::model::SessionState;
use crate::shell::Bar;

/// The product name as an operator reads it.
pub const PRODUCT: &str = "FerroBRIDGE Console";

/// The console's version, which the footer and the About page name.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Who makes the console, which the footer and the About page name.
pub const MANUFACTURER: &str = "Cadasto B.V.";

/// The path the server serves the `FerroHEALTH` stylesheet at, beside the
/// site bundle.
pub const STYLESHEET: &str = "/pkg/ferrohealth-ui.css";

/// The path the server serves the console's brand theme at, which loads
/// after the stylesheet.
pub const THEME: &str = "/pkg/ferrobridge-theme.css";

/// The path of the sign-in route the server serves.
pub const SIGN_IN: &str = "/login";

/// The path of the sign-out route the server serves.
pub const SIGN_OUT: &str = "/logout";

/// The path of the About page, which every page's footer links to.
pub const ABOUT: &str = "/about";

/// The word the page shows for an operator the provider gave no name.
fn unnamed_operator() -> String {
    tr(Msg::Operator)
}

/// The whole HTML document the server renders around [`App`], in the theme
/// the request's cookie names, so the page arrives in it before any script
/// runs.
#[cfg(not(target_arch = "wasm32"))]
#[must_use]
pub fn shell(options: LeptosOptions) -> impl IntoView {
    let theme = crate::theme::server::requested().key();
    let lang = crate::locale::requested().tag();
    view! {
        <!DOCTYPE html>
        <html lang=lang data-theme=theme>
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
    let tag = SharedValue::new_str(locale_requested).into_inner();
    provide_locale(Locale::parse(&tag).unwrap_or_default());
    let bridge = SharedValue::new_str(bridge_base).into_inner();
    let bar = (session == SessionState::SignedIn).then(|| Bar {
        theme,
        bridge: Resource::new_blocking(|| (), |()| crate::status::bridge_status()),
    });
    view! {
        <Stylesheet id="ferrohealth-ui" href=STYLESHEET />
        <Stylesheet id="theme" href=THEME />
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
fn signed_in(operator: String, bar: Bar, routes: AnyView) -> AnyView {
    ferrohealth_ui::view::shell::shell(Shell {
        topbar: crate::shell::topbar(operator, bar),
        banner: None,
        sidebar: crate::shell::sidebar(),
        content: routes,
        footer: crate::shell::footer_names(),
    })
}

/// The shell of a visitor: the page alone, and the footer.
fn visitor(page: AnyView) -> AnyView {
    let foot = crate::shell::footer().into_any();
    view! {
        <div class="fui-page">
            <main id=MAIN_ID class="fui-main">
                <div class="fui-column">{page}</div>
            </main>
            {foot}
        </div>
    }
    .into_any()
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
                path=path!("/edit")
                view=guarded(session, crate::editor::EditPage)
                ssr=SsrMode::Async
            />
            <Route
                path=path!("/edit/*path")
                view=guarded(session, crate::editor::EditPage)
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
    // NOTE: OpenID Connect Core 1.0 §3.1: the console signs an operator in at
    // their provider alone, so the card offers the provider's button and no form.
    let card = sign_in(SignIn {
        lockup: crate::shell::lockup().into_any(),
        notices: Vec::new(),
        refusal: None,
        basic: None,
        provider: Some(Provider {
            href: String::from(SIGN_IN),
            label: tr(Msg::SignIn),
        }),
        links: Vec::new(),
    });
    view! {
        <Title text=PRODUCT />
        <div class="fui-centred">
            <h1 class="fui-sr-only">{PRODUCT}</h1>
            {card}
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
    let alert = ferrohealth_ui::view::refusal::fault(tr(Msg::UnreadableFault));
    view! {
        <Title text=tr(Msg::UnreadableTitle) />
        <div class="fui-centred">
            <div class="fui-card fui-sign-in">
                <h1>{tr(Msg::UnreadableHeading)}</h1>
                {alert}
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
    let back = view! {
        <a href="/" class=Variant::Primary.class()>
            {label(Some(Glyph::ARROW_LEFT), tr(Msg::NotFoundBack))}
        </a>
    };
    let empty = ferrohealth_ui::view::empty::empty_state(
        Glyph::COMPASS,
        tr(Msg::NotFoundTitle),
        tr(Msg::NotFoundBody),
        back,
    );
    view! {
        <Title text=tr(Msg::NotFoundTitle) />
        <div class="fui-centred">
            <h1 class="fui-sr-only">{tr(Msg::NotFoundTitle)}</h1>
            <div class="fui-card">{empty}</div>
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
/// them, or [`unnamed_operator`] when it gave none.
fn operator_name() -> String {
    #[cfg(not(target_arch = "wasm32"))]
    {
        crate::session::request::operator().unwrap_or_else(unnamed_operator)
    }
    // NOTE: the browser reads the value the server decided; this runs only if
    // that value is missing, and then shows the word a nameless sign-in shows.
    #[cfg(target_arch = "wasm32")]
    {
        unnamed_operator()
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

/// The tag of the locale the request is served in.
fn locale_requested() -> String {
    #[cfg(not(target_arch = "wasm32"))]
    {
        crate::locale::requested().tag()
    }
    // NOTE: the browser reads the value the server decided; this runs only if
    // that value is missing, and then draws the reference locale.
    #[cfg(target_arch = "wasm32")]
    {
        Locale::english().tag()
    }
}

/// The bridge's base URL, which the About page shows a signed-in operator,
/// and empty for a visitor.
fn bridge_base() -> String {
    #[cfg(not(target_arch = "wasm32"))]
    {
        let signed_in = session_state() == SessionState::SignedIn;
        use_context::<crate::server::ConsoleState>()
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
