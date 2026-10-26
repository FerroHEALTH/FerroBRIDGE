// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The console's own components: the mapping editor and the playground's
//! input.
//!
//! Each draws the library's code editor with state it shares with the
//! console's own code (FerroUI `docs/consoles.md`, "What the console owns").
//!
//! The mapping editor is the library's code editor (B2) beside the
//! diagnostics of its draft (B3), whose caret and current line the two
//! share. It sits inside the editor page's form, so its text area posts the
//! draft with no WebAssembly. No specification governs the console: our own
//! design.
use ferrohealth_ui::highlight::Language;
use ferrohealth_ui::view::button::{Variant, label};
use ferrohealth_ui::view::diagnostics::{DiagnosticsWords, FileFindings, Position};
use ferrohealth_ui::view::surface::{SectionHead, panel_section, panel_sections_card};
use ferrohealth_ui::view::template::list::list_empty;
use ferrohealth_ui::widget::editor::{
    Editor, EditorWords, LineMark, MarkKind, Opened, Panel, editor_diagnostics, editor_panel,
    editor_split,
};
use leptos::prelude::*;
use serde::{Deserialize, Serialize};

/// The id and name of the editor's text area, which the page's form posts
/// as the draft.
pub const DRAFT: &str = "draft";

/// The words the mapping editor draws beside the library's own, resolved
/// on the server in the page's locale.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EditorText {
    /// The text area's accessible name.
    pub label: String,
    /// What the text is, at the toolbar's start.
    pub lead: String,
    /// The facts of the status line.
    pub facts: Vec<String>,
    /// The words of the toolbar's Check.
    pub check: String,
    /// The diagnostics section's title.
    pub diagnostics: String,
    /// What the diagnostics section says before a check or with no finding.
    pub none: String,
}

/// The marks of the lines `findings` name: an error over a warning.
fn marks_of(findings: &FileFindings) -> Vec<LineMark> {
    let mut marks: Vec<LineMark> = Vec::new();
    for finding in &findings.findings {
        let Some(at) = finding.at else {
            continue;
        };
        let kind = match finding.gravity {
            ferrohealth_ui::view::code::Gravity::Error => MarkKind::Error,
            ferrohealth_ui::view::code::Gravity::Warning => MarkKind::Warning,
        };
        match marks.iter_mut().find(|mark| mark.line == at.line) {
            Some(mark) if kind == MarkKind::Error => mark.kind = MarkKind::Error,
            Some(_) => {}
            None => marks.push(LineMark {
                line: at.line,
                kind,
            }),
        }
    }
    marks
}

/// The mapping editor: the code editor of `draft` beside the diagnostics
/// `findings` of its last check, in step with its caret.
#[component]
#[expect(
    clippy::needless_pass_by_value,
    reason = "a component takes its props by value"
)]
#[expect(
    clippy::must_use_candidate,
    reason = "the #[component] macro drops a #[must_use] from the function it writes"
)]
pub fn MappingEditor(
    /// The draft the text area holds.
    draft: String,
    /// What the draft is measured against.
    opened: Opened,
    /// The findings of the draft's last check, or of the file as loaded.
    findings: FileFindings,
    /// The console's words.
    text: EditorText,
    /// The code editor's own words.
    words: EditorWords,
    /// The diagnostics list's own words.
    rows: DiagnosticsWords,
) -> impl IntoView {
    let value = RwSignal::new(draft);
    let caret = RwSignal::new(Position::default());
    let marks = Signal::stored(marks_of(&findings));
    let field = Editor {
        id: DRAFT.into(),
        language: Language::Yaml,
        text: value.into(),
        marks,
        editable: Signal::stored(true),
    };
    let check = view! {
        <button type="submit" name="decision" value="check" class=Variant::Secondary.small()>
            {label(Some(ferrohealth_ui::icon::Glyph::LIST_CHECKS), text.check.clone())}
        </button>
    }
    .into_any();
    let panel = editor_panel(
        Panel {
            editor: field.clone(),
            label: text.label.clone(),
            lead: text.lead.clone(),
            facts: text.facts.clone(),
            opened,
            caret,
            tools: Some(check),
            end: None,
            words,
        },
        move |typed| value.set(typed),
    );
    let count = u64::try_from(findings.findings.len()).unwrap_or(u64::MAX);
    let body = if findings.findings.is_empty() {
        list_empty(text.none.clone(), None)
    } else {
        editor_diagnostics(&text.diagnostics, &findings, &rows, field, caret)
    };
    let side = panel_sections_card(
        text.diagnostics.clone(),
        panel_section(
            Some(SectionHead {
                title: text.diagnostics,
                count: Some(count.into()),
                actions: None,
            }),
            body,
        ),
    );
    editor_split(panel, side)
}

/// The id and name of the playground's input, which its form posts.
pub const INPUT: &str = "input";

/// The playground's input: the highlighted editor of the JSON the operator
/// runs an operation on, which the form around it posts.
#[component]
#[expect(
    clippy::must_use_candidate,
    reason = "the #[component] macro drops a #[must_use] from the function it writes"
)]
pub fn JsonInput(
    /// The text area's accessible name.
    label: String,
    /// The text it holds as the page arrives: what the operator last posted.
    text: String,
) -> impl IntoView {
    let value = RwSignal::new(text);
    let editor = ferrohealth_ui::widget::editor::code_editor(
        Editor {
            id: INPUT.into(),
            language: Language::Json,
            text: value.into(),
            marks: Signal::stored(Vec::new()),
            editable: Signal::stored(true),
        },
        move |typed| value.set(typed),
    );
    view! {
        <label for=INPUT class="fui-sr-only">
            {label}
        </label>
        {editor}
    }
}
