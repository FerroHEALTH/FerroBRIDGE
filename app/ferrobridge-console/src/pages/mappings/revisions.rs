// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The mapping set's revisions: the Revisions tab's history and one
//! revision, a detail page (T2) at `/mappings/revisions/{id}`.
//!
//! The history reads `GET /operator/revisions` and names each revision with
//! what recorded it, when, who and why, linked to its page. The page reads
//! the revision and its diff against the running one, draws every changed
//! file as a unified diff, lists the provisioned files as read-only (they
//! are in no revision), and docks Apply with its reason: the form posts to
//! the page with the running revision it showed, and the page draws the
//! bridge's answer in place.

use axum::response::Response;
use ferrobridge_operator_api::operator::{Listed, Owner};
use ferrobridge_operator_api::revisions::{
    Origin, Revision, RevisionDiff, RevisionSummary, Revisions,
};
use ferrohealth_ui::html::Html;
use ferrohealth_ui::i18n::Locale;
use ferrohealth_ui::icon::Glyph;
use ferrohealth_ui::table::change::{Change, change_table};
use ferrohealth_ui::table::view::{
    Body, Column, DataTable, Role, Shape, Sorting, cell, data_table, title_cell,
};
use ferrohealth_ui::tone::Tone;
use ferrohealth_ui::view::action::ActionGroup;
use ferrohealth_ui::view::action_menu::{Act, Action};
use ferrohealth_ui::view::button::Variant;
use ferrohealth_ui::view::diagnostics::diagnostics_by_file;
use ferrohealth_ui::view::dock::{Decision, DecisionBar, DockField, DockForm, decision_bar};
use ferrohealth_ui::view::line_diff::line_diff;
use ferrohealth_ui::view::load_report;
use ferrohealth_ui::view::load_report::{Arrival, LoadReport};
use ferrohealth_ui::view::notice::{Announce, notice_text};
use ferrohealth_ui::view::surface::{Container, Crumb, Header, definitions, section_intro};
use ferrohealth_ui::view::template::detail::{DetailPage, detail_page};

use crate::actions::{ApplyAnswer, ApplyPost};
use crate::app::{Drawn, MAPPINGS};
use crate::locale::messages::Msg;
use crate::locale::{tr, tr_with};
use crate::markup::{code, h, t};
use crate::model::{Loaded, Outcome};
use crate::pages::parts::{by_file, faulted, file_href, owner_tag, refused, section, short, tally};
use crate::visit::Visit;
use ferrohealth_ui::view::template::list::list_empty;

/// The id of the form the dock's Apply submits.
const FORM: &str = "revision-apply";

/// The address of the revision `id`.
#[must_use]
pub fn revision_href(id: &str) -> String {
    format!("{MAPPINGS}/revisions/{id}")
}

/// The words of `origin`.
const fn origin_words(origin: Origin) -> Msg {
    match origin {
        Origin::Baseline => Msg::OriginBaseline,
        Origin::Save => Msg::OriginSave,
        Origin::Reload => Msg::OriginReload,
        Origin::Apply => Msg::OriginApply,
        Origin::Import => Msg::OriginImport,
    }
}

/// The words of a revision's standing: running, ran before, or never
/// applied.
fn standing(running: Option<&str>, id: &str, applied: bool) -> Msg {
    if running == Some(id) {
        Msg::RevisionRunning
    } else if applied {
        Msg::RevisionRanBefore
    } else {
        Msg::RevisionNotApplied
    }
}

/// A count as a section carries it.
fn number(n: usize) -> u64 {
    u64::try_from(n).unwrap_or(u64::MAX)
}

/// The Revisions tab over the history `revisions`.
pub(super) fn listing(l: &Locale, revisions: &Loaded<Revisions>) -> Html {
    let revisions = match revisions {
        Ok(Outcome::Shown(revisions)) => revisions,
        Ok(Outcome::Refused(refusal))
            if refusal.status() == Some(http::StatusCode::CONFLICT) || refusal.not_served() =>
        {
            return list_empty(tr(l, Msg::RevisionsUnavailable), None);
        }
        Ok(Outcome::Refused(refusal)) => return refused(l, refusal),
        Err(fault) => return faulted(l, fault, &super::tab_href("revisions")),
    };
    if revisions.revisions.is_empty() {
        return list_empty(tr(l, Msg::RevisionsNone), None);
    }
    revision_table(l, revisions.running.as_deref(), &revisions.revisions)
}

/// The table of `revisions`, the one `running` names marked.
fn revision_table(l: &Locale, running: Option<&str>, revisions: &[RevisionSummary]) -> Html {
    let columns = [
        Column::text("revision", tr(l, Msg::ColRevision)).role(Role::Title),
        Column::text("recorded", tr(l, Msg::ColRecorded)),
        Column::text("author", tr(l, Msg::ColAuthor)).role(Role::Wide),
        Column::text("reason", tr(l, Msg::ColReason)).role(Role::Wide),
        Column::text("files", tr(l, Msg::ColFiles)).numeric(),
        Column::text("state", tr(l, Msg::ColState)),
    ];
    let [_, recorded, author, reason, files, state] = &columns;
    let rows = revisions
        .iter()
        .map(|revision| {
            h(format!(
                "<tr>{}{}{}{}{}{}</tr>",
                title_cell(
                    None,
                    Some(&revision_href(&revision.id)),
                    code(&revision.id),
                    Some(t(tr(l, origin_words(revision.origin)))),
                ),
                cell(recorded, code(&revision.time)),
                cell(author, code(&revision.author)),
                cell(reason, t(&revision.reason)),
                cell(
                    files,
                    t(tr_with(
                        l,
                        Msg::ValueCount,
                        &[("count", revision.files.into())]
                    ))
                ),
                cell(
                    state,
                    t(tr(l, standing(running, &revision.id, revision.applied)))
                ),
            ))
        })
        .collect::<Html>();
    data_table(DataTable {
        shape: Shape::Stacked,
        caption: &tr(l, Msg::TabRevisions),
        columns: &columns,
        sorting: Sorting::default(),
        body: Body::Rows(rows),
    })
}

/// The revision the route names, with the answer to the apply just posted
/// when `applied` holds one.
pub async fn page(visit: &Visit, applied: Option<(ApplyPost, ApplyAnswer)>) -> Drawn {
    let Some(id) = visit.segment(2).map(str::to_owned) else {
        return crate::pages::errors::not_found(visit);
    };
    let revision = visit.reads.revision(&id).await;
    if let Ok(Outcome::Refused(refusal)) = &revision
        && refusal.not_served()
    {
        return crate::pages::errors::not_found(visit);
    }
    let diff = visit.reads.revision_diff(&id).await;
    let listing = visit.reads.listing().await;
    let l = &visit.locale;
    let title = tr_with(l, Msg::RevisionTitle, &[("id", id.clone().into())]);
    let body = match (&revision, &diff) {
        (Ok(Outcome::Shown(revision)), Ok(Outcome::Shown(diff))) => {
            let answer = applied.as_ref().map(|(_, answer)| answer_view(l, answer));
            Some((revision, diff, answer))
        }
        _ => None,
    };
    let Some((revision, diff, answer)) = body else {
        let shown = match (&revision, &diff) {
            (Err(fault), _) | (_, Err(fault)) => faulted(l, fault, &revision_href(&id)),
            (Ok(Outcome::Refused(refusal)), _) | (_, Ok(Outcome::Refused(refusal))) => {
                refused(l, refusal)
            }
            (Ok(Outcome::Shown(_)), Ok(Outcome::Shown(_))) => Html::default(),
        };
        return Drawn::shell(title.clone(), frame(l, &title, None, shown));
    };
    let content = sections(l, revision, diff, &listing);
    let content = match answer {
        Some(answer) => answer.then(content),
        None => content,
    };
    let running = diff.from.as_deref();
    let drawn = Drawn::shell(title.clone(), frame(l, &title, Some(revision), content));
    if running == Some(revision.id.as_str()) && applied.is_none() {
        return drawn;
    }
    let post = applied.map(|(post, answer)| (post, Some(answer)));
    drawn.with_dock(dock(
        l,
        &revision.id,
        revision.applied,
        running,
        post.as_ref(),
    ))
}

/// Applies the revision the route names for the operator of `visit` and
/// draws its page with the bridge's answer.
pub async fn posted(visit: &Visit, body: &str) -> Response {
    let post = ApplyPost::from_form(body);
    let id = visit.segment(2).unwrap_or_default().to_owned();
    let answer = match visit.reads.signed_in() {
        Ok((state, token)) => crate::posts::applied(state, token, &id, &post).await,
        Err(fault) => ApplyAnswer::Answered(Err(fault)),
    };
    let drawn = page(visit, Some((post, answer))).await;
    crate::app::render(visit, drawn).await
}

/// The detail page around `body`, titled `title`.
fn frame(l: &Locale, title: &str, revision: Option<&Revision>, body: Html) -> Html {
    detail_page(
        l,
        DetailPage {
            header: Header {
                title: title.to_owned().into(),
                subtitle: Some(
                    revision
                        .map_or_else(
                            || tr(l, Msg::RevisionSubtitle),
                            |revision| tr(l, origin_words(revision.origin)),
                        )
                        .into(),
                ),
                subtitle_view: None,
                crumbs: vec![Crumb::new(
                    tr(l, Msg::TabRevisions),
                    super::tab_href("revisions"),
                )],
                mono: false,
                badge: None,
            },
            actions: revision.map_or_else(ActionGroup::default, |revision| {
                ActionGroup::primary(Action::new(
                    Some(Glyph::DOWNLOAD),
                    tr(l, Msg::ExportRevision),
                    Act::Download(super::archive::export_href(&revision.id)),
                ))
            }),
            stepper: None,
            tabs: None,
            body,
        },
    )
}

/// The facts, the changes against the running revision and the provisioned
/// files.
fn sections(
    l: &Locale,
    revision: &Revision,
    diff: &RevisionDiff,
    listing: &Loaded<Vec<Listed>>,
) -> Html {
    let dash = || t("–");
    let running = diff.from.as_deref();
    let facts = definitions(vec![
        (
            tr(l, Msg::ColState),
            t(tr(l, standing(running, &revision.id, revision.applied))),
        ),
        (tr(l, Msg::ColRecorded), code(&revision.time)),
        (tr(l, Msg::ColAuthor), code(&revision.author)),
        (tr(l, Msg::ColReason), t(&revision.reason)),
        (
            tr(l, Msg::FactParent),
            revision.parent.as_deref().map_or_else(dash, code),
        ),
        (
            tr(l, Msg::FactRestores),
            revision.source.as_deref().map_or_else(dash, code),
        ),
        (
            tr(l, Msg::ColFiles),
            t(tr_with(
                l,
                Msg::ValueCount,
                &[("count", revision.files.len().into())],
            )),
        ),
    ]);
    let changes = if diff.files.is_empty() {
        list_empty(tr(l, Msg::RevisionNoChanges), None)
    } else {
        let short =
            |digest: Option<&String>| digest.map(|digest| digest.chars().take(12).collect());
        change_table(
            l,
            tr(l, Msg::RevisionChanges),
            Some(tr(l, Msg::ColFile)),
            &diff
                .files
                .iter()
                .map(|file| Change {
                    key: file.path.clone(),
                    from: short(file.from_sha256.as_ref()),
                    to: short(file.to_sha256.as_ref()),
                })
                .collect::<Vec<_>>(),
        )
    };
    let diffs = diff
        .files
        .iter()
        .map(|file| {
            section(
                l,
                file.path.clone(),
                None,
                line_diff(l, &file.path, &file.unified),
            )
        })
        .collect::<Html>();
    Container::PanelSections.wrap(
        section(l, tr(l, Msg::RevisionFacts), None, facts)
            .then(section(
                l,
                tr(l, Msg::RevisionChanges),
                Some(number(diff.files.len())),
                changes,
            ))
            .then(diffs)
            .then(provisioned(l, listing)),
    )
}

/// The provisioned files, read-only and in no revision.
fn provisioned(l: &Locale, listing: &Loaded<Vec<Listed>>) -> Html {
    let Ok(Outcome::Shown(listing)) = listing else {
        return Html::default();
    };
    let files = listing
        .iter()
        .filter(|file| file.owner == Owner::Provisioned)
        .collect::<Vec<_>>();
    let body = if files.is_empty() {
        list_empty(tr(l, Msg::RevisionNoProvisioned), None)
    } else {
        let note = section_intro(tr(l, Msg::RevisionProvisionedNote));
        let columns = [
            Column::text("file", tr(l, Msg::ColFile)).role(Role::Title),
            Column::text("owner", tr(l, Msg::ColOwner)).role(Role::Wide),
        ];
        let [_, owner] = &columns;
        let rows = files
            .iter()
            .map(|file| {
                h(format!(
                    "<tr>{}{}</tr>",
                    title_cell(
                        None,
                        Some(&file_href(&file.path)),
                        t(short(&file.path)),
                        Some(code(&file.path)),
                    ),
                    cell(owner, owner_tag(l, Owner::Provisioned)),
                ))
            })
            .collect::<Html>();
        note.then(data_table(DataTable {
            shape: Shape::Stacked,
            caption: &tr(l, Msg::RevisionProvisioned),
            columns: &columns,
            sorting: Sorting::default(),
            body: Body::Rows(rows),
        }))
    };
    section(
        l,
        tr(l, Msg::RevisionProvisioned),
        Some(number(files.len())),
        body,
    )
}

/// The answer to an apply, in the place it was asked from.
fn answer_view(l: &Locale, answer: &ApplyAnswer) -> Html {
    match answer {
        ApplyAnswer::Invalid(said) => notice_text(Tone::Bad, Announce::Alert, tr(l, *said)),
        ApplyAnswer::Answered(Ok(Outcome::Shown(answered))) => {
            let report = &answered.report;
            let (errors, warnings) = tally(&report.diagnostics);
            let (outcome, title) = if report.clean {
                (
                    load_report::Outcome::Ok,
                    tr_with(
                        l,
                        Msg::ApplyDone,
                        &[("id", answered.etag.clone().unwrap_or_default().into())],
                    ),
                )
            } else {
                (load_report::Outcome::Bad, tr(l, Msg::ApplyRefused))
            };
            let shown = load_report::load_report(LoadReport {
                outcome,
                arrival: Arrival::AfterAction,
                title: t(title),
                counts: super::counts(l, errors, warnings),
                action: None,
            });
            if report.clean {
                shown
            } else {
                shown.then(diagnostics_by_file(
                    l,
                    &tr(l, Msg::ApplyDiagnostics),
                    &by_file(&report.diagnostics),
                ))
            }
        }
        ApplyAnswer::Answered(Ok(Outcome::Refused(refusal))) => refused(l, refusal),
        ApplyAnswer::Answered(Err(fault)) => faulted(l, fault, MAPPINGS),
    }
}

/// The docked bar of the revision `id`: the reason and Apply, whose form
/// carries the running revision the page showed.
fn dock(
    l: &Locale,
    id: &str,
    applied: bool,
    running: Option<&str>,
    post: Option<&(ApplyPost, Option<ApplyAnswer>)>,
) -> Html {
    let invalid = post.and_then(|(_, answer)| match answer {
        Some(ApplyAnswer::Invalid(said)) => Some(tr(l, *said)),
        _ => None,
    });
    let kept = post
        .filter(|(_, answer)| !matches!(answer, Some(ApplyAnswer::Answered(Ok(Outcome::Shown(_))))))
        .map(|(post, _)| post.reason.clone())
        .unwrap_or_default();
    let form = h(format!(
        r#"<form method="post" action="{action}" id="{FORM}"><input type="hidden" name="running" value="{running}"></form>"#,
        action = t(revision_href(id)),
        running = t(running.unwrap_or_default()),
    ));
    let reason = DockField {
        id: String::from("apply-reason"),
        name: String::from("reason"),
        label: tr(l, Msg::Reason),
        value: kept,
        placeholder: tr(l, Msg::ApplyReasonPlaceholder),
        shortcut: None,
        focus: invalid.is_some(),
        error: invalid,
        help: Some(tr(l, Msg::ReasonHelp)),
        disabled: false,
    };
    let bar = decision_bar(DecisionBar {
        id: String::from("apply-dock"),
        label: tr(l, Msg::ApplyBar),
        form: DockForm::Page(String::from(FORM)),
        fields: vec![reason],
        line: None,
        decisions: vec![Decision {
            value: String::from("apply"),
            label: tr(
                l,
                if applied {
                    Msg::RollBack
                } else {
                    Msg::ApplyRevision
                },
            ),
            variant: Variant::Primary,
            glyph: Some(Glyph::REFRESH_CW),
            ..Decision::default()
        }],
    });
    form.then(bar)
}
