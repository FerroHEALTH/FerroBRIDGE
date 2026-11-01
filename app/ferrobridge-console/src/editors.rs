// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The console's two uses of the library's code editor: the mapping editor
//! and the playground's input.
//!
//! The mapping editor is the library's code editor (B2) beside the
//! diagnostics of its draft (B3), whose caret and current line the
//! library's `editor` module keeps in step. It sits inside the editor
//! page's form, so its text area posts the draft as any form control does
//! (FerroUI `docs/consoles.md`, "An action that answers on its page"). No
//! specification governs the console: our own design.

use ferrohealth_ui::highlight::Language;
use ferrohealth_ui::html::Html;
use ferrohealth_ui::i18n::Locale;
use ferrohealth_ui::view::button::{Variant, label};
use ferrohealth_ui::view::diagnostics::{DiagnosticsWords, FileFindings, Position};
use ferrohealth_ui::view::editor::{
    Editor, EditorWords, LineMark, MarkKind, Opened, Panel, code_editor, editor_diagnostics,
    editor_panel, editor_split,
};
use ferrohealth_ui::view::surface::{SectionHead, panel_section, panel_sections_card};
use ferrohealth_ui::view::template::list::list_empty;

use crate::markup::{h, t};

/// The id and name of the editor's text area, which the page's form posts
/// as the draft.
pub const DRAFT: &str = "draft";

/// The words the mapping editor draws beside the library's own, in the
/// page's locale.
#[derive(Debug, Clone, PartialEq, Eq)]
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

/// Returns the mapping editor in `locale`: the code editor of `draft`
/// beside the diagnostics `findings` of its last check, measured against
/// `opened`, with the console's words `text`.
#[must_use]
pub fn mapping_editor(
    locale: &Locale,
    draft: String,
    opened: Opened,
    findings: &FileFindings,
    text: EditorText,
) -> Html {
    let field = Editor {
        id: DRAFT.into(),
        language: Language::Yaml,
        text: draft,
        marks: marks_of(findings),
        editable: true,
    };
    let check = h(format!(
        r#"<button type="submit" name="decision" value="check" class="{}">{}</button>"#,
        Variant::Secondary.small(),
        label(
            Some(ferrohealth_ui::icon::Glyph::LIST_CHECKS),
            text.check.clone()
        ),
    ));
    let caret = Position::default();
    let body = if findings.findings.is_empty() {
        list_empty(text.none.clone(), None)
    } else {
        editor_diagnostics(
            &text.diagnostics,
            findings,
            &DiagnosticsWords::of(locale),
            &field,
            caret,
        )
    };
    let panel = editor_panel(Panel {
        editor: field,
        label: text.label.clone(),
        lead: text.lead.clone(),
        facts: text.facts.clone(),
        opened,
        caret,
        tools: Some(check),
        end: None,
        words: EditorWords::of(locale),
    });
    let count = u64::try_from(findings.findings.len()).unwrap_or(u64::MAX);
    let side = panel_sections_card(
        text.diagnostics.clone(),
        panel_section(
            locale,
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

/// Returns the playground's input: the highlighted editor of the JSON the
/// operator runs an operation on, named `label`, holding `text`, what the
/// operator last posted.
#[must_use]
pub fn json_input(label: &str, text: String) -> Html {
    let editor = code_editor(&Editor {
        id: INPUT.into(),
        language: Language::Json,
        text,
        marks: Vec::new(),
        editable: true,
    });
    h(format!(
        r#"<label for="{INPUT}" class="fui-sr-only">{}</label>{editor}"#,
        t(label)
    ))
}
