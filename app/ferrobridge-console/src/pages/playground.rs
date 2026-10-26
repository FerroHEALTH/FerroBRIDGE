// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The Playground, a workbench: the bridge's operations run on what the
//! operator enters, keeping nothing.
//!
//! Three binder tabs, each a route: openEHR to FHIR (`$tofhir`), FHIR to
//! openEHR (`$toopenehr`) and Validate a resource (`$validate`). Each tab is
//! one `POST` form: the options row with Run at its end, then the transform
//! pair (B5), the input beside the answer. The answer is drawn in the page
//! the post returns, or in place through the `run` server function once the
//! page has hydrated, and nowhere else: no URL, no history, no store holds
//! it (FerroBRIDGE's console rule, §0).

use ferrobridge_operator_api::health::Info;
use ferrobridge_operator_api::operator::EngineState;
use ferrohealth_ui::highlight::Language;
use ferrohealth_ui::icon::Glyph;
use ferrohealth_ui::view::action::ActionGroup;
use ferrohealth_ui::view::code::{Code, Gravity, LineDiagnostic, code_viewer};
use ferrohealth_ui::view::form::{ERROR, Field, error_id};
use ferrohealth_ui::view::load_report;
use ferrohealth_ui::view::segmented::choice_group;
use ferrohealth_ui::view::surface::{Header, page_heading};
use ferrohealth_ui::view::tabs::{Tab, binder};
use ferrohealth_ui::view::transform::{
    Answer, AnswerState, Density, Run, RunState, TransformPane, run_button, transform_options,
    transform_pair, transform_quiet,
};
#[cfg(not(target_arch = "wasm32"))]
use fhir_types::r4::operation_outcome::OperationOutcome;
#[cfg(not(target_arch = "wasm32"))]
use fhir_types::r4::parameters::{Parameters, ParametersParameter, ParametersParameterValue};
#[cfg(not(target_arch = "wasm32"))]
use fhir_types::r4::primitives;
#[cfg(not(target_arch = "wasm32"))]
use fhir_types::r4::reference::Reference;
use leptos::form::FromFormData as _;
use leptos::prelude::*;
use leptos_router::hooks::use_params_map;

#[cfg(not(target_arch = "wasm32"))]
use crate::actions::RunAnswer;
use crate::actions::{PlaygroundAnswer, PlaygroundPost, Ran, RunOperation};
use crate::app::{PLAYGROUND, page_title};
#[cfg(not(target_arch = "wasm32"))]
use crate::bridge::AccessToken;
use crate::islands::JsonInput;
use crate::locale::messages::Msg;
use crate::locale::{tr, tr_with};
#[cfg(not(target_arch = "wasm32"))]
use crate::model::loaded;
use crate::model::{Loaded, Outcome};
use crate::pages::mappings::template_id;
use crate::pages::parts::{faulted, refused};
#[cfg(not(target_arch = "wasm32"))]
use crate::server::ConsoleState;
use ferrobridge_operator_api::capability::Capability;

/// The tabs of the playground, each its key and its words.
const TABS: [(&str, Msg); 3] = [
    ("tofhir", Msg::TabToFhir),
    ("toopenehr", Msg::TabToOpenehr),
    ("validate", Msg::TabValidate),
];

/// The `$toopenehr` formats the REST API chapter defines.
#[cfg(not(target_arch = "wasm32"))]
const FORMATS: [&str; 2] = ["canonical", "flat"];

/// The page of the tab `key`.
fn tab_href(key: &str) -> String {
    if key == "tofhir" {
        String::from(PLAYGROUND)
    } else {
        format!("{PLAYGROUND}/{key}")
    }
}

/// The tab an address names, `tofhir` for the playground itself.
#[cfg(not(target_arch = "wasm32"))]
fn tab_of(address: &str) -> &str {
    address
        .strip_prefix(PLAYGROUND)
        .and_then(|rest| rest.strip_prefix('/'))
        .unwrap_or("tofhir")
}

/// The playground, on the tab the route names.
#[must_use]
pub fn page() -> impl IntoView {
    let tab = use_params_map()
        .with_untracked(|params| params.get("tab").unwrap_or_else(|| String::from("tofhir")));
    if !TABS.iter().any(|(key, _)| *key == tab) {
        return crate::pages::errors::not_found().into_any();
    }
    let running = Running::new();
    let info = Resource::new(crate::load::changes, |_| crate::load::info_now());
    let capability = (tab == "validate")
        .then(|| Resource::new(crate::load::changes, |_| crate::load::capability_now()));
    let state = (tab != "validate")
        .then(|| Resource::new(crate::load::changes, |_| crate::load::state_now()));
    view! {
        {page_title(tr(Msg::Playground))}
        <Suspense>
            {move || {
                let tab = tab.clone();
                let ran = running.ran();
                Suspend::new(async move {
                    let info = info.await;
                    let capability = match capability {
                        Some(capability) => Some(capability.await),
                        None => None,
                    };
                    let templates = match state {
                        Some(state) => template_ids(&state.await),
                        None => None,
                    };
                    drawn(
                        &tab,
                        ran.as_ref(),
                        &info,
                        capability.as_ref(),
                        templates.as_deref(),
                        running,
                    )
                })
            }}
        </Suspense>
    }
    .into_any()
}

/// How the playground's form posts once the page has hydrated: through the
/// `run` server function, with the post it last sent.
#[derive(Clone, Copy)]
struct Running {
    /// The server action the form dispatches.
    run: ServerAction<RunOperation>,
    /// The post the form last sent.
    last: RwSignal<PlaygroundPost>,
    /// The run the server drew the page with, after a post with no
    /// WebAssembly.
    posted: StoredValue<Option<Ran>>,
}

impl std::fmt::Debug for Running {
    fn fmt(&self, out: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        out.debug_struct("Running").finish_non_exhaustive()
    }
}

impl Running {
    /// The running of a page.
    fn new() -> Self {
        Self {
            run: ServerAction::<RunOperation>::new(),
            last: RwSignal::new(PlaygroundPost::default()),
            posted: StoredValue::new((*SharedValue::new(use_context::<Ran>)).clone()),
        }
    }

    /// The run the page draws: the last one the form sent in the page, else
    /// the one the server drew it with.
    fn ran(self) -> Option<Ran> {
        match self.run.value().get() {
            Some(Ok(ran)) => Some(ran),
            Some(Err(fault)) => Some(Ran {
                post: self.last.get_untracked(),
                answer: PlaygroundAnswer::Answered(Err(fault)),
            }),
            None => self.posted.get_value(),
        }
    }
}

/// The ids of the templates the running set compiled against, or `None`
/// when the operator API did not answer them.
fn template_ids(state: &Loaded<EngineState>) -> Option<Vec<String>> {
    match state {
        Ok(Outcome::Shown(state)) => Some(
            state
                .templates
                .iter()
                .map(|template| template_id(template).to_owned())
                .collect(),
        ),
        Ok(Outcome::Refused(_)) | Err(_) => None,
    }
}

/// Whether the bridge's operations lane runs, or why it does not.
fn operations_off(info: &Loaded<Info>) -> Option<String> {
    match info {
        Ok(Outcome::Shown(info)) => {
            let on = info
                .lanes
                .iter()
                .any(|lane| lane.name == "operations" && lane.enabled);
            (!on).then(|| tr(Msg::OperationsOff))
        }
        Ok(Outcome::Refused(_)) | Err(_) => None,
    }
}

/// The resource types the bridge declares `$validate` on, in its order.
fn validate_types(capability: &Capability) -> Vec<String> {
    capability
        .declaring("validate")
        .into_iter()
        .map(str::to_owned)
        .collect()
}

/// The page over the tab `tab`, after the run `ran` when one was posted.
fn drawn(
    tab: &str,
    ran: Option<&Ran>,
    info: &Loaded<Info>,
    capability: Option<&Loaded<Capability>>,
    templates: Option<&[String]>,
    running: Running,
) -> AnyView {
    let header = page_heading(
        Header {
            title: tr(Msg::Playground).into(),
            subtitle: Some(tr(Msg::PlaygroundSubtitle).into()),
            ..Header::default()
        },
        ActionGroup::none(),
    )
    .into_any();
    let off = operations_off(info);
    let post = ran.map(|ran| ran.post.clone()).unwrap_or_default();
    let types = match capability {
        Some(Ok(Outcome::Shown(capability))) => validate_types(capability),
        Some(Ok(Outcome::Refused(refusal))) => {
            return view! { {header} {refused(refusal)} }.into_any();
        }
        Some(Err(fault)) => {
            return view! { {header} {faulted(fault, &tab_href(tab))} }.into_any();
        }
        None => Vec::new(),
    };
    let (options, input, answer) = match tab {
        "toopenehr" => to_openehr(&post, ran, off, templates),
        "validate" => validate(&post, ran, off, &types),
        _ => to_fhir(&post, ran, off, templates),
    };
    let address = tab_href(tab);
    // NOTE: no specification governs this: our own design; the form keeps its own
    // route as `action`, so a post with no WebAssembly draws the page with its answer.
    let submit = {
        let address = address.clone();
        move |event: leptos::ev::SubmitEvent| {
            let Ok(post) = PlaygroundPost::from_event(&event) else {
                return;
            };
            event.prevent_default();
            running.last.set(post.clone());
            running.run.dispatch(RunOperation {
                address: address.clone(),
                post,
            });
        }
    };
    let panel = view! {
        <form method="post" action=address class="fui-panel" on:submit=submit>
            {options}
            {transform_pair(input, answer)}
        </form>
    }
    .into_any();
    let tabs = TABS
        .iter()
        .map(|(key, words)| Tab::new(*key, tab_href(key), tr(*words)))
        .collect();
    view! {
        {header}
        <div class="fui-stack">{binder(tr(Msg::Playground), tabs, tab, panel)}</div>
    }
    .into_any()
}

/// Run in its state: off with why, or ready to post.
fn run(id: &str, label: Msg, busy: Msg, operation: &str, off: Option<String>) -> AnyView {
    run_button(Run {
        id: id.to_owned(),
        label: tr(label),
        busy: tr(busy),
        glyph: Glyph::PLAY,
        shortcut: None,
        operation: Some(tr_with(
            Msg::RunsOperation,
            &[("operation", operation.into())],
        )),
        state: off.map_or(RunState::Ready, RunState::Off),
    })
}

/// The template field of the options row: a choice of the running set's
/// templates, the first leaving it to `unset`, or a text field when the
/// running set is not known.
fn template_field(value: &str, templates: Option<&[String]>, unset: Msg) -> AnyView {
    let Some(templates) = templates else {
        return text_field("pg-template", "template", Msg::Template, value, None);
    };
    let choices = templates
        .iter()
        .map(|id| {
            let on = id == value;
            view! { <option value=id.clone() selected=on>{id.clone()}</option> }
        })
        .collect_view();
    let control = view! {
        <select id="pg-template" name="template" class="fui-select">
            <option value="" selected=value.is_empty()>{tr(unset)}</option>
            {choices}
        </select>
    };
    Field::new("pg-template", tr(Msg::Template)).view(control)
}

/// A text field of the options row.
fn text_field(
    id: &'static str,
    name: &'static str,
    label: Msg,
    value: &str,
    error: Option<String>,
) -> AnyView {
    let invalid = error.is_some().then_some("true");
    let control = view! {
        <input
            id=id
            name=name
            class="fui-input fui-mono"
            value=value.to_owned()
            spellcheck="false"
            autocomplete="off"
            aria-invalid=invalid
        />
    };
    let error = error.map(|error| view! { <p id=error_id(id) class=ERROR>{error}</p> });
    Field::new(id, tr(label)).after(error).view(control)
}

/// The input pane of a tab: the editor of what the operator enters, with
/// the reason a run was refused under it.
fn input_pane(
    title: Msg,
    tag: &str,
    label: Msg,
    text: &str,
    invalid: Option<String>,
) -> TransformPane {
    let error =
        invalid.map(|said| view! { <p class="fui-help fui-error fui-transform-error">{said}</p> });
    TransformPane {
        title: tr(title),
        tags: vec![tag.to_owned()],
        state: None,
        actions: None,
        body: view! {
            <JsonInput label=tr(label) text=text.to_owned() />
            {error}
        }
        .into_any(),
    }
}

/// The answer pane of a tab, waiting for the first run, or holding the
/// answer or the refusal of the run `ran`.
fn answer_pane(
    title: Msg,
    tag: &str,
    waiting: Msg,
    ran: Option<&Ran>,
    operation: &str,
) -> TransformPane {
    let (state, body) = match ran.map(|ran| &ran.answer) {
        None | Some(PlaygroundAnswer::Invalid(_)) => (None, Answer::Waiting(tr(waiting)).view()),
        Some(PlaygroundAnswer::Answered(Ok(Outcome::Shown(answered)))) => {
            let document = &answered.document;
            let text =
                serde_json::to_string_pretty(document).unwrap_or_else(|_| document.to_string());
            let diagnostics = answered
                .issues
                .iter()
                .map(|said| LineDiagnostic {
                    line: 0,
                    gravity: Gravity::Warning,
                    code: String::from("OperationOutcome"),
                    message: said.clone(),
                })
                .collect();
            let code = code_viewer(
                None,
                &Code {
                    label: tr(title),
                    language: Language::Json,
                    text,
                    ids: None,
                    current: None,
                    diagnostics,
                },
            );
            (
                Some(AnswerState {
                    outcome: load_report::Outcome::Ok,
                    word: tr(Msg::Answered),
                    detail: Some(operation.to_owned()),
                }),
                Answer::Answered(code).view(),
            )
        }
        Some(PlaygroundAnswer::Answered(Ok(Outcome::Refused(refusal)))) => (
            Some(AnswerState {
                outcome: load_report::Outcome::Bad,
                word: tr(Msg::Refused),
                detail: refusal
                    .status()
                    .map(|status| format!("{operation} {}", status.as_u16())),
            }),
            Answer::Refused(refused(refusal)).view(),
        ),
        Some(PlaygroundAnswer::Answered(Err(fault))) => (
            Some(AnswerState {
                outcome: load_report::Outcome::Bad,
                word: tr(Msg::NotRun),
                detail: None,
            }),
            Answer::Refused(faulted(fault, PLAYGROUND)).view(),
        ),
    };
    TransformPane {
        title: tr(title),
        tags: vec![tag.to_owned()],
        state,
        actions: None,
        body,
    }
}

/// The reason the run `ran` was not sent, when it was not.
fn invalid_of(ran: Option<&Ran>) -> Option<String> {
    match ran.map(|ran| &ran.answer) {
        Some(PlaygroundAnswer::Invalid(said)) => Some(tr(*said)),
        _ => None,
    }
}

/// The `$tofhir` tab.
fn to_fhir(
    post: &PlaygroundPost,
    ran: Option<&Ran>,
    off: Option<String>,
    templates: Option<&[String]>,
) -> (AnyView, TransformPane, TransformPane) {
    let quiet = off.as_ref().map(|said| transform_quiet(said.clone()));
    let fields = view! {
        {template_field(&post.template, templates, Msg::TemplateFromComposition)}
        {text_field("pg-patient", "patient", Msg::Patient, &post.patient, None)}
    };
    let options = view! {
        {transform_options(Density::Plain, fields, run("pg-run", Msg::ConvertToFhir, Msg::Converting, "$tofhir", off))}
        {quiet}
    }
    .into_any();
    (
        options,
        input_pane(
            Msg::Composition,
            "JSON",
            Msg::CompositionLabel,
            &post.input,
            invalid_of(ran),
        ),
        answer_pane(
            Msg::Bundle,
            "FHIR R4",
            Msg::ConvertToSeeBundle,
            ran,
            "$tofhir",
        ),
    )
}

/// The `$toopenehr` tab.
fn to_openehr(
    post: &PlaygroundPost,
    ran: Option<&Ran>,
    off: Option<String>,
    templates: Option<&[String]>,
) -> (AnyView, TransformPane, TransformPane) {
    let format = if post.format.is_empty() {
        "canonical"
    } else {
        post.format.as_str()
    };
    let quiet = off.as_ref().map(|said| transform_quiet(said.clone()));
    let fields = view! {
        {choice_group("format", tr(Msg::AnswerFormat), &[("canonical", tr(Msg::Canonical)), ("flat", tr(Msg::Flat))], format)}
        {template_field(&post.template, templates, Msg::TemplateChosenByMapping)}
    };
    let options = view! {
        {transform_options(Density::Plain, fields, run("pg-run", Msg::ConvertToOpenehr, Msg::Converting, "$toopenehr", off))}
        {quiet}
    }
    .into_any();
    (
        options,
        input_pane(
            Msg::Bundle,
            "FHIR R4",
            Msg::BundleLabel,
            &post.input,
            invalid_of(ran),
        ),
        answer_pane(
            Msg::Composition,
            "openEHR",
            Msg::ConvertToSeeComposition,
            ran,
            "$toopenehr",
        ),
    )
}

/// The `$validate` tab.
fn validate(
    post: &PlaygroundPost,
    ran: Option<&Ran>,
    off: Option<String>,
    types: &[String],
) -> (AnyView, TransformPane, TransformPane) {
    let chosen = post.resource_type.clone();
    let choices = types
        .iter()
        .map(|kind| {
            let on = *kind == chosen;
            view! { <option value=kind.clone() selected=on>{kind.clone()}</option> }
        })
        .collect_view();
    let control = view! {
        <select id="pg-type" name="resource_type" class="fui-select">{choices}</select>
    };
    let field = Field::new("pg-type", tr(Msg::ResourceType)).view(control);
    let off = off.or_else(|| types.is_empty().then(|| tr(Msg::ValidateNone)));
    let quiet = off.as_ref().map(|said| transform_quiet(said.clone()));
    let options = view! {
        {transform_options(Density::Plain, field, run("pg-run", Msg::ValidateRun, Msg::Validating, "$validate", off))}
        {quiet}
    }
    .into_any();
    (
        options,
        input_pane(
            Msg::Resource,
            "FHIR R4",
            Msg::ResourceLabel,
            &post.input,
            invalid_of(ran),
        ),
        answer_pane(
            Msg::Outcome,
            "FHIR R4",
            Msg::ValidateToSee,
            ran,
            "$validate",
        ),
    )
}

/// A parameter named `name` carrying `value`.
#[cfg(not(target_arch = "wasm32"))]
fn parameter(name: &str, value: ParametersParameterValue) -> ParametersParameter {
    ParametersParameter {
        name: primitives::String::from(name),
        value: Some(value),
        ..ParametersParameter::default()
    }
}

/// What the bridge answers the post `post` of the playground at `address`,
/// as the operator whose access token is `token`.
#[cfg(not(target_arch = "wasm32"))]
pub async fn answer(
    state: &ConsoleState,
    token: &AccessToken,
    address: &str,
    post: &PlaygroundPost,
) -> PlaygroundAnswer {
    let input = post.input.trim();
    match tab_of(address) {
        "toopenehr" => to_openehr_answer(state, token, input, post).await,
        "validate" => validate_answer(state, token, input, post).await,
        _ => to_fhir_answer(state, token, input, post).await,
    }
}

/// Runs `$tofhir` over the composition `input`.
#[cfg(not(target_arch = "wasm32"))]
async fn to_fhir_answer(
    state: &ConsoleState,
    token: &AccessToken,
    input: &str,
    post: &PlaygroundPost,
) -> PlaygroundAnswer {
    if input.is_empty() {
        return PlaygroundAnswer::Invalid(Msg::CompositionEmpty);
    }
    let mut parameters = vec![parameter(
        "composition",
        ParametersParameterValue::String(primitives::String::from(input)),
    )];
    let template = post.template.trim();
    if !template.is_empty() {
        parameters.push(parameter(
            "templateId",
            ParametersParameterValue::String(primitives::String::from(template)),
        ));
    }
    let patient = post.patient.trim();
    if !patient.is_empty() {
        // NOTE: FHIRconnect REST API chapter, `ToFhir.fsh`: the patient
        // travels as `context.patient`, a `Reference`, in the request body.
        let reference = Reference {
            reference: Some(primitives::String::from(patient)),
            ..Reference::default()
        };
        parameters.push(ParametersParameter {
            name: primitives::String::from("context"),
            part: vec![parameter(
                "patient",
                ParametersParameterValue::Reference(Box::new(reference)),
            )],
            ..ParametersParameter::default()
        });
    }
    let request = Parameters {
        parameter: parameters,
        ..Parameters::default()
    };
    PlaygroundAnswer::Answered(
        loaded(state.bridge().to_fhir(&request, token).await)
            .map(|outcome| outcome.map(RunAnswer::from)),
    )
}

/// Runs `$toopenehr` over the Bundle `input`.
#[cfg(not(target_arch = "wasm32"))]
async fn to_openehr_answer(
    state: &ConsoleState,
    token: &AccessToken,
    input: &str,
    post: &PlaygroundPost,
) -> PlaygroundAnswer {
    let format = if post.format.is_empty() {
        "canonical"
    } else {
        post.format.as_str()
    };
    if !FORMATS.contains(&format) {
        return PlaygroundAnswer::Invalid(Msg::FormatNotKnown);
    }
    // NOTE: no specification governs this: our own design; text that is no
    // JSON is the operator's input to correct, never sent.
    let Ok(bundle) = serde_json::from_str::<serde_json::Value>(input) else {
        return PlaygroundAnswer::Invalid(Msg::BundleNotJson);
    };
    if bundle
        .get("resourceType")
        .and_then(serde_json::Value::as_str)
        != Some("Bundle")
    {
        return PlaygroundAnswer::Invalid(Msg::BundleNotBundle);
    }
    let mut parameter = vec![serde_json::json!({ "name": "bundle", "resource": bundle })];
    let template = post.template.trim();
    if !template.is_empty() {
        parameter.push(serde_json::json!({ "name": "templateId", "valueString": template }));
    }
    parameter.push(serde_json::json!({ "name": "format", "valueCode": format }));
    // NOTE: FHIRconnect `ToOpenEhr.fsh` declares `bundle` a resource part; the
    // console's model holds no clinical resource, so the Bundle passes as JSON.
    let envelope = serde_json::json!({ "resourceType": "Parameters", "parameter": parameter });
    let answer = loaded(state.bridge().to_openehr(&envelope, token).await)
        .map(|outcome| outcome.map(|(parameters, raw)| converted(&parameters, raw)));
    PlaygroundAnswer::Answered(answer)
}

/// Runs `$validate` on the resource `input`.
#[cfg(not(target_arch = "wasm32"))]
async fn validate_answer(
    state: &ConsoleState,
    token: &AccessToken,
    input: &str,
    post: &PlaygroundPost,
) -> PlaygroundAnswer {
    // NOTE: no specification governs this: our own design; text that is no
    // JSON is the operator's input to correct, never sent.
    let Ok(parsed) = serde_json::from_str::<serde_json::Value>(input) else {
        return PlaygroundAnswer::Invalid(Msg::ResourceNotJson);
    };
    if parsed
        .get("resourceType")
        .and_then(serde_json::Value::as_str)
        != Some(post.resource_type.as_str())
    {
        return PlaygroundAnswer::Invalid(Msg::ResourceTypeMismatch);
    }
    let answer = loaded(
        state
            .bridge()
            .validate(&post.resource_type, input.to_owned(), token)
            .await,
    )
    .map(|outcome| outcome.map(|(_outcome, raw)| RunAnswer::from(raw)));
    PlaygroundAnswer::Answered(answer)
}

/// What a `$toopenehr` answer shows: the composition its `composition`
/// parameter carries, with the issues of its `outcome` beside it, or the
/// whole answer when it carries no composition (FHIRconnect
/// `ToOpenEhr.fsh`).
#[cfg(not(target_arch = "wasm32"))]
fn converted(parameters: &Parameters, raw: serde_json::Value) -> RunAnswer {
    let named = |name: &str| {
        parameters
            .parameter
            .iter()
            .find(|parameter| parameter.name.value.as_deref() == Some(name))
    };
    let composition = named("composition").and_then(|parameter| match &parameter.value {
        Some(ParametersParameterValue::String(text)) => text.value.as_deref(),
        _ => None,
    });
    let issues = named("outcome")
        .and_then(|parameter| match &parameter.resource {
            Some(fhir_types::r4::resource::Resource::OperationOutcome(outcome)) => {
                Some(issue_words(outcome))
            }
            _ => None,
        })
        .unwrap_or_default();
    let Some(text) = composition else {
        return RunAnswer::from(raw);
    };
    // NOTE: FHIRconnect `ToOpenEhr.fsh`: the composition is serialized as a string;
    // one that is no JSON is shown as the text it is.
    let document =
        serde_json::from_str(text).unwrap_or_else(|_| serde_json::Value::String(text.to_owned()));
    RunAnswer { document, issues }
}

/// The words of each issue of `outcome`: its `details.text` and its
/// `diagnostics`, each when present (FHIR R4 `OperationOutcome.issue`).
#[cfg(not(target_arch = "wasm32"))]
fn issue_words(outcome: &OperationOutcome) -> Vec<String> {
    outcome
        .issue
        .iter()
        .flat_map(|issue| {
            let details = issue
                .details
                .as_ref()
                .and_then(|details| details.text.as_ref())
                .and_then(|text| text.value.clone());
            let diagnostics = issue
                .diagnostics
                .as_ref()
                .and_then(|text| text.value.clone());
            details.into_iter().chain(diagnostics)
        })
        .collect()
}
