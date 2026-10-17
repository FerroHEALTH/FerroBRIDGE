// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The editor's answers, drawn on the server: a draft check's verdict and
//! diagnostics, a save's result with the reload's report, and a reload's
//! report.

use http::StatusCode;
use leptos::prelude::*;

use crate::bridge::BridgeError;
use crate::bridge::editing::Answered;
use crate::engine::render::parts::{self, Shape};

/// The verdict and the diagnostics of a report.
fn report(heading: &'static str, verdict: &str, answered: &Answered) -> AnyView {
    let diagnostics = match parts::diagnostics(&answered.report, "diagnostics") {
        Ok(diagnostics) => parts::diagnostics_table(&diagnostics),
        Err(shape) => return parts::unreadable(heading, &shape),
    };
    let tone = if answered.status == StatusCode::OK {
        "pill tone-ok"
    } else {
        "pill tone-danger"
    };
    view! {
        <section class="card">
            <h2>{heading}</h2>
            <p>
                <span class=tone>{verdict.to_owned()}</span>
            </p>
            {diagnostics}
        </section>
    }
    .into_any()
}

/// Whether the report says the set compiled.
fn clean(answered: &Answered) -> Result<bool, Shape> {
    parts::field(&answered.report, "clean")?
        .as_bool()
        .ok_or(Shape { path: "clean" })
}

/// A draft check's answer.
#[must_use]
pub fn checked(answer: &Result<Answered, BridgeError>) -> String {
    let section = match answer {
        Ok(answered) => match clean(answered) {
            Ok(true) => report("Check", "The set compiles with this draft.", answered),
            Ok(false) => report(
                "Check",
                "The set does not compile with this draft; a save would be refused.",
                answered,
            ),
            Err(shape) => parts::unreadable("Check", &shape),
        },
        Err(error) => parts::failed("Check", error),
    };
    section.to_html()
}

/// A save's answer, with the reload's report.
#[must_use]
pub fn saved(answer: &Result<Answered, BridgeError>) -> String {
    let section = match answer {
        Ok(answered) if answered.status == StatusCode::OK => {
            let digest = answered
                .etag
                .clone()
                .unwrap_or_else(|| String::from("not stated"));
            let reported = report("Saved", "Saved, and the set reloaded with it.", answered);
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
        ),
        Err(error) if error.refusal().is_some_and(|(status, _)| status == StatusCode::PRECONDITION_FAILED) => {
            view! {
                <section class="card">
                    <h2>"Not saved"</h2>
                    <p role="alert">
                        "412: the file changed since you opened it, so nothing was written. Open the file again to see the current version; your draft stays in the editor."
                    </p>
                </section>
            }
            .into_any()
        }
        Err(error) if error.refusal().is_some_and(|(status, _)| status == StatusCode::CONFLICT) => {
            view! {
                <section class="card">
                    <h2>"Not saved"</h2>
                    <p role="alert">
                        "409: a provisioned file holds this path, and the console never writes over one. Save the draft under a path of its own."
                    </p>
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
        ),
        Ok(answered) => report(
            "Reload",
            "The files on disk do not compile, so the running set is unchanged.",
            answered,
        ),
        Err(error) => parts::failed("Reload", error),
    };
    section.to_html()
}
