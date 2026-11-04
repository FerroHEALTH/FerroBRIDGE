// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The mapping set as a ZIP: the export of a revision, a download at
//! `/mappings/revisions/{id}/export`, and the import page (T3) at
//! `/mappings/import`.
//!
//! The import page holds the archive's file field in a multipart form and
//! docks the reason with Check and save as a revision. The post sends the
//! archive to the bridge as it came, and the page draws the check: a set
//! that compiles is saved as a revision that does not run yet, linked to its
//! page where it is applied; one that does not compile shows its
//! diagnostics and nothing is saved.

use axum::Extension;
use axum::extract::Path;
use axum::response::{IntoResponse as _, Response};
use ferrohealth_ui::html::Html;
use ferrohealth_ui::i18n::Locale;
use ferrohealth_ui::icon::Glyph;
use ferrohealth_ui::tone::Tone;
use ferrohealth_ui::view::action::ActionGroup;
use ferrohealth_ui::view::button::Variant;
use ferrohealth_ui::view::diagnostics::diagnostics_by_file;
use ferrohealth_ui::view::dock::{Decision, DecisionBar, DockField, DockForm, decision_bar};
use ferrohealth_ui::view::form::Field;
use ferrohealth_ui::view::load_report;
use ferrohealth_ui::view::load_report::{Arrival, LoadReport};
use ferrohealth_ui::view::notice::{Announce, notice_text};
use ferrohealth_ui::view::surface::{Crumb, Header, card, card_action, page_heading};
use http::StatusCode;

use crate::actions::{ImportAnswer, ImportPost};
use crate::app::{Drawn, MAPPINGS};
use crate::locale::messages::Msg;
use crate::locale::{tr, tr_with};
use crate::markup::{h, t};
use crate::model::Outcome;
use crate::pages::mappings::revisions::revision_href;
use crate::pages::parts::{by_file, faulted, refused, tally};
use crate::server::ConsoleState;
use crate::visit::Visit;

/// The route of a revision's export.
pub const EXPORT: &str = "/mappings/revisions/{id}/export";

/// The address of the import page.
pub const IMPORT: &str = "/mappings/import";

/// The id of the import page's form.
const FORM: &str = "import-form";

/// The address of the export of the revision `id`.
#[must_use]
pub fn export_href(id: &str) -> String {
    EXPORT.replace("{id}", id)
}

/// The export of the revision the path names, as the bridge wrote it, for
/// the session the request holds.
pub async fn export(
    Extension(state): Extension<ConsoleState>,
    Extension(held): Extension<ferrohealth_session::record::Held>,
    Path(id): Path<String>,
) -> Response {
    let Some(token) = crate::server::session_token(&held) else {
        return StatusCode::UNAUTHORIZED.into_response();
    };
    if id.is_empty() || !id.chars().all(|character| character.is_ascii_digit()) {
        return StatusCode::NOT_FOUND.into_response();
    }
    match state.bridge().export_revision(&id, &token).await {
        Ok(bytes) => (
            [
                (
                    http::header::CONTENT_TYPE,
                    String::from(crate::bridge::revisions::ZIP),
                ),
                (
                    http::header::CONTENT_DISPOSITION,
                    format!("attachment; filename=\"ferrobridge-mappings-{id}.zip\""),
                ),
            ],
            bytes,
        )
            .into_response(),
        Err(error) => {
            tracing::warn!(error = %ferrohealth_server::run::chain(&error), "a revision could not be exported");
            StatusCode::BAD_GATEWAY.into_response()
        }
    }
}

/// The import page, with the answer to the import just posted when
/// `imported` holds one.
#[must_use]
pub fn page(visit: &Visit, imported: Option<&(ImportPost, ImportAnswer)>) -> Drawn {
    let l = &visit.locale;
    let heading = page_heading(
        l,
        Header {
            title: tr(l, Msg::ImportTitle).into(),
            subtitle: Some(tr(l, Msg::ImportSubtitle).into()),
            crumbs: vec![Crumb::new(tr(l, Msg::MappingSet), MAPPINGS)],
            ..Header::default()
        },
        ActionGroup::default(),
    );
    let input = h(format!(
        r#"<input id="import-archive" name="archive" type="file" accept=".zip,application/zip" class="fui-input" form="{FORM}" required>"#
    ));
    let field = Field::new("import-archive", tr(l, Msg::ImportArchive))
        .help(tr(l, Msg::ImportArchiveHelp))
        .view(input);
    let form = h(format!(
        r#"<form method="post" action="{IMPORT}" id="{FORM}" enctype="multipart/form-data"></form>"#
    ));
    let answer = imported
        .map(|(_, answer)| answer_view(l, answer))
        .unwrap_or_default();
    let body = heading.then(h(format!(
        r#"<div class="fui-stack">{form}{card}{answer}</div>"#,
        card = card(tr(l, Msg::ImportArchiveCard), field),
    )));
    Drawn::shell(visit.tr(Msg::ImportTitle), body).with_dock(dock(l, imported))
}

/// Imports the archive the multipart body `raw` carries for the operator of
/// `visit` and draws the page with the bridge's answer; Cancel goes back to
/// the mapping set.
pub async fn posted(visit: &Visit, raw: &[u8], content_type: Option<&str>) -> Response {
    let post = crate::posts::import_form(raw, content_type)
        .await
        .unwrap_or_default();
    if post.decision == "discard" {
        return axum::response::Redirect::to(MAPPINGS).into_response();
    }
    let answer = match visit.reads.signed_in() {
        Ok((state, token)) => crate::posts::imported(state, token, &post).await,
        Err(fault) => ImportAnswer::Answered(Err(fault)),
    };
    let drawn = page(visit, Some(&(post, answer)));
    crate::app::render(visit, drawn).await
}

/// The answer to an import, under the archive's field.
fn answer_view(l: &Locale, answer: &ImportAnswer) -> Html {
    match answer {
        ImportAnswer::Invalid(said) => notice_text(Tone::Bad, Announce::Alert, tr(l, *said)),
        ImportAnswer::Answered(Ok(Outcome::Shown(answered))) => {
            let report = &answered.report;
            let (errors, warnings) = tally(&report.diagnostics);
            let counts = super::counts(l, errors, warnings);
            if report.clean {
                let id = answered.etag.clone().unwrap_or_default();
                load_report::load_report(LoadReport {
                    outcome: load_report::Outcome::Ok,
                    arrival: Arrival::AfterAction,
                    title: t(tr_with(l, Msg::ImportSaved, &[("id", id.clone().into())])),
                    counts,
                    action: Some(card_action(revision_href(&id), tr(l, Msg::ImportOpen))),
                })
            } else {
                load_report::load_report(LoadReport {
                    outcome: load_report::Outcome::Bad,
                    arrival: Arrival::AfterAction,
                    title: t(tr(l, Msg::ImportRefused)),
                    counts,
                    action: None,
                })
                .then(diagnostics_by_file(
                    l,
                    &tr(l, Msg::ImportDiagnostics),
                    &by_file(&report.diagnostics),
                ))
            }
        }
        ImportAnswer::Answered(Ok(Outcome::Refused(refusal))) => refused(l, refusal),
        ImportAnswer::Answered(Err(fault)) => faulted(l, fault, IMPORT),
    }
}

/// The docked bar: the reason, Cancel and Check and save as a revision.
fn dock(l: &Locale, imported: Option<&(ImportPost, ImportAnswer)>) -> Html {
    let invalid = imported.and_then(|(_, answer)| match answer {
        ImportAnswer::Invalid(said) => Some(tr(l, *said)),
        ImportAnswer::Answered(_) => None,
    });
    let kept = imported
        .filter(|(_, answer)| !matches!(answer, ImportAnswer::Answered(Ok(Outcome::Shown(_)))))
        .map(|(post, _)| post.reason.clone())
        .unwrap_or_default();
    decision_bar(DecisionBar {
        id: String::from("import-dock"),
        label: tr(l, Msg::ImportBar),
        form: DockForm::Page(String::from(FORM)),
        fields: vec![DockField {
            id: String::from("import-reason"),
            name: String::from("reason"),
            label: tr(l, Msg::Reason),
            value: kept,
            placeholder: tr(l, Msg::ImportReasonPlaceholder),
            shortcut: None,
            focus: invalid.is_some(),
            error: invalid,
            help: Some(tr(l, Msg::ReasonHelp)),
            disabled: false,
        }],
        line: None,
        decisions: vec![
            Decision {
                value: String::from("discard"),
                label: tr(l, Msg::Cancel),
                glyph: Some(Glyph::X),
                leaves: true,
                ..Decision::default()
            },
            Decision {
                value: String::from("import"),
                label: tr(l, Msg::ImportSave),
                variant: Variant::Primary,
                glyph: Some(Glyph::UPLOAD),
                ..Decision::default()
            },
        ],
    })
}
