// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The page tree the server draws and the browser hydrates whole.
//!
//! The server draws the first page and the browser takes the whole tree
//! over, then draws every page after it, each group of pages a lazy route
//! ([`crate::routes`]), so no link or form reloads the document (FerroUI
//! `docs/consoles.md`, "How a console builds"). The document carries the operator's theme,
//! density, motion and hints from their cookies and the request's locale, so
//! a page arrives right before any script runs. A signed-in operator reads
//! the shell around the routes, a visitor the sign-in, and a request whose
//! session store cannot be read the fault page.

use ferrohealth_ui::i18n::provide_locale;
use ferrohealth_ui::island::{FormNavigator, PageAnnouncer, ToastRegion};
use ferrohealth_ui::style::Brand;
use ferrohealth_ui::widget::toast::{Flash, RegionWords};
use leptos::prelude::*;
#[cfg(not(target_arch = "wasm32"))]
use leptos_meta::MetaTags;
use leptos_meta::{Title, provide_meta_context};
use leptos_router::components::{Route, Router, Routes};
use leptos_router::{Lazy, SsrMode, path};

use crate::locale::messages::Msg;
use crate::locale::tr;
use crate::model::SessionState;
use crate::routes::{LBridge, LConsole, LMappings, LOverview, LWorkbench};
use crate::visit::{FollowVisit, Visits};

/// The product name as an operator reads it.
pub const PRODUCT: &str = "FerroBRIDGE Console";

/// The console's version, which the footer and the About page name.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// The path the server serves the `FerroHEALTH` stylesheet at, beside the
/// site bundle.
pub const STYLESHEET: &str = "/pkg/ferrohealth-ui.css";

/// The path the server serves the FerroBRIDGE theme at, which loads after
/// the stylesheet.
pub const THEME: &str = "/pkg/ferrobridge-theme.css";

/// The path of the brand mark the lockup draws in the light theme.
pub const MARK_LIGHT: &str = "/pkg/ferrobridge-icon.svg";

/// The path of the brand mark the lockup draws in the dark theme.
pub const MARK_DARK: &str = "/pkg/ferrobridge-icon-dark.svg";

/// The path of the sign-in route the server serves.
pub const SIGN_IN: &str = "/login";

/// The path of the sign-out route the server serves.
pub const SIGN_OUT: &str = "/logout";

/// The page a signed-out operator lands on.
pub const SIGNED_OUT: &str = "/signed-out";

/// The dashboard.
pub const DASHBOARD: &str = "/";

/// The mapping set.
pub const MAPPINGS: &str = "/mappings";

/// The path a mapping file's page takes after it.
pub const FILES: &str = "/mappings/files";

/// The path a template's page takes after it.
pub const TEMPLATES: &str = "/mappings/templates";

/// The path the editor takes a file's path after.
pub const EDIT: &str = "/mappings/edit";

/// The editor of a new file.
pub const NEW_FILE: &str = "/mappings/new";

/// The playground.
pub const PLAYGROUND: &str = "/playground";

/// The FHIR facade.
pub const FACADE: &str = "/facade";

/// The HL7 v2 face.
pub const HL7V2: &str = "/hl7v2";

/// The OMOP ETL.
pub const OMOP: &str = "/omop";

/// The connections.
pub const CONNECTIONS: &str = "/connections";

/// The bridge's running configuration.
pub const CONFIGURATION: &str = "/configuration";

/// Your preferences.
pub const PREFERENCES: &str = "/preferences";

/// The route every preference posts to, its value under its cookie's name.
pub const PREFERENCE: &str = "/preference";

/// The route Reset to defaults posts to.
pub const RESET: &str = "/preferences/reset";

/// The route the sidebar's Collapse and Expand post to.
pub const SIDEBAR: &str = "/sidebar";

/// The About page, which every page's footer links to.
pub const ABOUT: &str = "/about";

/// The brand whose theme the console draws in.
pub const BRAND: Brand = Brand::Bridge;

/// The toasts a form post's answer shows as the page arrives, which the
/// server hands the region (FerroUI R17).
#[derive(Debug, Clone, Default)]
pub struct Flashes(pub Vec<Flash>);

/// The whole HTML document the server renders around [`App`], in the
/// preferences the request's cookies name and its locale, so the page
/// arrives right before any script runs.
#[cfg(not(target_arch = "wasm32"))]
#[must_use]
pub fn document(options: LeptosOptions) -> impl IntoView {
    let visit = crate::visit::Visit::requested();
    let chosen = visit.chosen().preferences;
    let lang = visit.locale().tag();
    view! {
        <!DOCTYPE html>
        <html
            lang=lang
            data-theme=chosen.theme.key()
            data-density=chosen.density.key()
            data-motion=chosen.motion.key()
            data-hints=chosen.hints.key()
        >
            <head>
                <meta charset="utf-8" />
                <meta name="viewport" content="width=device-width, initial-scale=1" />
                <link rel="icon" href="/favicon.ico" sizes="any" />
                <link rel="stylesheet" href=STYLESHEET />
                <link rel="stylesheet" href=THEME />
                <HydrationScripts options />
                <MetaTags />
            </head>
            <body>
                <App />
            </body>
        </html>
    }
}

/// The root component: the title format, the routes in the layout the
/// visit's session calls for, the toast region and the page announcer.
///
/// The server draws it and the browser hydrates it whole, both from the
/// same [`crate::visit::Visit`], which the browser asks for again after
/// each navigation.
#[component]
pub fn App() -> impl IntoView {
    provide_meta_context();
    // NOTE: the Leptos book, `ssr/22_life_cycle`: what the server read reaches the
    // hydrating page serialized, so both halves draw the same page.
    let visits = RwSignal::new((*SharedValue::new(crate::visit::server_visit)).clone());
    provide_context(Visits(visits));
    provide_context(crate::load::Changes(RwSignal::new(0)));
    provide_locale(visits.with_untracked(crate::visit::Visit::locale));
    ferrohealth_ui::widget::prefetch::routes(crate::routes::ROUTES);
    ferrohealth_ui::widget::hydration::mark();
    let session = Memo::new(move |_| visits.with(|visit| visit.session));
    let flash = (*SharedValue::new(|| use_context::<Flashes>().unwrap_or_default().0)).clone();
    let region = move || {
        (session.get() == SessionState::SignedIn).then(|| {
            view! { <ToastRegion words=RegionWords::of_page() flash=flash.clone() /> }
        })
    };
    view! {
        <Title formatter=|text: String| title(&text) text=String::new() />
        <Router>
            <FollowVisit />
            <FormNavigator />
            {move || layout(session.get())}
        </Router>
        {region}
        <PageAnnouncer />
    }
}

/// The document title of a page titled `text`: the page, then the product.
fn title(text: &str) -> String {
    if text.is_empty() {
        PRODUCT.to_owned()
    } else {
        format!("{text} · {PRODUCT}")
    }
}

/// The page for a request whose session is `session`.
fn layout(session: SessionState) -> AnyView {
    // NOTE: leptos_axum, `generate_route_list`: the server lists its routes by
    // rendering the app with no request, so every layout draws the same routes.
    let routes = routes(session);
    match session {
        SessionState::SignedIn => crate::shell::shell(routes),
        SessionState::SignedOut | SessionState::Unreadable => routes,
    }
}

/// The page `page` for the visit's session: the page for a signed-in
/// operator, the sign-in for a visitor, and the fault page when the console
/// cannot read its session store.
pub fn guarded<V>(page: fn() -> V) -> AnyView
where
    V: IntoView + 'static,
{
    match crate::visit::visit_untracked().session {
        SessionState::SignedIn => page().into_any(),
        SessionState::SignedOut => crate::pages::sign_in::sign_in_page().into_any(),
        SessionState::Unreadable => crate::pages::errors::unreadable().into_any(),
    }
}

/// The routes, for a request whose session is `session`.
fn routes(session: SessionState) -> AnyView {
    // NOTE: the Leptos book, `ssr/23_ssr_modes`: a page the bridge fills renders
    // `Async`, whole in one sweep, so no content waits on a script.
    let fallback = move || match session {
        SessionState::SignedIn => crate::pages::errors::not_found().into_any(),
        SessionState::SignedOut => crate::pages::sign_in::sign_in_page().into_any(),
        SessionState::Unreadable => crate::pages::errors::unreadable().into_any(),
    };
    view! {
        <Routes fallback>
            <Route path=path!("/") view={Lazy::<LOverview>::new()} ssr=SsrMode::Async />
            <Route path=path!("/search") view={Lazy::<LOverview>::new()} ssr=SsrMode::Async />
            <Route path=path!("/mappings") view={Lazy::<LMappings>::new()} ssr=SsrMode::Async />
            <Route path=path!("/mappings/new") view={Lazy::<LWorkbench>::new()} ssr=SsrMode::Async />
            <Route path=path!("/mappings/:tab") view={Lazy::<LMappings>::new()} ssr=SsrMode::Async />
            <Route
                path=path!("/mappings/files/*path")
                view={Lazy::<LMappings>::new()}
                ssr=SsrMode::Async
            />
            <Route
                path=path!("/mappings/templates/:id")
                view={Lazy::<LMappings>::new()}
                ssr=SsrMode::Async
            />
            <Route
                path=path!("/mappings/edit/*path")
                view={Lazy::<LWorkbench>::new()}
                ssr=SsrMode::Async
            />
            <Route path=path!("/playground") view={Lazy::<LWorkbench>::new()} ssr=SsrMode::Async />
            <Route
                path=path!("/playground/:tab")
                view={Lazy::<LWorkbench>::new()}
                ssr=SsrMode::Async
            />
            <Route path=path!("/facade") view={Lazy::<LBridge>::new()} ssr=SsrMode::Async />
            <Route path=path!("/facade/:tab") view={Lazy::<LBridge>::new()} ssr=SsrMode::Async />
            <Route path=path!("/hl7v2") view={Lazy::<LBridge>::new()} ssr=SsrMode::Async />
            <Route path=path!("/hl7v2/:tab") view={Lazy::<LBridge>::new()} ssr=SsrMode::Async />
            <Route path=path!("/omop") view={Lazy::<LBridge>::new()} ssr=SsrMode::Async />
            <Route path=path!("/omop/:tab") view={Lazy::<LBridge>::new()} ssr=SsrMode::Async />
            <Route path=path!("/connections") view={Lazy::<LBridge>::new()} ssr=SsrMode::Async />
            <Route
                path=path!("/connections/:name")
                view={Lazy::<LBridge>::new()}
                ssr=SsrMode::Async
            />
            <Route
                path=path!("/connections/:name/:tab")
                view={Lazy::<LBridge>::new()}
                ssr=SsrMode::Async
            />
            <Route path=path!("/configuration") view={Lazy::<LConsole>::new()} ssr=SsrMode::Async />
            <Route
                path=path!("/configuration/:tab")
                view={Lazy::<LConsole>::new()}
                ssr=SsrMode::Async
            />
            <Route path=path!("/preferences") view={Lazy::<LConsole>::new()} ssr=SsrMode::Async />
            <Route
                path=path!("/preferences/:section")
                view={Lazy::<LConsole>::new()}
                ssr=SsrMode::Async
            />
            <Route path=path!("/about") view={Lazy::<LConsole>::new()} ssr=SsrMode::Async />
            <Route
                path=path!("/signed-out")
                view=move || crate::pages::sign_in::signed_out_page().into_any()
                ssr=SsrMode::Async
            />
        </Routes>
    }
    .into_any()
}

/// The page's title in the document, `text` before the product.
#[must_use]
pub fn page_title(text: String) -> impl IntoView {
    view! { <Title text /> }
}

/// The word an unnamed operator is shown as.
#[must_use]
pub fn unnamed_operator() -> String {
    tr(Msg::Operator)
}
