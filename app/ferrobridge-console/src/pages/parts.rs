// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! What several pages draw alike.
//!
//! A file's diagnostics as a state, a file's owner and language as tags, a
//! load's diagnostics as the library's findings, a quiet line, a section of
//! a tab panel, and the console's fault or the bridge's refusal in the place
//! of what failed.

use std::collections::BTreeMap;

use ferrobridge_operator_api::operator::{Language, Owner};
use ferrohealth_ui::html::Html;
use ferrohealth_ui::i18n::{Locale, Phrase};
use ferrohealth_ui::icon::Glyph;
use ferrohealth_ui::view::code::{Gravity, LineDiagnostic};
use ferrohealth_ui::view::diagnostics::{FileFindings, Finding, Position};
use ferrohealth_ui::view::refusal::fault;
use ferrohealth_ui::view::state::{Mark, state};
use ferrohealth_ui::view::surface::{SectionHead, panel_section};
use ferrohealth_ui::view::tag::{Tag, tag};
use ferrohealth_ui::view::template::list::{HeadingCount, list_empty};
use openehr_mapping_core::diagnostic::{Diagnostic, Severity};

use crate::app::{EDIT, FILES};
use crate::locale::messages::Msg;
use crate::locale::{tr, tr_with};
use crate::markup::{code, h, t};
use crate::model::{Refusal, ViewError};

/// The last segment of the wire path `path`, a file's name.
#[must_use]
pub fn short(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

/// The page of the mapping file at the wire path `path`.
#[must_use]
pub fn file_href(path: &str) -> String {
    format!("{FILES}/{path}")
}

/// The editor of the mapping file at the wire path `path`.
#[must_use]
pub fn edit_href(path: &str) -> String {
    format!("{EDIT}/{path}")
}

/// Returns `text`, a message looked up with `Phrase::open` for each name in
/// `parts`, with each open placeable drawn as the markup `parts` gives it,
/// such as a bold word or a code link in a page subtitle.
#[must_use]
pub fn with_markup(text: &str, mut parts: Vec<(&str, Html)>) -> Html {
    let mut rest = text.to_owned();
    let mut drawn = Vec::new();
    loop {
        let next = parts
            .iter()
            .enumerate()
            .filter_map(|(at, (name, _))| {
                rest.find(&Phrase::open_mark(name)).map(|found| (found, at))
            })
            .min();
        let Some((found, at)) = next else {
            drawn.push(t(&rest));
            break;
        };
        let (name, part) = parts.swap_remove(at);
        let mut marked = rest.split_off(found);
        let after = marked.split_off(Phrase::open_mark(name).len().min(marked.len()));
        drawn.push(t(&rest));
        drawn.push(part);
        rest = after;
    }
    Html::join(drawn)
}

/// Returns the state of a file or a context with `errors` and `warnings`.
#[must_use]
pub fn diagnostics_state(l: &Locale, errors: usize, warnings: usize, clean: Msg) -> Html {
    if errors > 0 {
        state(
            Mark::Bad,
            tr_with(l, Msg::CountErrors, &[("count", errors.into())]),
            None,
        )
    } else if warnings > 0 {
        state(
            Mark::Warn,
            tr_with(l, Msg::CountWarnings, &[("count", warnings.into())]),
            None,
        )
    } else {
        state(Mark::Ok, tr(l, clean), None)
    }
}

/// The errors and the warnings among `diagnostics`.
#[must_use]
pub fn tally<'a>(diagnostics: impl IntoIterator<Item = &'a Diagnostic>) -> (usize, usize) {
    diagnostics
        .into_iter()
        .fold((0, 0), |(errors, warnings), diagnostic| {
            match diagnostic.severity() {
                Severity::Error => (errors.saturating_add(1), warnings),
                Severity::Warning => (errors, warnings.saturating_add(1)),
            }
        })
}

/// Returns the tag of a file's `owner`: the lock of a provisioned file the
/// console never writes (F4), or the console's own.
#[must_use]
pub fn owner_tag(l: &Locale, owner: Owner) -> Html {
    match owner {
        Owner::Provisioned => Tag::new(tr(l, Msg::OwnerProvisioned))
            .locked(tr(l, Msg::OwnerProvisionedTitle))
            .view(),
        Owner::Console => Tag::new(tr(l, Msg::OwnerConsole))
            .title(tr(l, Msg::OwnerConsoleTitle))
            .view(),
    }
}

/// The word of a file's `owner`.
#[must_use]
pub fn owner_word(l: &Locale, owner: Owner) -> String {
    tr(
        l,
        match owner {
            Owner::Provisioned => Msg::OwnerProvisioned,
            Owner::Console => Msg::OwnerConsole,
        },
    )
}

/// The name of a mapping `language`.
#[must_use]
pub const fn language_name(language: Language) -> &'static str {
    match language {
        Language::Fhirconnect => "FHIRconnect",
        Language::Omocl => "OMOCL",
    }
}

/// Returns the tag of a mapping `language`.
#[must_use]
pub fn language_tag(language: Language) -> Html {
    tag(language_name(language))
}

/// The gravity of a diagnostic of `severity`.
const fn gravity(severity: Severity) -> Gravity {
    match severity {
        Severity::Error => Gravity::Error,
        Severity::Warning => Gravity::Warning,
    }
}

/// The 1-based line of `diagnostic`, 0 for one about the whole file.
fn line_of(diagnostic: &Diagnostic) -> u32 {
    diagnostic.position().map_or(0, |position| {
        u32::try_from(position.line()).unwrap_or(u32::MAX)
    })
}

/// `diagnostics` as the library's findings, each with its place.
#[must_use]
pub fn findings(diagnostics: &[&Diagnostic]) -> Vec<Finding> {
    diagnostics
        .iter()
        .map(|diagnostic| Finding {
            gravity: gravity(diagnostic.severity()),
            code: diagnostic.code().to_string(),
            at: diagnostic.position().map(|position| Position {
                line: u32::try_from(position.line()).unwrap_or(u32::MAX),
                column: u32::try_from(position.column()).unwrap_or(u32::MAX),
            }),
            message: diagnostic.message().to_owned(),
            mapping: diagnostic.mapping_name().map(ToString::to_string),
        })
        .collect()
}

/// `diagnostics` as the code viewer draws them under their lines.
#[must_use]
pub fn line_diagnostics(diagnostics: &[&Diagnostic]) -> Vec<LineDiagnostic> {
    diagnostics
        .iter()
        .map(|diagnostic| LineDiagnostic {
            line: line_of(diagnostic),
            gravity: gravity(diagnostic.severity()),
            code: diagnostic.code().to_string(),
            message: diagnostic.message().to_owned(),
        })
        .collect()
}

/// The wire path a diagnostic is about.
#[must_use]
pub fn wire_path(diagnostic: &Diagnostic) -> String {
    diagnostic.file().to_string_lossy().into_owned()
}

/// `diagnostics` grouped by the file each is about, in path order, each
/// file's head leading to its page and each line link to its line there.
#[must_use]
pub fn by_file(diagnostics: &[Diagnostic]) -> Vec<FileFindings> {
    let mut files: BTreeMap<String, Vec<&Diagnostic>> = BTreeMap::new();
    for diagnostic in diagnostics {
        files
            .entry(wire_path(diagnostic))
            .or_default()
            .push(diagnostic);
    }
    files
        .into_iter()
        .map(|(file, found)| {
            let href = file_href(&file);
            FileFindings {
                lines: format!("{href}#L"),
                href: Some(href),
                file,
                current: None,
                findings: findings(&found),
                all: None,
            }
        })
        .collect()
}

/// Returns one part of a tab panel (B10): `title` with its `count`, over
/// `body`.
#[must_use]
pub fn section(l: &Locale, title: String, count: Option<u64>, body: Html) -> Html {
    panel_section(
        l,
        Some(SectionHead {
            title,
            count: count.map(HeadingCount::All),
            actions: None,
        }),
        body,
    )
}

/// Returns one quiet line: an empty, off or no-match state with a text link
/// at most.
#[must_use]
pub fn quiet(sentence: String, link: Option<(String, String)>) -> Html {
    list_empty(sentence, link)
}

/// Returns the console's fault `fault` in the place of what it kept from
/// the page, with the way to try again.
#[must_use]
pub fn faulted(l: &Locale, fault_of: &ViewError, again: &str) -> Html {
    h(format!(
        r#"<div class="fui-stack">{alert}<p><a href="{again}" class="fui-button fui-button-secondary">{glyph}{words}</a></p></div>"#,
        alert = fault(fault_of.text(l)),
        again = t(again),
        glyph = ferrohealth_ui::icon::icon(Glyph::REFRESH_CW, "16"),
        words = t(tr(l, Msg::TryAgain)),
    ))
}

/// Returns the bridge's refusal `refusal` in the place of what it refused.
#[must_use]
pub fn refused(l: &Locale, refusal: &Refusal) -> Html {
    let shown = ferrohealth_ui::view::refusal::Refusal {
        status: refusal.status().map(|status| status.as_u16()),
        summary: refusal.text(l),
        issues: Vec::new(),
        request: None,
        evidence: None,
    };
    ferrohealth_ui::view::refusal::refusal(l, &shown)
}

/// Returns the line that says the operator API is off on this bridge, with
/// the configuration key that turns it on.
#[must_use]
pub fn operator_off(l: &Locale) -> Html {
    h(format!(
        r#"<div class="fui-stack">{quiet}<p class="fui-small fui-muted">{key} · {hint}</p></div>"#,
        quiet = quiet(tr(l, Msg::OperatorOff), None),
        key = code("[operator] enabled = false"),
        hint = t(tr(l, Msg::OperatorOffHint)),
    ))
}

/// Returns what a page draws in place of a document the bridge refused:
/// the operator API's off line for its `404`, and the refusal otherwise.
#[must_use]
pub fn refused_operator(l: &Locale, refusal: &Refusal) -> Html {
    if refusal.not_served() {
        operator_off(l)
    } else {
        refused(l, refusal)
    }
}

/// The route segment `raw` with its percent-encoded octets decoded (RFC 3986
/// §2.1), or `None` for octets that are no UTF-8.
///
/// A path the browser encoded reaches the bridge client as the names it
/// holds, `..` among them, which the client refuses.
#[must_use]
pub fn decoded(raw: &str) -> Option<String> {
    let bytes = raw.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut at = 0;
    while let Some(&byte) = bytes.get(at) {
        let hex = |offset: usize| {
            bytes
                .get(at.saturating_add(offset))
                .and_then(|digit| char::from(*digit).to_digit(16))
        };
        if let (b'%', Some(high), Some(low)) = (byte, hex(1), hex(2)) {
            out.push(u8::try_from(high * 16 + low).ok()?);
            at = at.saturating_add(3);
        } else {
            out.push(byte);
            at = at.saturating_add(1);
        }
    }
    String::from_utf8(out).ok()
}
