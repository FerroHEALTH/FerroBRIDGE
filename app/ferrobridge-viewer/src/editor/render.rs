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
use crate::locale::{Msg, tr};

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
fn report(heading: &str, verdict: &str, answered: &Answered, draft: Option<&str>) -> AnyView {
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
                &tr(Msg::Check),
                &tr(Msg::SetCompilesDraft),
                answered,
                Some(path),
            ),
            marks(answered, path),
        ),
        Ok(answered) => (
            report(
                &tr(Msg::Check),
                &tr(Msg::SetDoesNotCompileDraftSave),
                answered,
                Some(path),
            ),
            marks(answered, path),
        ),
        Err(error) => (parts::failed(&tr(Msg::Check), error), Vec::new()),
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
            let digest = answered.etag.clone().unwrap_or_else(|| tr(Msg::NotStated));
            let reported = report(
                &tr(Msg::Saved),
                &tr(Msg::SavedSetReloaded),
                answered,
                Some(path),
            );
            view! {
                {reported}
                <p>{tr(Msg::FileSSha256Now)} <code>{digest}</code> "."</p>
            }
            .into_any()
        }
        Ok(answered) => report(
            &tr(Msg::NotSaved),
            &tr(Msg::DraftDoesNotCompileSetSo),
            answered,
            Some(path),
        ),
        Err(error)
            if error
                .refusal()
                .is_some_and(|(status, _)| status == StatusCode::PRECONDITION_FAILED) =>
        {
            view! {
                <section class="fui-card">
                    <h2 class="fui-card-title">{tr(Msg::NotSaved)}</h2>
                    {notice(
                        Tone::Bad,
                        Announce::Alert,
                        None,
                        tr(Msg::N412FileChangedSinceYouOpened),
                    )}
                </section>
            }
            .into_any()
        }
        Err(error)
            if error
                .refusal()
                .is_some_and(|(status, _)| status == StatusCode::CONFLICT) =>
        {
            view! {
                <section class="fui-card">
                    <h2 class="fui-card-title">{tr(Msg::NotSaved)}</h2>
                    {notice(
                        Tone::Bad,
                        Announce::Alert,
                        None,
                        tr(Msg::N409ProvisionedFileHoldsPathConsole),
                    )}
                </section>
            }
            .into_any()
        }
        Err(error) => parts::failed(&tr(Msg::NotSaved), error),
    };
    section.to_html()
}

/// A reload's answer.
#[must_use]
pub fn reloaded(answer: &Result<Answered, BridgeError>) -> String {
    let section = match answer {
        Ok(answered) if answered.status == StatusCode::OK => report(
            &tr(Msg::Reload),
            &tr(Msg::FilesDiskCompiledReplacedRunningSet),
            answered,
            None,
        ),
        Ok(answered) => report(
            &tr(Msg::Reload),
            &tr(Msg::FilesDiskDoNotCompileSo),
            answered,
            None,
        ),
        Err(error) => parts::failed(&tr(Msg::Reload), error),
    };
    section.to_html()
}
