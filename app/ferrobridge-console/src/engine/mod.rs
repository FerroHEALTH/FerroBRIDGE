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
use crate::locale::{Msg, tr};
use crate::model::ViewError;

#[cfg(not(target_arch = "wasm32"))]
pub mod render;

/// The path of the capability screen.
pub const CAPABILITY: &str = "/capability";

/// The path of the mappings overview.
pub const MAPPINGS: &str = "/mappings";

/// The path of the mapping file list; a file's page is under it.
pub const FILES: &str = "/files";

/// Draws the list of every mapping file with its owner and diagnostics.
///
/// # Errors
/// As [`dashboard`].
#[server(endpoint = "files")]
pub async fn mapping_files() -> Result<String, ViewError> {
    let (state, token) = crate::session::request::signed_in()?;
    let listing = state.bridge().mapping_files(&token).await;
    Ok(render::files::list(&listing))
}

/// Draws the mapping file at the wire path `path`, verbatim with its
/// diagnostics.
///
/// # Errors
/// As [`dashboard`]; a path that is not plain names below a root is drawn
/// as the refusal of input the console does not send.
#[server(endpoint = "file")]
pub async fn mapping_file(
    /// The file's wire path, such as `fhirconnect/models/a.model.yml`.
    path: String,
) -> Result<String, ViewError> {
    let (state, token) = crate::session::request::signed_in()?;
    let bridge = state.bridge();
    let (file, listing) = tokio::join!(
        bridge.mapping_file(&path, &token),
        bridge.mapping_files(&token)
    );
    Ok(render::files::shown(&path, &file, &listing))
}

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
fn screen<F>(title: String, fetch: fn() -> F) -> impl IntoView
where
    F: Future<Output = Result<String, ViewError>> + Send + 'static,
{
    let drawn = Resource::new_blocking(|| (), move |()| fetch());
    drawn_screen(title, drawn)
}

/// The page titled `title` that shows what `drawn` answers.
fn drawn_screen(title: String, drawn: Resource<Result<String, ViewError>>) -> impl IntoView {
    let document = format!("{title} · {PRODUCT}");
    view! {
        <Title text=document />
        {ferrohealth_ui::view::surface::page_header(title, None, ())}
        <Suspense fallback=|| {
            ferrohealth_ui::view::empty::busy(
                tr(Msg::AskingTheBridge),
                ferrohealth_ui::view::empty::Busy::Section,
            )
        }>
            {move || Suspend::new(async move {
                match drawn.await {
                    Ok(html) => view! { <div inner_html=html></div> }.into_any(),
                    Err(error) => {
                        ferrohealth_ui::view::refusal::fault(error.text()).into_any()
                    }
                }
            })}
        </Suspense>
    }
}

/// The mapping file list page.
#[component]
pub fn FilesPage() -> impl IntoView {
    screen(tr(Msg::MappingFiles), mapping_files)
}

/// The page of one mapping file, the wire path after `/files/` naming it.
#[component]
pub fn FilePage() -> impl IntoView {
    let params = leptos_router::hooks::use_params_map();
    // NOTE: no specification governs this: our own design; a missing path is
    // the empty one, which the bridge client refuses before it asks anything.
    let path = move || params.read().get("path").unwrap_or_default();
    let drawn = Resource::new_blocking(path, mapping_file);
    drawn_screen(tr(Msg::MappingFile), drawn)
}

/// The dashboard page.
#[component]
pub fn DashboardPage() -> impl IntoView {
    screen(tr(Msg::Dashboard), dashboard)
}

/// The capability page.
#[component]
pub fn CapabilityPage() -> impl IntoView {
    screen(tr(Msg::Capability), capability)
}

/// The mappings overview page.
#[component]
pub fn MappingsPage() -> impl IntoView {
    screen(tr(Msg::Mappings), mappings)
}
