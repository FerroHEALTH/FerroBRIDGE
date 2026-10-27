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
use ferrohealth_ui::widget::action::{answer_field, answered};
use leptos::form::{ActionForm, FromFormData as _};
use leptos::prelude::*;
use leptos_router::hooks::use_params_map;

use crate::actions::{PlaygroundAnswer, PlaygroundPost, Ran, RunOperation};
use crate::app::{PLAYGROUND, page_title};
use crate::editors::JsonInput;
use crate::locale::messages::Msg;
use crate::locale::{tr, tr_with};
use crate::model::{Loaded, Outcome};
use crate::pages::mappings::template_id;
use crate::pages::parts::{faulted, refused};
use ferrobridge_operator_api::capability::Capability;

#[cfg(not(target_arch = "wasm32"))]
pub mod answer;

/// The tabs of the playground, each its key and its words.
const TABS: [(&str, Msg); 3] = [
    ("tofhir", Msg::TabToFhir),
    ("toopenehr", Msg::TabToOpenehr),
    ("validate", Msg::TabValidate),
];

/// The page of the tab `key`.
fn tab_href(key: &str) -> String {
    if key == "tofhir" {
        String::from(PLAYGROUND)
    } else {
        format!("{PLAYGROUND}/{key}")
    }
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

/// How the playground's form posts: through the `run` server function,
/// with the post it last sent.
#[derive(Clone, Copy)]
struct Running {
    /// The server action the form calls.
    run: ServerAction<RunOperation>,
    /// The post the form last sent: in the page, or the plain post the
    /// server drew the page after.
    last: RwSignal<PlaygroundPost>,
}

impl std::fmt::Debug for Running {
    fn fmt(&self, out: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        out.debug_struct("Running").finish_non_exhaustive()
    }
}

impl Running {
    /// The running of a page.
    fn new() -> Self {
        let run = ServerAction::<RunOperation>::new();
        crate::load::drawn_after(run);
        let posted = answered(&run);
        Self {
            run,
            last: RwSignal::new(PlaygroundPost::from_posted(&posted)),
        }
    }

    /// The run the page draws: the last answer of the form, in the page or
    /// to the plain post the server drew it after.
    fn ran(self) -> Option<Ran> {
        self.run.value().get().map(|answer| {
            answer.unwrap_or_else(|fault| Ran {
                post: self.last.get_untracked(),
                answer: PlaygroundAnswer::Answered(Err(fault)),
            })
        })
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
    // NOTE: UI Events §3.1: a capture listener on an ancestor runs before the form's
    // own, so the post is kept before `<ActionForm>` sends it.
    let submit = move |event: leptos::ev::SubmitEvent| {
        if let Ok(post) = PlaygroundPost::from_event(&event) {
            running.last.set(post);
        }
    };
    let panel = view! {
        <ActionForm action=running.run attr:class="fui-panel">
            {answer_field()}
            <input type="hidden" name="address" value=address />
            {options}
            {transform_pair(input, answer)}
        </ActionForm>
    }
    .into_any();
    let tabs = TABS
        .iter()
        .map(|(key, words)| Tab::new(*key, tab_href(key), tr(*words)))
        .collect();
    view! {
        {header}
        <div class="fui-stack" on:submit:capture=submit>
            {binder(tr(Msg::Playground), tabs, tab, panel)}
        </div>
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
