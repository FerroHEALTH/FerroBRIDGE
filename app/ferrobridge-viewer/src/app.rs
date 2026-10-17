// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The page tree the server renders and the browser hydrates.
//!
//! It holds the document the server writes around the app, the routes, the
//! landing page in the three states of the request's session, and the page
//! for a path the console does not serve.

use leptos::either::{Either, EitherOf3};
use leptos::prelude::*;
use leptos_meta::{Stylesheet, Title, provide_meta_context};
use leptos_router::components::{Route, Router, Routes};
use leptos_router::{SsrMode, path};

use crate::model::{BridgeView, Outcome, Refusal, SessionState, ViewError};

/// The product name as an operator reads it.
pub const PRODUCT: &str = "FerroBRIDGE Console";

/// The path of the stylesheet cargo-leptos writes into the site bundle.
pub const STYLESHEET: &str = "/pkg/ferrobridge-viewer.css";

/// The path of the sign-in route the server serves.
pub const SIGN_IN: &str = "/login";

/// The path of the sign-out route the server serves.
pub const SIGN_OUT: &str = "/logout";

/// The word the page shows for an operator the provider gave no name.
const OPERATOR: &str = "Operator";

/// The whole HTML document the server renders around [`App`].
#[cfg(not(target_arch = "wasm32"))]
#[must_use]
pub fn shell(options: LeptosOptions) -> impl IntoView {
    view! {
        <!DOCTYPE html>
        <html lang="en">
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

/// The root component: the stylesheet, the document title and the routes,
/// drawn for the state of the request's session.
#[component]
pub fn App() -> impl IntoView {
    provide_meta_context();
    // NOTE: the Leptos book, `ssr/22_life_cycle`: a value the server decides reaches
    // the hydrating page serialized, so both halves draw the same page.
    let session = *SharedValue::new_str(session_state);
    let operator = SharedValue::new_str(operator_name).into_inner();
    view! {
        <Stylesheet id="leptos" href=STYLESHEET />
        <Title text=PRODUCT />
        <Router>
            <main id="main">{routes(session, operator)}</main>
        </Router>
    }
}

/// The routes, for a request whose session is `session` and whose operator
/// is named `operator`.
fn routes(session: SessionState, operator: String) -> AnyView {
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
                view=move || match session {
                    SessionState::SignedIn => {
                        EitherOf3::A(view! { <SignedInHome operator=operator.clone() /> })
                    }
                    SessionState::SignedOut => EitherOf3::B(view! { <SignInPage /> }),
                    SessionState::Unreadable => EitherOf3::C(unreadable()),
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
        <h1>{PRODUCT}</h1>
        <p>"The operator console of the FerroBRIDGE bridge. Sign in to continue."</p>
        <a href=SIGN_IN rel="external">
            "Sign in"
        </a>
    }
}

/// The landing page of a signed-in operator: who they are, what the bridge
/// says of itself, and the sign-out.
#[component]
fn SignedInHome(
    /// The operator's name, as the provider gave it.
    operator: String,
) -> impl IntoView {
    let status = Resource::new_blocking(|| (), |()| crate::status::bridge_status());
    let section = view! {
        <section aria-labelledby="bridge-heading">
            <h2 id="bridge-heading">"Bridge"</h2>
            <Suspense fallback=|| {
                view! { <p>"Asking the bridge."</p> }
            }>{move || Suspend::new(async move { bridge_view(status.await) })}</Suspense>
        </section>
    }
    .into_any();
    view! {
        <Title text=PRODUCT />
        <h1>{PRODUCT}</h1>
        <p>"Signed in as " <span>{operator}</span> "."</p>
        {section}
        {sign_out()}
    }
}

/// The sign-out: a `POST` form, the only method the server's sign-out route
/// takes.
fn sign_out() -> impl IntoView {
    view! {
        <form method="post" action=SIGN_OUT>
            <button type="submit">"Sign out"</button>
        </form>
    }
}

/// The bridge section for the answer `answered`: what the bridge says of
/// itself, its refusal with its status, or the console's fault, never
/// nothing.
fn bridge_view(answered: Result<Outcome<BridgeView>, ViewError>) -> AnyView {
    match answered {
        Ok(Outcome::Shown(status)) => {
            let unnamed = || String::from("not stated");
            view! {
                <dl>
                    <dt>"Software"</dt>
                    <dd>{status.software.unwrap_or_else(unnamed)}</dd>
                    <dt>"Version"</dt>
                    <dd>{status.version.unwrap_or_else(unnamed)}</dd>
                    <dt>"FHIR version"</dt>
                    <dd>{status.fhir_version.unwrap_or_else(unnamed)}</dd>
                </dl>
            }
            .into_any()
        }
        Ok(Outcome::Refused(Refusal::NotAuthenticated)) => view! {
            <p role="alert">
                "401: the bridge did not accept your sign-in. "
                <a href=SIGN_IN rel="external">
                    "Sign in again"
                </a>
                "."
            </p>
        }
        .into_any(),
        Ok(Outcome::Refused(Refusal::Bridge {
            status,
            diagnostics,
        })) => view! {
            <p role="alert">{format!("{status}: the bridge refused the request.")}</p>
            <ul>
                {diagnostics.into_iter().map(|text| view! { <li>{text}</li> }).collect_view()}
            </ul>
        }
        .into_any(),
        Err(error) => view! { <p role="alert">{error.to_string()}</p> }.into_any(),
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
        <h1>"The console cannot read its sessions"</h1>
        <p role="alert">
            "500: the console cannot read its session store, so it cannot tell whether you "
            "are signed in. Ask the console's operator to restart it."
        </p>
    }
}

/// The page for a path the console does not serve, answered `404`.
#[component]
fn NotFound() -> impl IntoView {
    #[cfg(not(target_arch = "wasm32"))]
    if let Some(response) = use_context::<leptos_axum::ResponseOptions>() {
        response.set_status(http::StatusCode::NOT_FOUND);
    }
    view! {
        <Title text="Not found" />
        <h1>"Not found"</h1>
        <p>"404: the console has no page at this address."</p>
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
