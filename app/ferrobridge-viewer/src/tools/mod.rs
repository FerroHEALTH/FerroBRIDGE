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

use ferrohealth_ui::highlight::Language;
use ferrohealth_ui::icon::Glyph;
use ferrohealth_ui::view::button::{Variant, label};
use ferrohealth_ui::view::empty::{Busy, busy};
use ferrohealth_ui::view::form::{FORM, INPUT, MEDIUM, SELECT};
use ferrohealth_ui::view::refusal::fault;
use ferrohealth_ui::view::surface::{page_header, stack};
use ferrohealth_ui::widget::editor::{Editor, code_editor};
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
        Some(Err(error)) => fault(error.to_string()).into_any(),
    }
}

/// A labelled text field bound to `value`.
fn field(id: &'static str, label: &'static str, value: RwSignal<String>) -> impl IntoView {
    let control = view! {
        <input
            id=id
            name=id
            type="text"
            class=INPUT
            autocomplete="off"
            spellcheck="false"
            prop:value=move || value.get()
            on:input:target=move |event| value.set(event.target().value())
        />
    };
    ferrohealth_ui::view::form::field(id, label, None, control)
}

/// A labelled JSON editor bound to `value`, highlighted with numbered lines
/// and editable once `ready`.
fn area(
    id: &'static str,
    label: &'static str,
    value: RwSignal<String>,
    ready: RwSignal<bool>,
) -> impl IntoView {
    let control = code_editor(
        Editor {
            id,
            language: Language::Json,
            text: value.into(),
            marks: Signal::stored(Vec::new()),
            editable: ready.into(),
        },
        move |text| value.set(text),
    );
    ferrohealth_ui::view::form::field(id, label, None, control)
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
        <section class="fui-card">
            <h2 class="fui-card-title">"openEHR to FHIR: $tofhir"</h2>
            <form method="post" action=RunToFhir::url() autocomplete="off" class=FORM on:submit=submit>
                {area("tofhir-composition", "Composition, canonical or FLAT JSON", composition, ready)}
                {field("tofhir-template", "Template id (required for FLAT)", template_id)}
                {field("tofhir-patient", "context.patient, a reference such as Patient/123", patient)}
                <div class="fui-cluster">
                    <button
                        type="submit"
                        class=Variant::Primary.class()
                        disabled=move || !ready.get() || run.pending().get()
                    >
                        {label(Some(Glyph::PLAY), "Run $tofhir")}
                    </button>
                </div>
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
        <section class="fui-card">
            <h2 class="fui-card-title">"FHIR to openEHR: $toopenehr"</h2>
            <form method="post" action=RunToOpenehr::url() autocomplete="off" class=FORM on:submit=submit>
                {area("toopenehr-bundle", "Bundle, JSON", bundle, ready)}
                {field("toopenehr-template", "Template id", template_id)}
                {ferrohealth_ui::view::form::field(
                    "toopenehr-format",
                    "Composition format",
                    None,
                    view! {
                        <select
                            id="toopenehr-format"
                            name="format"
                            class=format!("{SELECT} {MEDIUM}")
                            on:change:target=move |event| format.set(event.target().value())
                        >
                            <option value="canonical" selected=true>
                                "canonical"
                            </option>
                            <option value="flat">"flat"</option>
                        </select>
                    },
                )}
                <div class="fui-cluster">
                    <button
                        type="submit"
                        class=Variant::Primary.class()
                        disabled=move || !ready.get() || run.pending().get()
                    >
                        {label(Some(Glyph::PLAY), "Run $toopenehr")}
                    </button>
                </div>
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
        {page_header(
            "Transform playground",
            Some(String::from("Runs a transform against the bridge's running programs. Nothing you enter or the bridge answers is kept.")),
            (),
        )}
        {stack(view! {
            {forms.0}
            {forms.1}
        })}
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
        <form method="post" action=RunValidate::url() autocomplete="off" class=FORM on:submit=submit>
            {ferrohealth_ui::view::form::field(
                "validate-type",
                "Resource type",
                None,
                view! {
                    <select
                        id="validate-type"
                        name="resource_type"
                        class=format!("{SELECT} {MEDIUM}")
                        on:change:target=move |event| resource_type.set(event.target().value())
                    >
                        {options}
                    </select>
                },
            )}
            {area("validate-resource", "Resource, JSON", resource, ready)}
            <div class="fui-cluster">
                <button
                    type="submit"
                    class=Variant::Primary.class()
                    disabled=move || !ready.get() || run.pending().get()
                >
                    {label(Some(Glyph::PLAY), "Run $validate")}
                </button>
            </div>
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
        {page_header(
            "Validation dry run",
            Some(String::from("Runs $validate, which commits nothing. Nothing you enter or the bridge answers is kept.")),
            (),
        )}
        <section class="fui-card">
            <h2 class="fui-card-title">"$validate"</h2>
            <Suspense fallback=|| busy("Reading the resource types the bridge validates", Busy::Section)>
                {move || Suspend::new(async move {
                    match types.await {
                        Ok(Outcome::Shown(types)) if types.is_empty() => {
                            fault("The bridge declares $validate on no resource type.").into_any()
                        }
                        Ok(Outcome::Shown(types)) => validate_form(types),
                        Ok(Outcome::Refused(refusal)) => fault(refusal.to_string()).into_any(),
                        Err(error) => fault(error.to_string()).into_any(),
                    }
                })}
            </Suspense>
        </section>
    }
}
