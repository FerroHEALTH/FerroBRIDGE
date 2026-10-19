// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The editor's answers, drawn on the server: a draft check's verdict and
//! diagnostics, a save's result with the reload's report, and a reload's
//! report.

use std::path::Path;

use ferrohealth_ui::tone::Tone;
use ferrohealth_ui::view::notice::{Announce, notice};
use ferrohealth_ui::widget::editor::{LineMark, MarkKind, line_id};
use http::StatusCode;
use leptos::prelude::*;
use openehr_mapping_core::diagnostic::{Diagnostic, Severity};

use crate::bridge::BridgeError;
use crate::bridge::editing::Answered;
use crate::editor::{CheckView, DRAFT};
use crate::engine::render::parts;

/// The line of the draft at `path` that `diagnostic` names, when it is about
/// the draft and has a position.
fn draft_line(diagnostic: &Diagnostic, path: &str) -> Option<u32> {
    if diagnostic.file() != Path::new(path) {
        return None;
    }
    // NOTE: no specification governs this: our own design; a line past
    // `u32::MAX` names no line the editor draws, so it carries no link.
    u32::try_from(diagnostic.position()?.line()).ok()
}

/// The lines of the draft at `path` that the diagnostics of `answered` name.
fn marks(answered: &Answered, path: &str) -> Vec<LineMark> {
    answered
        .report
        .diagnostics
        .iter()
        .filter_map(|diagnostic| {
            let line = draft_line(diagnostic, path)?;
            let kind = match diagnostic.severity() {
                Severity::Error => MarkKind::Error,
                Severity::Warning => MarkKind::Warning,
            };
            Some(LineMark { line, kind })
        })
        .collect()
}

/// The verdict and the diagnostics of a report; a diagnostic of the draft at
/// `draft` links to its line in the editor.
fn report(
    heading: &'static str,
    verdict: &str,
    answered: &Answered,
    draft: Option<&str>,
) -> AnyView {
    let href = |diagnostic: &Diagnostic| {
        let line = draft_line(diagnostic, draft?)?;
        Some(format!("#{}", line_id(DRAFT, line)))
    };
    let diagnostics = parts::linked_diagnostics_table(&answered.report.diagnostics, &href);
    let (tone, announce) = if answered.status == StatusCode::OK {
        (Tone::Ok, Announce::Status)
    } else {
        (Tone::Bad, Announce::Alert)
    };
    view! {
        <section class="fui-card">
            <h2 class="fui-card-title">{heading}</h2>
            {notice(tone, announce, None, verdict.to_owned())}
            {diagnostics}
        </section>
    }
    .into_any()
}

/// A draft check's answer for the draft at `path`, with the lines its
/// diagnostics name.
#[must_use]
pub fn checked(answer: &Result<Answered, BridgeError>, path: &str) -> CheckView {
    let (section, marks) = match answer {
        Ok(answered) if answered.report.clean => (
            report(
                "Check",
                "The set compiles with this draft.",
                answered,
                Some(path),
            ),
            marks(answered, path),
        ),
        Ok(answered) => (
            report(
                "Check",
                "The set does not compile with this draft; a save would be refused.",
                answered,
                Some(path),
            ),
            marks(answered, path),
        ),
        Err(error) => (parts::failed("Check", error), Vec::new()),
    };
    CheckView {
        html: section.to_html(),
        marks,
    }
}

/// A save's answer for the draft at `path`, with the reload's report.
#[must_use]
pub fn saved(answer: &Result<Answered, BridgeError>, path: &str) -> String {
    let section = match answer {
        Ok(answered) if answered.status == StatusCode::OK => {
            let digest = answered
                .etag
                .clone()
                .unwrap_or_else(|| String::from("not stated"));
            let reported = report(
                "Saved",
                "Saved, and the set reloaded with it.",
                answered,
                Some(path),
            );
            view! {
                {reported}
                <p>"The file's SHA-256 is now " <code>{digest}</code> "."</p>
            }
            .into_any()
        }
        Ok(answered) => report(
            "Not saved",
            "The draft does not compile with the set, so nothing was written and the running set is unchanged.",
            answered,
            Some(path),
        ),
        Err(error) if error.refusal().is_some_and(|(status, _)| status == StatusCode::PRECONDITION_FAILED) => {
            view! {
                <section class="fui-card">
                    <h2 class="fui-card-title">"Not saved"</h2>
                    {notice(
                        Tone::Bad,
                        Announce::Alert,
                        None,
                        "412: the file changed since you opened it, so nothing was written. Open the file again to see the current version; your draft stays in the editor.",
                    )}
                </section>
            }
            .into_any()
        }
        Err(error) if error.refusal().is_some_and(|(status, _)| status == StatusCode::CONFLICT) => {
            view! {
                <section class="fui-card">
                    <h2 class="fui-card-title">"Not saved"</h2>
                    {notice(
                        Tone::Bad,
                        Announce::Alert,
                        None,
                        "409: a provisioned file holds this path, and the console never writes over one. Save the draft under a path of its own.",
                    )}
                </section>
            }
            .into_any()
        }
        Err(error) => parts::failed("Not saved", error),
    };
    section.to_html()
}

/// A reload's answer.
#[must_use]
pub fn reloaded(answer: &Result<Answered, BridgeError>) -> String {
    let section = match answer {
        Ok(answered) if answered.status == StatusCode::OK => report(
            "Reload",
            "The files on disk compiled and replaced the running set.",
            answered,
            None,
        ),
        Ok(answered) => report(
            "Reload",
            "The files on disk do not compile, so the running set is unchanged.",
            answered,
            None,
        ),
        Err(error) => parts::failed("Reload", error),
    };
    section.to_html()
}
