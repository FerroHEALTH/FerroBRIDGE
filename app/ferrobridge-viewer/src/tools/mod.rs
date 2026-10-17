// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The tools: the transform playground over `$tofhir` and `$toopenehr`, and
//! the `$validate` dry run.
//!
//! What the operator enters travels in a server function's request body
//! alone: never in a URL, a query, the history, browser storage or a log.
//! The forms turn autocomplete off, every answer is `no-store`, and the
//! answer is drawn on the server, escaped, and kept nowhere but the page.
//! Each form is disabled until the page has hydrated, and a plain form post
//! that still arrives is refused before the bridge is asked anything. No
//! specification governs the console: our own design.

use leptos::prelude::*;
use leptos::server_fn::ServerFn as _;
use leptos_meta::Title;

use crate::app::PRODUCT;
use crate::model::{Outcome, ViewError};

#[cfg(not(target_arch = "wasm32"))]
pub mod render;
#[cfg(not(target_arch = "wasm32"))]
pub mod server;

/// The path of the transform playground.
pub const PLAYGROUND: &str = "/playground";

/// The path of the validation dry run.
pub const VALIDATE: &str = "/validate";

/// Runs `$tofhir` over a composition and draws the `Bundle` it answered.
///
/// # Errors
/// Returns [`ViewError::SignedOut`] without a live session and
/// [`ViewError::PlainPost`] for a plain form post.
#[server(endpoint = "tofhir")]
pub async fn run_to_fhir(
    /// The composition, canonical or FLAT JSON.
    composition: String,
    /// The template id, empty for none.
    template_id: String,
    /// The `context.patient` reference, empty for none.
    patient: String,
) -> Result<String, ViewError> {
    server::to_fhir(&composition, &template_id, &patient).await
}

/// Runs `$toopenehr` over a `Bundle` and draws the composition it answered.
///
/// # Errors
/// As [`run_to_fhir`].
#[server(endpoint = "toopenehr")]
pub async fn run_to_openehr(
    /// The `Bundle`, JSON.
    bundle: String,
    /// The template id, empty for none.
    template_id: String,
    /// The composition's format, `canonical` or `flat`.
    format: String,
) -> Result<String, ViewError> {
    server::to_openehr(&bundle, &template_id, &format).await
}

/// Runs `$validate` on a resource type and draws the `OperationOutcome` it
/// answered.
///
/// # Errors
/// As [`run_to_fhir`].
#[server(endpoint = "validate")]
pub async fn run_validate(
    /// The resource type.
    resource_type: String,
    /// The resource, JSON.
    resource: String,
) -> Result<String, ViewError> {
    server::validate(&resource_type, resource).await
}

/// Lists the resource types the bridge declares `$validate` on.
///
/// # Errors
/// Returns [`ViewError::SignedOut`] without a live session.
#[server(endpoint = "validate-types")]
pub async fn validate_types() -> Result<Outcome<Vec<String>>, ViewError> {
    server::validate_types().await
}

/// A signal that turns true once the page has hydrated, so a form's submit
/// stays disabled while only a plain post could send it.
fn hydrated() -> RwSignal<bool> {
    let hydrated = RwSignal::new(false);
    Effect::new(move |_| hydrated.set(true));
    hydrated
}

/// The answer of an action: the HTML the server drew, the console's fault,
/// or nothing while none was asked for.
fn answer(value: Option<Result<String, ViewError>>) -> AnyView {
    match value {
        None => ().into_any(),
        Some(Ok(html)) => view! { <div inner_html=html></div> }.into_any(),
        Some(Err(error)) => view! { <p role="alert">{error.to_string()}</p> }.into_any(),
    }
}

/// A labelled text field bound to `value`.
fn field(id: &'static str, label: &'static str, value: RwSignal<String>) -> impl IntoView {
    view! {
        <label for=id>{label}</label>
        <input
            id=id
            name=id
            type="text"
            autocomplete="off"
            spellcheck="false"
            prop:value=move || value.get()
            on:input:target=move |event| value.set(event.target().value())
        />
    }
}

/// A labelled text area bound to `value`.
fn area(id: &'static str, label: &'static str, value: RwSignal<String>) -> impl IntoView {
    view! {
        <label for=id>{label}</label>
        <textarea
            id=id
            name=id
            rows="14"
            autocomplete="off"
            spellcheck="false"
            prop:value=move || value.get()
            on:input:target=move |event| value.set(event.target().value())
        ></textarea>
    }
}

/// The `$tofhir` form and its answer.
fn to_fhir_form() -> AnyView {
    let run = ServerAction::<RunToFhir>::new();
    let ready = hydrated();
    let composition = RwSignal::new(String::new());
    let template_id = RwSignal::new(String::new());
    let patient = RwSignal::new(String::new());
    let submit = move |event: leptos::ev::SubmitEvent| {
        event.prevent_default();
        run.dispatch(RunToFhir {
            composition: composition.get_untracked(),
            template_id: template_id.get_untracked(),
            patient: patient.get_untracked(),
        });
    };
    view! {
        <section class="card">
            <h2>"openEHR to FHIR: $tofhir"</h2>
            <form method="post" action=RunToFhir::url() autocomplete="off" on:submit=submit>
                {area("tofhir-composition", "Composition, canonical or FLAT JSON", composition)}
                {field("tofhir-template", "Template id (required for FLAT)", template_id)}
                {field("tofhir-patient", "context.patient, a reference such as Patient/123", patient)}
                <button type="submit" disabled=move || !ready.get() || run.pending().get()>
                    "Run $tofhir"
                </button>
            </form>
            <div aria-live="polite">{move || answer(run.value().get())}</div>
        </section>
    }
    .into_any()
}

/// The `$toopenehr` form and its answer.
fn to_openehr_form() -> AnyView {
    let run = ServerAction::<RunToOpenehr>::new();
    let ready = hydrated();
    let bundle = RwSignal::new(String::new());
    let template_id = RwSignal::new(String::new());
    let format = RwSignal::new(String::from("canonical"));
    let submit = move |event: leptos::ev::SubmitEvent| {
        event.prevent_default();
        run.dispatch(RunToOpenehr {
            bundle: bundle.get_untracked(),
            template_id: template_id.get_untracked(),
            format: format.get_untracked(),
        });
    };
    view! {
        <section class="card">
            <h2>"FHIR to openEHR: $toopenehr"</h2>
            <form method="post" action=RunToOpenehr::url() autocomplete="off" on:submit=submit>
                {area("toopenehr-bundle", "Bundle, JSON", bundle)}
                {field("toopenehr-template", "Template id", template_id)}
                <label for="toopenehr-format">"Composition format"</label>
                <select
                    id="toopenehr-format"
                    name="format"
                    on:change:target=move |event| format.set(event.target().value())
                >
                    <option value="canonical" selected=true>
                        "canonical"
                    </option>
                    <option value="flat">"flat"</option>
                </select>
                <button type="submit" disabled=move || !ready.get() || run.pending().get()>
                    "Run $toopenehr"
                </button>
            </form>
            <div aria-live="polite">{move || answer(run.value().get())}</div>
        </section>
    }
    .into_any()
}

/// The transform playground.
#[component]
#[expect(
    clippy::must_use_candidate,
    reason = "the #[component] macro drops a #[must_use] from the function it writes"
)]
pub fn PlaygroundPage() -> impl IntoView {
    let forms = (to_fhir_form(), to_openehr_form());
    view! {
        <Title text=format!("Transform playground · {PRODUCT}") />
        <h1>"Transform playground"</h1>
        <p>
            "Runs a transform against the bridge's running programs. Nothing you enter or the bridge answers is kept."
        </p>
        <div class="sections">{forms.0} {forms.1}</div>
    }
}

/// The `$validate` form over the types `types` and its answer.
fn validate_form(types: Vec<String>) -> AnyView {
    let run = ServerAction::<RunValidate>::new();
    let ready = hydrated();
    let first = types.first().cloned().unwrap_or_default();
    let resource_type = RwSignal::new(first);
    let resource = RwSignal::new(String::new());
    let submit = move |event: leptos::ev::SubmitEvent| {
        event.prevent_default();
        run.dispatch(RunValidate {
            resource_type: resource_type.get_untracked(),
            resource: resource.get_untracked(),
        });
    };
    let options = types
        .into_iter()
        .map(|kind| view! { <option value=kind.clone()>{kind.clone()}</option> })
        .collect_view();
    view! {
        <form method="post" action=RunValidate::url() autocomplete="off" on:submit=submit>
            <label for="validate-type">"Resource type"</label>
            <select
                id="validate-type"
                name="resource_type"
                on:change:target=move |event| resource_type.set(event.target().value())
            >
                {options}
            </select>
            {area("validate-resource", "Resource, JSON", resource)}
            <button type="submit" disabled=move || !ready.get() || run.pending().get()>
                "Run $validate"
            </button>
        </form>
        <div aria-live="polite">{move || answer(run.value().get())}</div>
    }
    .into_any()
}

/// The validation dry run: the types from the `CapabilityStatement`, the
/// form, and the `OperationOutcome`.
#[component]
#[expect(
    clippy::must_use_candidate,
    reason = "the #[component] macro drops a #[must_use] from the function it writes"
)]
pub fn ValidatePage() -> impl IntoView {
    let types = Resource::new_blocking(|| (), |()| validate_types());
    view! {
        <Title text=format!("Validation dry run · {PRODUCT}") />
        <h1>"Validation dry run"</h1>
        <p>
            "Runs $validate, which commits nothing. Nothing you enter or the bridge answers is kept."
        </p>
        <section class="card">
            <Suspense fallback=|| {
                view! { <p>"Asking the bridge."</p> }
            }>
                {move || Suspend::new(async move {
                    match types.await {
                        Ok(Outcome::Shown(types)) if types.is_empty() => {
                            view! {
                                <p role="alert">
                                    "The bridge declares $validate on no resource type."
                                </p>
                            }
                                .into_any()
                        }
                        Ok(Outcome::Shown(types)) => validate_form(types),
                        Ok(Outcome::Refused(refusal)) => {
                            view! { <p role="alert">{refusal.to_string()}</p> }.into_any()
                        }
                        Err(error) => view! { <p role="alert">{error.to_string()}</p> }.into_any(),
                    }
                })}
            </Suspense>
        </section>
    }
}
