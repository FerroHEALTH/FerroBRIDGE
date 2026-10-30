// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The editor, a workbench.
//!
//! The code editor (B2) sits beside the diagnostics of its draft (B3), with
//! Check in its toolbar, and the reason and Save in the docked bar (R4), the
//! save confirmed in the bar before it is sent.
//!
//! The editor is one plain `POST` form: Check asks the bridge to compile the
//! set with the draft and writes nothing; Save asks to confirm, and Confirm
//! the save sends the draft under `If-Match` on the digest it was opened
//! from, with its reason. A provisioned file is never saved over: its draft
//! is checked, and saved under a path of its own. A new file names its path
//! below `fhirconnect/` or `omocl/`. The form posts to the editor's own
//! address, whose route draws the page again with the bridge's answer and
//! the posted draft; Discard answers `303 See Other` to the file. The
//! library's `editor` module adds the highlighting as you type and keeps the
//! caret and the diagnostics in step.

use axum::response::{IntoResponse, Redirect, Response};
use ferrobridge_operator_api::operator::{Listed, LoadReport};
use ferrohealth_ui::html::Html;
use ferrohealth_ui::i18n::Locale;
use ferrohealth_ui::icon::Glyph;
use ferrohealth_ui::tone::Tone;
use ferrohealth_ui::view::action::ActionGroup;
use ferrohealth_ui::view::button::Variant;
use ferrohealth_ui::view::diagnostics::FileFindings;
use ferrohealth_ui::view::dock::{
    Decision, DecisionBar, DecisionState, DockField, DockForm, DockLine, decision_bar,
};
use ferrohealth_ui::view::editor::Opened;
use ferrohealth_ui::view::form::{ERROR, error_id};
use ferrohealth_ui::view::load_report;
use ferrohealth_ui::view::load_report::{Arrival, Count};
use ferrohealth_ui::view::notice::{Announce, notice};
use ferrohealth_ui::view::surface::{Crumb, Header, Mode, card, editor_heading};
use http::StatusCode;

use crate::actions::{Edited, EditorAnswer, EditorPost, Field};
use crate::app::{Drawn, EDIT, MAPPINGS, NEW_FILE};
use crate::editors::{EditorText, mapping_editor};
use crate::locale::messages::Msg;
use crate::locale::{message, tr, tr_with};
use crate::markup::{h, t};
use crate::model::{Answered, Outcome, Refusal};
use crate::pages::file::short_digest;
use crate::pages::parts::{
    faulted, file_href, findings, refused, refused_operator, short, tally, wire_path,
};
use crate::visit::Visit;

/// The id of the editor's form, which the docked bar's fields and
/// decisions join.
pub const FORM: &str = "editor";

pub mod answer;

/// The editor of the file the route names, or of a new file, after the
/// post `edited` when one was sent.
pub async fn page(visit: &Visit, edited: Option<Edited>) -> Drawn {
    let l = &visit.locale;
    let path = visit
        .path
        .strip_prefix(EDIT)
        .and_then(|rest| rest.strip_prefix('/'))
        .and_then(crate::pages::parts::decoded)
        .unwrap_or_default();
    if path.is_empty() {
        let opened = Opening {
            path: String::new(),
            text: String::new(),
            base: String::new(),
            listed: None,
        };
        return Drawn::shell(
            tr(l, Msg::NewMappingFile),
            editor(l, &opened, edited.as_ref()),
        );
    }
    let title = tr_with(l, Msg::EditTitle, &[("name", short(&path).into())]);
    let file = visit.reads.opened(&path).await;
    let listing = visit.reads.listing().await;
    let body = match (file, listing) {
        (Ok(Outcome::Shown(file)), Ok(Outcome::Shown(listing))) => {
            let Some(text) = file.text else {
                return Drawn::shell(
                    title,
                    crate::pages::parts::quiet(tr(l, Msg::FileNotUtf8), None),
                );
            };
            let listed = listing.iter().find(|each| each.path == path).cloned();
            let opened = Opening {
                path: path.clone(),
                text,
                base: file.etag.unwrap_or_default(),
                listed,
            };
            editor(l, &opened, edited.as_ref())
        }
        (Err(fault), _) | (_, Err(fault)) => faulted(l, &fault, &format!("{EDIT}/{path}")),
        (Ok(Outcome::Refused(refusal)), _) => {
            if refusal.not_served() {
                return crate::pages::errors::not_found(visit);
            }
            refused(l, &refusal)
        }
        (_, Ok(Outcome::Refused(refusal))) => refused_operator(l, &refusal),
    };
    Drawn::shell(title, body)
}

/// Answers the editor's form body `body`: Discard leaves for the file the
/// draft was opened from, and every other decision draws the editor again
/// with the bridge's answer and the posted draft.
pub async fn posted(visit: &Visit, body: &str) -> Response {
    let post = EditorPost::from_form(body);
    if post.decision == "discard" {
        return Redirect::to(&crate::posts::discarded(&visit.path)).into_response();
    }
    let edited = match visit.reads.signed_in() {
        Ok((state, token)) => crate::posts::edited(state, token, &visit.path, post).await,
        Err(fault) => Edited {
            post,
            answer: EditorAnswer::Checked(Err(fault)),
        },
    };
    let drawn = page(visit, Some(edited)).await;
    crate::app::render(visit, drawn).await
}

/// The file an editor opened.
#[derive(Debug, Clone)]
struct Opening {
    /// Its wire path; empty for a new file.
    path: String,
    /// Its text as opened.
    text: String,
    /// The digest it was opened at; empty for a new file.
    base: String,
    /// What the listing says of it.
    listed: Option<Listed>,
}

impl Opening {
    /// Whether it is a new file.
    fn is_new(&self) -> bool {
        self.path.is_empty()
    }

    /// Whether the console never saves over it.
    fn read_only(&self) -> bool {
        self.listed.as_ref().is_some_and(|listed| listed.read_only)
    }
}

/// What the page shows of a post's answer.
#[derive(Debug, Default)]
struct Shown {
    /// The strip over the editor: a load report or a refusal.
    strip: Option<Html>,
    /// The findings the island draws.
    findings: Vec<ferrohealth_ui::view::diagnostics::Finding>,
    /// The bar's line.
    line: Option<DockLine>,
    /// Whether the draft was checked.
    checked: bool,
    /// Whether Save waits on a draft the set does not compile with.
    blocked: Option<String>,
    /// Whether the bar asks to confirm the save.
    confirm: bool,
    /// Whether the draft was saved.
    saved: Option<String>,
    /// An error under the reason.
    reason_error: Option<String>,
    /// An error under the path.
    path_error: Option<String>,
}

/// The counts of a load report of the set `report`.
fn set_counts(l: &Locale, report: &LoadReport) -> Vec<Count> {
    let (errors, warnings) = tally(&report.diagnostics);
    vec![
        Count {
            value: errors.to_string(),
            noun: tr_with(l, Msg::NounErrorsInSet, &[("count", errors.into())]),
        },
        Count {
            value: warnings.to_string(),
            noun: tr_with(l, Msg::NounWarningsInSet, &[("count", warnings.into())]),
        },
    ]
}

/// The findings of `report` about the file at `path`.
fn findings_of(report: &LoadReport, path: &str) -> Vec<ferrohealth_ui::view::diagnostics::Finding> {
    let found = report
        .diagnostics
        .iter()
        .filter(|diagnostic| wire_path(diagnostic) == path)
        .collect::<Vec<_>>();
    findings(&found)
}

/// What the page shows of `edited`, the post the editor of `path` sent.
fn shown(l: &Locale, edited: &Edited, path: &str) -> Shown {
    match &edited.answer {
        EditorAnswer::Confirm => Shown {
            confirm: true,
            line: Some(DockLine::State {
                glyph: Some(Glyph::CIRCLE_ALERT),
                said: tr(l, Msg::ConfirmSave),
            }),
            ..Shown::default()
        },
        EditorAnswer::Back => Shown::default(),
        EditorAnswer::Invalid { field, said } => Shown {
            reason_error: (*field == Field::Reason).then(|| tr(l, *said)),
            path_error: (*field == Field::Path).then(|| tr(l, *said)),
            ..Shown::default()
        },
        EditorAnswer::Checked(Ok(Outcome::Shown(answered))) => {
            let (errors, warnings) = tally(&answered.report.diagnostics);
            let said = tr_with(
                l,
                if answered.report.clean {
                    Msg::CheckClean
                } else {
                    Msg::CheckErrors
                },
                &[("errors", errors.into()), ("warnings", warnings.into())],
            );
            // NOTE: no specification governs this: our own design; a draft that does not
            // compile says so once, as the reason its save is blocked.
            let clean = answered.report.clean;
            Shown {
                findings: findings_of(&answered.report, path),
                line: clean.then(|| DockLine::State {
                    glyph: Some(Glyph::CIRCLE_CHECK),
                    said: said.clone(),
                }),
                checked: true,
                blocked: (!clean).then_some(said),
                ..Shown::default()
            }
        }
        EditorAnswer::Saved(Ok(Outcome::Shown(answered))) => saved(l, answered, path),
        EditorAnswer::Checked(Ok(Outcome::Refused(refusal)))
        | EditorAnswer::Saved(Ok(Outcome::Refused(refusal))) => refusal_shown(l, refusal),
        EditorAnswer::Checked(Err(fault)) | EditorAnswer::Saved(Err(fault)) => Shown {
            strip: Some(faulted(l, fault, &edit_href_of(path))),
            ..Shown::default()
        },
    }
}

/// The editor's own address for the file at `path`, the new file's for none.
fn edit_href_of(path: &str) -> String {
    if path.is_empty() {
        String::from(NEW_FILE)
    } else {
        format!("{EDIT}/{path}")
    }
}

/// What the page shows of a save the bridge answered.
fn saved(l: &Locale, answered: &Answered, path: &str) -> Shown {
    if answered.status == StatusCode::OK {
        let digest = answered
            .etag
            .as_deref()
            .map(short_digest)
            .unwrap_or_default();
        let report = load_report::load_report(load_report::LoadReport {
            outcome: load_report::Outcome::Ok,
            arrival: Arrival::AfterAction,
            title: t(tr_with(
                l,
                Msg::SavedReport,
                &[("digest", digest.clone().into())],
            )),
            counts: set_counts(l, &answered.report),
            action: Some(h(format!(
                r#"<a href="{href}" class="{class}">{label}</a>"#,
                href = t(file_href(path)),
                class = Variant::Secondary.small(),
                label = ferrohealth_ui::view::button::label(
                    Some(Glyph::ARROW_RIGHT),
                    tr(l, Msg::OpenFile)
                ),
            ))),
        });
        Shown {
            strip: Some(report),
            findings: findings_of(&answered.report, path),
            line: Some(DockLine::Done(tr(l, Msg::SavedLine))),
            saved: Some(digest),
            ..Shown::default()
        }
    } else {
        let report = load_report::load_report(load_report::LoadReport {
            outcome: load_report::Outcome::Bad,
            arrival: Arrival::AfterAction,
            title: t(tr(l, Msg::SaveRefusedCompile)),
            counts: set_counts(l, &answered.report),
            action: None,
        });
        Shown {
            strip: Some(report),
            findings: findings_of(&answered.report, path),
            checked: true,
            blocked: Some(tr(l, Msg::SaveBlocked)),
            ..Shown::default()
        }
    }
}

/// What the page shows of a refused check or save: the refusal over the
/// editor, and the path's error for a provisioned path.
fn refusal_shown(l: &Locale, refusal: &Refusal) -> Shown {
    let (said, path_error) = match refusal.status() {
        Some(StatusCode::PRECONDITION_FAILED) => (Some(tr(l, Msg::SaveConflict)), None),
        Some(StatusCode::CONFLICT) => (
            Some(tr(l, Msg::SaveProvisionedPath)),
            Some(tr(l, Msg::ChooseOtherPath)),
        ),
        _ => (None, None),
    };
    let strip = match said {
        Some(said) => notice(Tone::Bad, Announce::Alert, None, t(said)),
        None => refused(l, refusal),
    };
    Shown {
        strip: Some(strip),
        path_error,
        ..Shown::default()
    }
}

/// The editor page over the file `opened`, after the post `edited` when one
/// was sent.
fn editor(l: &Locale, opened: &Opening, edited: Option<&Edited>) -> Html {
    let post = edited.map(|edited| edited.post.clone()).unwrap_or_default();
    let draft = if edited.is_some() {
        post.draft.clone()
    } else {
        opened.text.clone()
    };
    let save_path = if opened.is_new() || opened.read_only() {
        post.path.trim().to_owned()
    } else {
        opened.path.clone()
    };
    let shown = edited
        .map(|edited| shown(l, edited, &save_path))
        .unwrap_or_default();
    let header = heading(l, opened, shown.saved.as_deref());
    let lock = if opened.read_only() {
        notice(
            Tone::Info,
            Announce::Status,
            None,
            t(tr(l, Msg::ProvisionedEditorNotice)),
        )
    } else {
        Html::empty()
    };
    let path_card = if opened.is_new() || opened.read_only() {
        path_card(l, opened, &post.path, shown.path_error.clone())
    } else {
        Html::empty()
    };
    let findings = FileFindings {
        file: save_path.clone(),
        href: None,
        lines: format!("#{}-L", crate::editors::DRAFT),
        current: None,
        findings: shown.findings.clone(),
        all: None,
    };
    let text = EditorText {
        label: tr_with(l, Msg::DraftOf, &[("name", short(&opened.path).into())]),
        lead: tr(
            l,
            if opened.path.starts_with("omocl/") {
                Msg::LeadOmocl
            } else {
                Msg::LeadFhirconnect
            },
        ),
        facts: vec![tr(l, Msg::EditorFacts)],
        check: tr(l, Msg::Check),
        diagnostics: tr(l, Msg::TabDiagnostics),
        none: tr(
            l,
            if shown.checked {
                Msg::DraftNoDiagnostics
            } else {
                Msg::NotCheckedYet
            },
        ),
    };
    let opened_text = if opened.is_new() {
        Opened::New
    } else {
        Opened::Text(opened.text.clone())
    };
    let mapping = mapping_editor(l, draft, opened_text, &findings, text);
    let form = h(format!(
        r#"<form method="post" action="{action}" id="{FORM}" class="fui-stack"><input type="hidden" name="base" value="{base}">{path_card}{mapping}</form>"#,
        action = t(edit_href_of(&opened.path)),
        base = t(&opened.base),
    ));
    let dock = dock(l, opened, &post, &shown);
    let strip = shown.strip.unwrap_or_default();
    header.then(h(format!(
        r#"<div class="fui-stack">{lock}{strip}{form}</div>{dock}"#
    )))
}

/// The title row of the editor of `opened`, saying the new digest once
/// saved: "New mapping file", "Edit", or "Check a draft" for a provisioned
/// file, its trail ending with the list a new file joins or the file.
fn heading(l: &Locale, opened: &Opening, saved: Option<&str>) -> Html {
    if opened.is_new() {
        return editor_heading(
            l,
            message(Msg::EditorTitle),
            &Mode::Creating,
            Header {
                subtitle: Some(tr(l, Msg::NewFileSubtitle).into()),
                crumbs: vec![Crumb::new(tr(l, Msg::MappingSet), MAPPINGS)],
                ..Header::default()
            },
            ActionGroup::none(),
        );
    }
    let title = message(if opened.read_only() {
        Msg::DraftTitle
    } else {
        Msg::EditorTitle
    });
    let subtitle = match saved {
        Some(digest) => tr_with(l, Msg::EditSavedSubtitle, &[("digest", digest.into())]),
        None => tr_with(
            l,
            Msg::EditSubtitle,
            &[("digest", short_digest(&opened.base).into())],
        ),
    };
    editor_heading(
        l,
        title,
        &Mode::Editing(short(&opened.path).to_owned()),
        Header {
            subtitle: Some(subtitle.into()),
            crumbs: vec![
                Crumb::new(tr(l, Msg::MappingSet), format!("{MAPPINGS}/files")),
                Crumb::new(short(&opened.path).to_owned(), file_href(&opened.path)),
            ],
            ..Header::default()
        },
        ActionGroup::none(),
    )
}

/// The card that names where a new file, or a draft of a provisioned one,
/// saves.
fn path_card(l: &Locale, opened: &Opening, value: &str, error: Option<String>) -> Html {
    let (title, label, help) = if opened.is_new() {
        (Msg::TheFile, Msg::Path, Msg::NewPathHelp)
    } else {
        (Msg::WhereToSave, Msg::NewPath, Msg::SaveAsHelp)
    };
    let invalid = if error.is_some() {
        r#" aria-invalid="true""#
    } else {
        ""
    };
    let control = h(format!(
        r#"<input id="editor-path" name="path" form="{FORM}" class="fui-input fui-mono" value="{value}" spellcheck="false" autocomplete="off"{invalid}>"#,
        value = t(value),
    ));
    let error = error.map_or_else(Html::empty, |error| {
        h(format!(
            r#"<p id="{id}" class="{ERROR}">{error}</p>"#,
            id = t(error_id("editor-path")),
            error = t(error),
        ))
    });
    let shown = ferrohealth_ui::view::form::Field::new("editor-path", tr(l, label))
        .help(tr(l, help))
        .after(error)
        .view(control);
    card(tr(l, title), shown)
}

/// The docked bar: the reason, the bar's line, and Discard and Save, or the
/// confirmation's Go back and Confirm the save.
fn dock(l: &Locale, opened: &Opening, post: &EditorPost, shown: &Shown) -> Html {
    let save_words = tr(
        l,
        if opened.is_new() {
            Msg::CreateAndReload
        } else if opened.read_only() {
            Msg::SaveUnderNewPath
        } else {
            Msg::SaveAndReload
        },
    );
    let reason = DockField {
        id: String::from("editor-reason"),
        name: String::from("reason"),
        label: tr(l, Msg::Reason),
        value: post.reason.clone(),
        placeholder: tr(l, Msg::ReasonPlaceholder),
        shortcut: None,
        error: shown.reason_error.clone(),
        help: Some(tr(l, Msg::ReasonHelp)),
        disabled: false,
        focus: shown.reason_error.is_some(),
    };
    let decisions = if shown.confirm {
        vec![
            Decision {
                value: String::from("back"),
                label: tr(l, Msg::GoBack),
                glyph: Some(Glyph::ARROW_LEFT),
                ..Decision::default()
            },
            Decision {
                value: String::from("confirm"),
                label: tr(l, Msg::ConfirmTheSave),
                variant: Variant::Primary,
                glyph: Some(Glyph::SAVE),
                ..Decision::default()
            },
        ]
    } else if shown.saved.is_some() {
        vec![Decision {
            value: String::from("discard"),
            label: tr(l, Msg::Close),
            glyph: Some(Glyph::X),
            leaves: true,
            ..Decision::default()
        }]
    } else {
        vec![
            Decision {
                value: String::from("discard"),
                label: tr(
                    l,
                    if opened.is_new() {
                        Msg::Cancel
                    } else {
                        Msg::Discard
                    },
                ),
                glyph: Some(Glyph::X),
                leaves: true,
                ..Decision::default()
            },
            Decision {
                value: String::from("save"),
                label: save_words,
                variant: Variant::Primary,
                glyph: Some(Glyph::SAVE),
                state: shown
                    .blocked
                    .clone()
                    .map_or(DecisionState::Ready, DecisionState::Blocked),
                ..Decision::default()
            },
        ]
    };
    decision_bar(DecisionBar {
        id: String::from("editor-dock"),
        label: tr(l, Msg::SaveBar),
        form: DockForm::Page(String::from(FORM)),
        fields: vec![reason],
        line: shown.line.clone(),
        decisions,
    })
}
