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
//! below `fhirconnect/` or `omocl/`. The page works with no WebAssembly; the
//! island adds the highlighting and keeps the caret and the diagnostics in
//! step.

use ferrobridge_operator_api::operator::{Listed, LoadReport};
use ferrohealth_ui::icon::Glyph;
use ferrohealth_ui::tone::Tone;
use ferrohealth_ui::view::action::ActionGroup;
use ferrohealth_ui::view::button::Variant;
use ferrohealth_ui::view::diagnostics::{DiagnosticsWords, FileFindings};
use ferrohealth_ui::view::dock::{
    Decision, DecisionBar, DecisionState, DockField, DockForm, DockLine, decision_bar,
};
use ferrohealth_ui::view::form::{ERROR, Field, error_id};
use ferrohealth_ui::view::load_report::{
    Arrival, Count, LoadReport as Report, Outcome as Verdict, load_report,
};
use ferrohealth_ui::view::notice::{Announce, notice};
use ferrohealth_ui::view::surface::{Crumb, Header, Mode, card, editor_heading};
use ferrohealth_ui::widget::editor::{EditorWords, Opened};
use http::StatusCode;
use leptos::prelude::*;
use leptos_router::hooks::use_params_map;

use crate::app::{EDIT, MAPPINGS, NEW_FILE, page_title};
use crate::bridge::AccessToken;
use crate::bridge::editing::{Answered, MAX_REASON, Precondition};
use crate::islands::{EditorText, MappingEditor};
use crate::locale::messages::Msg;
use crate::locale::{message, tr, tr_with};
use crate::model::{Outcome, Refusal, loaded};
use crate::pages::file::short_digest;
use crate::pages::parts::{
    faulted, file_href, findings, refused, refused_operator, short, tally, wire_path,
};
use crate::posts::{Edited, EditorAnswer, EditorPost};
use crate::server::ConsoleState;

/// The id of the editor's form, which the docked bar's fields and
/// decisions join.
pub const FORM: &str = "editor";

/// The editor of the file the route names.
#[must_use]
pub fn page() -> impl IntoView {
    let path = use_params_map()
        .with_untracked(|params| params.get("path"))
        .and_then(|raw| crate::pages::parts::decoded(&raw))
        .unwrap_or_default();
    let edited = use_context::<Edited>();
    view! {
        {page_title(tr_with(Msg::EditTitle, &[("name", short(&path).into())]))}
        <Suspense>
            {move || {
                let path = path.clone();
                let edited = edited.clone();
                Suspend::new(async move {
                    let file = crate::load::file(&path).await;
                    let listing = crate::load::listing().await;
                    match (file, listing) {
                        (Ok(Outcome::Shown(file)), Ok(Outcome::Shown(listing))) => {
                            let Ok(text) = String::from_utf8(file.bytes) else {
                                return crate::pages::parts::quiet(tr(Msg::FileNotUtf8), None);
                            };
                            let listed = listing.iter().find(|each| each.path == path).cloned();
                            let opened = Opening {
                                path: path.clone(),
                                text,
                                base: file.etag.unwrap_or_default(),
                                listed,
                            };
                            editor(&opened, edited.as_ref())
                        }
                        (Err(fault), _) | (_, Err(fault)) => faulted(&fault, &format!("{EDIT}/{path}")),
                        (Ok(Outcome::Refused(refusal)), _) => {
                            if refusal.not_served() {
                                crate::pages::errors::not_found().into_any()
                            } else {
                                refused(&refusal)
                            }
                        }
                        (_, Ok(Outcome::Refused(refusal))) => refused_operator(&refusal),
                    }
                })
            }}
        </Suspense>
    }
}

/// The editor of a new mapping file.
#[must_use]
pub fn new_page() -> impl IntoView {
    let edited = use_context::<Edited>();
    let opened = Opening {
        path: String::new(),
        text: String::new(),
        base: String::new(),
        listed: None,
    };
    view! {
        {page_title(tr(Msg::NewMappingFile))}
        {editor(&opened, edited.as_ref())}
    }
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
    strip: Option<AnyView>,
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
fn set_counts(report: &LoadReport) -> Vec<Count> {
    let (errors, warnings) = tally(&report.diagnostics);
    vec![
        Count {
            value: errors.to_string(),
            noun: tr_with(Msg::NounErrorsInSet, &[("count", errors.into())]),
        },
        Count {
            value: warnings.to_string(),
            noun: tr_with(Msg::NounWarningsInSet, &[("count", warnings.into())]),
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
fn shown(edited: &Edited, path: &str) -> Shown {
    match &edited.answer {
        EditorAnswer::Confirm => Shown {
            confirm: true,
            line: Some(DockLine::State {
                glyph: Some(Glyph::CIRCLE_ALERT),
                said: tr(Msg::ConfirmSave),
            }),
            ..Shown::default()
        },
        EditorAnswer::Back => Shown::default(),
        EditorAnswer::Invalid { field, said } => Shown {
            reason_error: (*field == "reason").then(|| tr(*said)),
            path_error: (*field == "path").then(|| tr(*said)),
            ..Shown::default()
        },
        EditorAnswer::Checked(Ok(Outcome::Shown(answered))) => {
            let (errors, warnings) = tally(&answered.report.diagnostics);
            let said = tr_with(
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
        EditorAnswer::Saved(Ok(Outcome::Shown(answered))) => saved(answered, path),
        EditorAnswer::Checked(Ok(Outcome::Refused(refusal)))
        | EditorAnswer::Saved(Ok(Outcome::Refused(refusal))) => refusal_shown(refusal),
        EditorAnswer::Checked(Err(fault)) | EditorAnswer::Saved(Err(fault)) => Shown {
            strip: Some(faulted(fault, &edit_href_of(path))),
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
fn saved(answered: &Answered, path: &str) -> Shown {
    if answered.status == StatusCode::OK {
        let digest = answered
            .etag
            .as_deref()
            .map(short_digest)
            .unwrap_or_default();
        let report = load_report(Report {
            outcome: Verdict::Ok,
            arrival: Arrival::AfterAction,
            title: tr_with(Msg::SavedReport, &[("digest", digest.clone().into())]).into_any(),
            counts: set_counts(&answered.report),
            action: Some(
                view! {
                    <a href=file_href(path) class=Variant::Secondary.small()>
                        {ferrohealth_ui::view::button::label(Some(Glyph::ARROW_RIGHT), tr(Msg::OpenFile))}
                    </a>
                }
                .into_any(),
            ),
        });
        Shown {
            strip: Some(report),
            findings: findings_of(&answered.report, path),
            line: Some(DockLine::Done(tr(Msg::SavedLine))),
            saved: Some(digest),
            ..Shown::default()
        }
    } else {
        let report = load_report(Report {
            outcome: Verdict::Bad,
            arrival: Arrival::AfterAction,
            title: tr(Msg::SaveRefusedCompile).into_any(),
            counts: set_counts(&answered.report),
            action: None,
        });
        Shown {
            strip: Some(report),
            findings: findings_of(&answered.report, path),
            checked: true,
            blocked: Some(tr(Msg::SaveBlocked)),
            ..Shown::default()
        }
    }
}

/// What the page shows of a refused check or save: the refusal over the
/// editor, and the path's error for a provisioned path.
fn refusal_shown(refusal: &Refusal) -> Shown {
    let (said, path_error) = match refusal.status() {
        Some(StatusCode::PRECONDITION_FAILED) => (Some(tr(Msg::SaveConflict)), None),
        Some(StatusCode::CONFLICT) => (
            Some(tr(Msg::SaveProvisionedPath)),
            Some(tr(Msg::ChooseOtherPath)),
        ),
        _ => (None, None),
    };
    let strip = match said {
        Some(said) => notice(Tone::Bad, Announce::Alert, None, said).into_any(),
        None => refused(refusal),
    };
    Shown {
        strip: Some(strip),
        path_error,
        ..Shown::default()
    }
}

/// The editor page over the file `opened`, after the post `edited` when one
/// was sent.
fn editor(opened: &Opening, edited: Option<&Edited>) -> AnyView {
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
        .map(|edited| shown(edited, &save_path))
        .unwrap_or_default();
    let header = heading(opened, shown.saved.as_deref());
    let lock = opened.read_only().then(|| {
        notice(
            Tone::Info,
            Announce::Status,
            None,
            tr(Msg::ProvisionedEditorNotice),
        )
        .into_any()
    });
    let path_card = (opened.is_new() || opened.read_only())
        .then(|| path_card(opened, &post.path, shown.path_error.clone()));
    let findings = FileFindings {
        file: save_path.clone(),
        href: None,
        lines: format!("#{}-L", crate::islands::DRAFT),
        current: None,
        findings: shown.findings.clone(),
        all: None,
    };
    let text = EditorText {
        label: tr_with(Msg::DraftOf, &[("name", short(&opened.path).into())]),
        lead: tr(if opened.path.starts_with("omocl/") {
            Msg::LeadOmocl
        } else {
            Msg::LeadFhirconnect
        }),
        facts: vec![tr(Msg::EditorFacts)],
        check: tr(Msg::Check),
        diagnostics: tr(Msg::TabDiagnostics),
        none: tr(if shown.checked {
            Msg::DraftNoDiagnostics
        } else {
            Msg::NotCheckedYet
        }),
    };
    let opened_text = if opened.is_new() {
        Opened::New
    } else {
        Opened::Text(opened.text.clone())
    };
    let island = view! {
        <MappingEditor
            draft
            opened=opened_text
            findings
            text
            words=EditorWords::of_page()
            rows=DiagnosticsWords::of_page()
        />
    }
    .into_any();
    let base = view! { <input type="hidden" name="base" value=opened.base.clone() /> }.into_any();
    let action = edit_href_of(&opened.path);
    let form = view! {
        <form id=FORM method="post" action=action class="fui-stack">
            {base}
            {path_card}
            {island}
        </form>
    }
    .into_any();
    let dock = dock(opened, &post, &shown);
    view! {
        {header}
        <div class="fui-stack">{lock} {shown.strip} {form}</div>
        {dock}
    }
    .into_any()
}

/// The title row of the editor of `opened`, saying the new digest once
/// saved: "New mapping file", "Edit", or "Check a draft" for a provisioned
/// file, its trail ending with the list a new file joins or the file.
fn heading(opened: &Opening, saved: Option<&str>) -> AnyView {
    if opened.is_new() {
        return editor_heading(
            message(Msg::EditorTitle),
            &Mode::Creating,
            Header {
                subtitle: Some(tr(Msg::NewFileSubtitle).into()),
                crumbs: vec![Crumb::new(tr(Msg::MappingSet), MAPPINGS)],
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
        Some(digest) => tr_with(Msg::EditSavedSubtitle, &[("digest", digest.into())]),
        None => tr_with(
            Msg::EditSubtitle,
            &[("digest", short_digest(&opened.base).into())],
        ),
    };
    editor_heading(
        title,
        &Mode::Editing(short(&opened.path).to_owned()),
        Header {
            subtitle: Some(subtitle.into()),
            crumbs: vec![
                Crumb::new(tr(Msg::MappingSet), format!("{MAPPINGS}/files")),
                Crumb::new(short(&opened.path).to_owned(), file_href(&opened.path)),
            ],
            ..Header::default()
        },
        ActionGroup::none(),
    )
}

/// The card that names where a new file, or a draft of a provisioned one,
/// saves.
fn path_card(opened: &Opening, value: &str, error: Option<String>) -> AnyView {
    let (title, label, help) = if opened.is_new() {
        (Msg::TheFile, Msg::Path, Msg::NewPathHelp)
    } else {
        (Msg::WhereToSave, Msg::NewPath, Msg::SaveAsHelp)
    };
    let invalid = error.is_some().then_some("true");
    let control = view! {
        <input
            id="editor-path"
            name="path"
            form=FORM
            class="fui-input fui-mono"
            value=value.to_owned()
            spellcheck="false"
            autocomplete="off"
            aria-invalid=invalid
        />
    };
    let error = error.map(|error| {
        view! {
            <p id=error_id("editor-path") class=ERROR>
                {error}
            </p>
        }
    });
    let shown = Field::new("editor-path", tr(label))
        .help(tr(help))
        .after(error)
        .view(control);
    card(tr(title), shown).into_any()
}

/// The docked bar: the reason, the bar's line, and Discard and Save, or the
/// confirmation's Go back and Confirm the save.
fn dock(opened: &Opening, post: &EditorPost, shown: &Shown) -> AnyView {
    let save_words = tr(if opened.is_new() {
        Msg::CreateAndReload
    } else if opened.read_only() {
        Msg::SaveUnderNewPath
    } else {
        Msg::SaveAndReload
    });
    let reason = DockField {
        id: String::from("editor-reason"),
        name: String::from("reason"),
        label: tr(Msg::Reason),
        value: post.reason.clone(),
        placeholder: tr(Msg::ReasonPlaceholder),
        shortcut: None,
        error: shown.reason_error.clone(),
        help: Some(tr(Msg::ReasonHelp)),
        disabled: false,
        focus: shown.reason_error.is_some(),
    };
    let decisions = if shown.confirm {
        vec![
            Decision {
                value: String::from("back"),
                label: tr(Msg::GoBack),
                glyph: Some(Glyph::ARROW_LEFT),
                ..Decision::default()
            },
            Decision {
                value: String::from("confirm"),
                label: tr(Msg::ConfirmTheSave),
                variant: Variant::Primary,
                glyph: Some(Glyph::SAVE),
                ..Decision::default()
            },
        ]
    } else if shown.saved.is_some() {
        vec![Decision {
            value: String::from("discard"),
            label: tr(Msg::Close),
            glyph: Some(Glyph::X),
            ..Decision::default()
        }]
    } else {
        vec![
            Decision {
                value: String::from("discard"),
                label: tr(if opened.is_new() {
                    Msg::Cancel
                } else {
                    Msg::Discard
                }),
                glyph: Some(Glyph::X),
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
        label: tr(Msg::SaveBar),
        form: DockForm::Page(String::from(FORM)),
        fields: vec![reason],
        line: shown.line.clone(),
        decisions,
    })
}

/// Why `reason` cannot go with a save, or `None` when it can: one line of
/// printable ASCII, at most [`MAX_REASON`] characters.
fn reason_fault(reason: &str) -> Option<Msg> {
    let reason = reason.trim();
    if reason.is_empty() {
        Some(Msg::ReasonMissing)
    } else if reason.chars().count() > MAX_REASON {
        Some(Msg::ReasonTooLong)
    } else if !reason.chars().all(|c| c.is_ascii_graphic() || c == ' ') {
        Some(Msg::ReasonCharacters)
    } else {
        None
    }
}

/// Why `path` cannot name a mapping file, or `None` when it can: plain
/// segments below `fhirconnect/` or `omocl/`, ending in `.yml`.
fn path_fault(path: &str) -> Option<Msg> {
    let segments = path.split('/').collect::<Vec<_>>();
    if path.is_empty() {
        Some(Msg::PathMissing)
    } else if segments
        .iter()
        .any(|segment| segment.is_empty() || *segment == "." || *segment == "..")
    {
        Some(Msg::PathSteps)
    } else if !matches!(segments.first(), Some(&("fhirconnect" | "omocl"))) || segments.len() < 2 {
        Some(Msg::PathRoot)
    } else if !std::path::Path::new(path)
        .extension()
        .is_some_and(|extension| extension == "yml" || extension == "yaml")
    {
        Some(Msg::PathExtension)
    } else {
        None
    }
}

/// What the bridge answers the post `post` of the editor of `path`, as the
/// operator whose access token is `token`.
pub async fn answer(
    state: &ConsoleState,
    token: &AccessToken,
    path: Option<&str>,
    post: &EditorPost,
) -> EditorAnswer {
    let named = post.path.trim();
    let Some(path) = path else {
        return EditorAnswer::Invalid {
            field: "path",
            said: Msg::PathMissing,
        };
    };
    if !named.is_empty()
        && let Some(said) = path_fault(named)
    {
        return EditorAnswer::Invalid {
            field: "path",
            said,
        };
    }
    match post.decision.as_str() {
        "check" => EditorAnswer::Checked(loaded(
            state
                .bridge()
                .check_mapping(path, post.draft.clone().into_bytes(), token)
                .await,
        )),
        "save" => match reason_fault(&post.reason) {
            Some(said) => EditorAnswer::Invalid {
                field: "reason",
                said,
            },
            None => EditorAnswer::Confirm,
        },
        "confirm" => {
            if let Some(said) = reason_fault(&post.reason) {
                return EditorAnswer::Invalid {
                    field: "reason",
                    said,
                };
            }
            // NOTE: RFC 9110 §13.1.2: a file named anew must not exist yet, and one
            // edited in place must still hold the digest it was opened at.
            let precondition = if post.base.is_empty() || !named.is_empty() {
                Precondition::Absent
            } else {
                Precondition::Match(post.base.clone())
            };
            EditorAnswer::Saved(loaded(
                state
                    .bridge()
                    .save_mapping(
                        path,
                        post.draft.clone().into_bytes(),
                        &precondition,
                        &post.reason,
                        token,
                    )
                    .await,
            ))
        }
        _ => EditorAnswer::Back,
    }
}

#[cfg(test)]
mod tests {
    use super::{path_fault, reason_fault};

    #[test]
    fn a_reason_is_one_line_of_plain_characters_and_not_too_long() {
        assert_eq!(None, reason_fault("Follow version 1.3.0 of the profile"));
        assert!(reason_fault("  ").is_some());
        assert!(reason_fault("tick \u{2713}").is_some());
        assert!(reason_fault(&"a".repeat(201)).is_some());
    }

    #[test]
    fn a_new_path_is_plain_segments_below_a_mapping_root() {
        assert_eq!(None, path_fault("fhirconnect/models/a.model.yml"));
        assert_eq!(None, path_fault("omocl/drug.yml"));
        assert!(path_fault("fhirconnect/../a.yml").is_some());
        assert!(path_fault("templates/a.yml").is_some());
        assert!(path_fault("fhirconnect/a.json").is_some());
        assert!(path_fault("").is_some());
    }
}
