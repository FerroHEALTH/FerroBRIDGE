// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The engine-state screens: the dashboard, the capability screen and the
//! mappings overview.
//!
//! Each page reads one server function, which checks the session, asks the
//! bridge as the operator, and answers the screen drawn as HTML on the
//! server ([`render`]). A section whose bridge call was refused or failed
//! draws the refusal's status and diagnostics, or the console's fault, in
//! place of its content, so a screen is never empty; only the console's own
//! faults (no live session, no state) fail the function. No specification
//! governs the console: our own design.

use leptos::prelude::*;
use leptos_meta::Title;

use crate::app::PRODUCT;
use crate::model::ViewError;

#[cfg(not(target_arch = "wasm32"))]
pub mod render;

/// The path of the capability screen.
pub const CAPABILITY: &str = "/capability";

/// The path of the mappings overview.
pub const MAPPINGS: &str = "/mappings";

/// Draws the dashboard: the bridge's build facts, pins, lanes and
/// listeners, its readiness and the counts of its running load.
///
/// # Errors
/// Returns [`ViewError::SignedOut`] without a live session and
/// [`ViewError::Unavailable`] when the console cannot read its state.
#[server(endpoint = "dashboard")]
pub async fn dashboard() -> Result<String, ViewError> {
    let (state, token) = crate::session::request::signed_in()?;
    let bridge = state.bridge();
    let (info, readiness, loaded) = tokio::join!(
        bridge.health_info(&token),
        bridge.readiness(&token),
        bridge.operator_state(&token)
    );
    Ok(render::dashboard::render(&info, &readiness, &loaded))
}

/// Draws the capability screen: the `CapabilityStatement` and the SMART
/// discovery document.
///
/// # Errors
/// As [`dashboard`].
#[server(endpoint = "capability")]
pub async fn capability() -> Result<String, ViewError> {
    let (state, token) = crate::session::request::signed_in()?;
    let bridge = state.bridge();
    let (statement, smart) = tokio::join!(
        bridge.capability(&token),
        bridge.smart_configuration(&token)
    );
    Ok(render::capability::render(&statement, &smart))
}

/// Draws the mappings overview from the operator API's running load.
///
/// # Errors
/// As [`dashboard`].
#[server(endpoint = "mappings")]
pub async fn mappings() -> Result<String, ViewError> {
    let (state, token) = crate::session::request::signed_in()?;
    let loaded = state.bridge().operator_state(&token).await;
    Ok(render::mappings::render(&loaded))
}

/// A screen read through `fetch`, titled `title`: the HTML the server drew,
/// or the console's fault.
fn screen<F>(title: &'static str, fetch: fn() -> F) -> impl IntoView
where
    F: Future<Output = Result<String, ViewError>> + Send + 'static,
{
    let drawn = Resource::new_blocking(|| (), move |()| fetch());
    view! {
        <Title text=format!("{title} · {PRODUCT}") />
        <h1>{title}</h1>
        <Suspense fallback=|| {
            view! { <p>"Asking the bridge."</p> }
        }>
            {move || Suspend::new(async move {
                match drawn.await {
                    Ok(html) => view! { <div inner_html=html></div> }.into_any(),
                    Err(error) => view! { <p role="alert">{error.to_string()}</p> }.into_any(),
                }
            })}
        </Suspense>
    }
}

/// The dashboard page.
#[component]
pub fn DashboardPage() -> impl IntoView {
    screen("Dashboard", dashboard)
}

/// The capability page.
#[component]
pub fn CapabilityPage() -> impl IntoView {
    screen("Capability", capability)
}

/// The mappings overview page.
#[component]
pub fn MappingsPage() -> impl IntoView {
    screen("Mappings", mappings)
}
