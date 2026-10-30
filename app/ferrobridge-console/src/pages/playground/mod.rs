// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The Playground, a workbench: the bridge's operations run on what the
//! operator enters, keeping nothing.
//!
//! Three binder tabs, each a route: openEHR to FHIR (`$tofhir`), FHIR to
//! openEHR (`$toopenehr`) and Validate a resource (`$validate`). Each tab is
//! one `POST` form: the options row with Run at its end, then the transform
//! pair (B5), the input beside the answer. The form posts to its tab's own
//! address, whose route draws the page again with the answer and the posted
//! input, and nowhere else: no URL, no history, no store holds it
//! (FerroBRIDGE's console rule, §0).

use axum::response::Response;
use ferrobridge_operator_api::health::Info;
use ferrobridge_operator_api::operator::EngineState;
use ferrohealth_ui::highlight::Language;
use ferrohealth_ui::html::Html;
use ferrohealth_ui::i18n::Locale;
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

use crate::actions::{PlaygroundAnswer, PlaygroundPost, Ran};
use crate::app::{Drawn, PLAYGROUND};
use crate::editors::json_input;
use crate::locale::messages::Msg;
use crate::locale::{tr, tr_with};
use crate::markup::{h, t};
use crate::model::{Loaded, Outcome};
use crate::pages::mappings::template_id;
use crate::pages::parts::{faulted, refused};
use crate::visit::Visit;
use ferrobridge_operator_api::capability::Capability;

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

/// The playground, on the tab the route names, after the run `ran` when
/// one was posted.
pub async fn page(visit: &Visit, ran: Option<Ran>) -> Drawn {
    let tab = visit.segment(1).unwrap_or("tofhir").to_owned();
    if !TABS.iter().any(|(key, _)| *key == tab) {
        return crate::pages::errors::not_found(visit);
    }
    let info = visit.reads.info().await;
    let capability = if tab == "validate" {
        Some(visit.reads.capability().await)
    } else {
        None
    };
    let templates = if tab == "validate" {
        None
    } else {
        template_ids(&visit.reads.running().await)
    };
    let body = drawn(
        &visit.locale,
        &tab,
        ran.as_ref(),
        &info,
        capability.as_ref(),
        templates.as_deref(),
    );
    Drawn::shell(visit.tr(Msg::Playground), body)
}

/// Runs the operation of the tab the route names over the form body `body`
/// and draws the page with what the bridge answered.
pub async fn posted(visit: &Visit, body: &str) -> Response {
    let post = PlaygroundPost::from_form(body);
    let answer = match visit.reads.signed_in() {
        Ok((state, token)) => answer::answer(state, token, &visit.path, &post).await,
        Err(fault) => PlaygroundAnswer::Answered(Err(fault)),
    };
    let drawn = page(visit, Some(Ran { post, answer })).await;
    crate::app::render(visit, drawn).await
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
fn operations_off(l: &Locale, info: &Loaded<Info>) -> Option<String> {
    match info {
        Ok(Outcome::Shown(info)) => {
            let on = info
                .lanes
                .iter()
                .any(|lane| lane.name == "operations" && lane.enabled);
            (!on).then(|| tr(l, Msg::OperationsOff))
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
    l: &Locale,
    tab: &str,
    ran: Option<&Ran>,
    info: &Loaded<Info>,
    capability: Option<&Loaded<Capability>>,
    templates: Option<&[String]>,
) -> Html {
    let header = page_heading(
        l,
        Header {
            title: tr(l, Msg::Playground).into(),
            subtitle: Some(tr(l, Msg::PlaygroundSubtitle).into()),
            ..Header::default()
        },
        ActionGroup::none(),
    );
    let off = operations_off(l, info);
    let post = ran.map(|ran| ran.post.clone()).unwrap_or_default();
    let types = match capability {
        Some(Ok(Outcome::Shown(capability))) => validate_types(capability),
        Some(Ok(Outcome::Refused(refusal))) => {
            return header.then(refused(l, refusal));
        }
        Some(Err(fault)) => {
            return header.then(faulted(l, fault, &tab_href(tab)));
        }
        None => Vec::new(),
    };
    let (options, input, answer) = match tab {
        "toopenehr" => to_openehr(l, &post, ran, off, templates),
        "validate" => validate(l, &post, ran, off, &types),
        _ => to_fhir(l, &post, ran, off, templates),
    };
    let panel = h(format!(
        r#"<form method="post" action="{action}" class="fui-panel">{options}{pair}</form>"#,
        action = t(tab_href(tab)),
        pair = transform_pair(input, answer),
    ));
    let tabs = TABS
        .iter()
        .map(|(key, words)| Tab::new(*key, tab_href(key), tr(l, *words)))
        .collect();
    header.then(h(format!(
        r#"<div class="fui-stack">{}</div>"#,
        binder(l, tr(l, Msg::Playground), tabs, tab, panel)
    )))
}

/// Run in its state: off with why, or ready to post.
fn run(l: &Locale, id: &str, label: Msg, busy: Msg, operation: &str, off: Option<String>) -> Html {
    run_button(Run {
        id: id.to_owned(),
        label: tr(l, label),
        busy: tr(l, busy),
        glyph: Glyph::PLAY,
        shortcut: None,
        operation: Some(tr_with(
            l,
            Msg::RunsOperation,
            &[("operation", operation.into())],
        )),
        state: off.map_or(RunState::Ready, RunState::Off),
    })
}

/// The template field of the options row: a choice of the running set's
/// templates, the first leaving it to `unset`, or a text field when the
/// running set is not known.
fn template_field(l: &Locale, value: &str, templates: Option<&[String]>, unset: Msg) -> Html {
    let Some(templates) = templates else {
        return text_field(l, "pg-template", "template", Msg::Template, value, None);
    };
    let choices = templates
        .iter()
        .map(|id| option(id, id == value))
        .collect::<Html>();
    let control = h(format!(
        r#"<select id="pg-template" name="template" class="fui-select"><option value=""{selected}>{unset}</option>{choices}</select>"#,
        selected = if value.is_empty() { " selected" } else { "" },
        unset = t(tr(l, unset)),
    ));
    Field::new("pg-template", tr(l, Msg::Template)).view(control)
}

/// An `<option>` of a choice, its value and words `value`.
fn option(value: &str, selected: bool) -> Html {
    h(format!(
        r#"<option value="{value}"{selected}>{value}</option>"#,
        value = t(value),
        selected = if selected { " selected" } else { "" },
    ))
}

/// A text field of the options row.
fn text_field(
    l: &Locale,
    id: &'static str,
    name: &'static str,
    label: Msg,
    value: &str,
    error: Option<String>,
) -> Html {
    let invalid = if error.is_some() {
        r#" aria-invalid="true""#
    } else {
        ""
    };
    let control = h(format!(
        r#"<input id="{id}" name="{name}" class="fui-input fui-mono" value="{value}" spellcheck="false" autocomplete="off"{invalid}>"#,
        value = t(value),
    ));
    let error = error.map_or_else(Html::empty, |error| {
        h(format!(
            r#"<p id="{}" class="{ERROR}">{}</p>"#,
            t(error_id(id)),
            t(error)
        ))
    });
    Field::new(id, tr(l, label)).after(error).view(control)
}

/// The input pane of a tab: the editor of what the operator enters, with
/// the reason a run was refused under it.
fn input_pane(
    l: &Locale,
    title: Msg,
    tag: &str,
    label: Msg,
    text: &str,
    invalid: Option<String>,
) -> TransformPane {
    let error = invalid.map_or_else(Html::empty, |said| {
        h(format!(
            r#"<p class="fui-help fui-error fui-transform-error">{}</p>"#,
            t(said)
        ))
    });
    TransformPane {
        title: tr(l, title),
        tags: vec![tag.to_owned()],
        state: None,
        actions: None,
        body: json_input(&tr(l, label), text.to_owned()).then(error),
    }
}

/// The answer pane of a tab, waiting for the first run, or holding the
/// answer or the refusal of the run `ran`.
fn answer_pane(
    l: &Locale,
    title: Msg,
    tag: &str,
    waiting: Msg,
    ran: Option<&Ran>,
    operation: &str,
) -> TransformPane {
    let (state, body) = match ran.map(|ran| &ran.answer) {
        None | Some(PlaygroundAnswer::Invalid(_)) => (None, Answer::Waiting(tr(l, waiting)).view()),
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
                l,
                None,
                &Code {
                    label: tr(l, title),
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
                    word: tr(l, Msg::Answered),
                    detail: Some(operation.to_owned()),
                }),
                Answer::Answered(code).view(),
            )
        }
        Some(PlaygroundAnswer::Answered(Ok(Outcome::Refused(refusal)))) => (
            Some(AnswerState {
                outcome: load_report::Outcome::Bad,
                word: tr(l, Msg::Refused),
                detail: refusal
                    .status()
                    .map(|status| format!("{operation} {}", status.as_u16())),
            }),
            Answer::Refused(refused(l, refusal)).view(),
        ),
        Some(PlaygroundAnswer::Answered(Err(fault))) => (
            Some(AnswerState {
                outcome: load_report::Outcome::Bad,
                word: tr(l, Msg::NotRun),
                detail: None,
            }),
            Answer::Refused(faulted(l, fault, PLAYGROUND)).view(),
        ),
    };
    TransformPane {
        title: tr(l, title),
        tags: vec![tag.to_owned()],
        state,
        actions: None,
        body,
    }
}

/// The reason the run `ran` was not sent, when it was not.
fn invalid_of(l: &Locale, ran: Option<&Ran>) -> Option<String> {
    match ran.map(|ran| &ran.answer) {
        Some(PlaygroundAnswer::Invalid(said)) => Some(tr(l, *said)),
        _ => None,
    }
}

/// The `$tofhir` tab.
fn to_fhir(
    l: &Locale,
    post: &PlaygroundPost,
    ran: Option<&Ran>,
    off: Option<String>,
    templates: Option<&[String]>,
) -> (Html, TransformPane, TransformPane) {
    let quiet = off
        .as_ref()
        .map(|said| transform_quiet(said.clone()))
        .unwrap_or_default();
    let fields = template_field(l, &post.template, templates, Msg::TemplateFromComposition).then(
        text_field(
            l,
            "pg-patient",
            "patient",
            Msg::Patient,
            &post.patient,
            None,
        ),
    );
    let options = transform_options(
        Density::Plain,
        fields,
        run(
            l,
            "pg-run",
            Msg::ConvertToFhir,
            Msg::Converting,
            "$tofhir",
            off,
        ),
    )
    .then(quiet);
    (
        options,
        input_pane(
            l,
            Msg::Composition,
            "JSON",
            Msg::CompositionLabel,
            &post.input,
            invalid_of(l, ran),
        ),
        answer_pane(
            l,
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
    l: &Locale,
    post: &PlaygroundPost,
    ran: Option<&Ran>,
    off: Option<String>,
    templates: Option<&[String]>,
) -> (Html, TransformPane, TransformPane) {
    let format = if post.format.is_empty() {
        "canonical"
    } else {
        post.format.as_str()
    };
    let quiet = off
        .as_ref()
        .map(|said| transform_quiet(said.clone()))
        .unwrap_or_default();
    let fields = choice_group(
        "format",
        tr(l, Msg::AnswerFormat),
        &[
            ("canonical", tr(l, Msg::Canonical)),
            ("flat", tr(l, Msg::Flat)),
        ],
        format,
    )
    .then(template_field(
        l,
        &post.template,
        templates,
        Msg::TemplateChosenByMapping,
    ));
    let options = transform_options(
        Density::Plain,
        fields,
        run(
            l,
            "pg-run",
            Msg::ConvertToOpenehr,
            Msg::Converting,
            "$toopenehr",
            off,
        ),
    )
    .then(quiet);
    (
        options,
        input_pane(
            l,
            Msg::Bundle,
            "FHIR R4",
            Msg::BundleLabel,
            &post.input,
            invalid_of(l, ran),
        ),
        answer_pane(
            l,
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
    l: &Locale,
    post: &PlaygroundPost,
    ran: Option<&Ran>,
    off: Option<String>,
    types: &[String],
) -> (Html, TransformPane, TransformPane) {
    let chosen = post.resource_type.clone();
    let choices = types
        .iter()
        .map(|kind| option(kind, *kind == chosen))
        .collect::<Html>();
    let control = h(format!(
        r#"<select id="pg-type" name="resource_type" class="fui-select">{choices}</select>"#
    ));
    let field = Field::new("pg-type", tr(l, Msg::ResourceType)).view(control);
    let off = off.or_else(|| types.is_empty().then(|| tr(l, Msg::ValidateNone)));
    let quiet = off
        .as_ref()
        .map(|said| transform_quiet(said.clone()))
        .unwrap_or_default();
    let options = transform_options(
        Density::Plain,
        field,
        run(
            l,
            "pg-run",
            Msg::ValidateRun,
            Msg::Validating,
            "$validate",
            off,
        ),
    )
    .then(quiet);
    (
        options,
        input_pane(
            l,
            Msg::Resource,
            "FHIR R4",
            Msg::ResourceLabel,
            &post.input,
            invalid_of(l, ran),
        ),
        answer_pane(
            l,
            Msg::Outcome,
            "FHIR R4",
            Msg::ValidateToSee,
            ran,
            "$validate",
        ),
    )
}
