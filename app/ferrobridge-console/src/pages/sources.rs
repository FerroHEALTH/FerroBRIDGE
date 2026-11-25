// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The inbound sources: the list at `/sources`, the form of a new one at
//! `/sources/new`, and one source with its versions and its edit form at
//! `/sources/{id}`.
//!
//! The pages read `GET /operator/sources` and one source with its versions.
//! A source's document is the kind's JSON object, edited as text; it names
//! its secrets and never holds one. A save posts to its own page, which asks
//! the bridge under `If-Match` on the `ETag` the page showed and draws the
//! answer; a create answers `303` to the new source and a delete to the
//! list. No specification governs the console: our own design.

use axum::response::{IntoResponse as _, Redirect, Response};
use ferrobridge_operator_api::sources::{Kind, Source, SourceVersion, Sources, Versions, Write};
use ferrohealth_ui::highlight::Language;
use ferrohealth_ui::html::Html;
use ferrohealth_ui::i18n::Locale;
use ferrohealth_ui::icon::Glyph;
use ferrohealth_ui::table::view::{
    Body, Column, DataTable, Role, Shape, Sorting, cell, data_table, title_cell,
};
use ferrohealth_ui::tone::Tone;
use ferrohealth_ui::view::action::ActionGroup;
use ferrohealth_ui::view::action_menu::{Act, Action};
use ferrohealth_ui::view::button::Variant;
use ferrohealth_ui::view::editor::{Editor, code_editor};
use ferrohealth_ui::view::form::{Field, check};
use ferrohealth_ui::view::notice::{Announce, notice_text};
use ferrohealth_ui::view::state::{Mark, state};
use ferrohealth_ui::view::surface::{Container, Crumb, Header, definitions};
use ferrohealth_ui::view::template::detail::{DetailPage, detail_page};
use ferrohealth_ui::view::template::list::list_empty;

use crate::actions::field;
use crate::app::{Drawn, SOURCES};
use crate::bridge::sources::Tagged;
use crate::locale::messages::Msg;
use crate::locale::{tr, tr_with};
use crate::markup::{code, h, t};
use crate::model::{Loaded, Outcome, loaded};
use crate::pages::parts::{faulted, refused, refused_operator, section};
use crate::visit::Visit;

/// The address of the form of a new source.
const NEW: &str = "/sources/new";

/// The id and name of the document's text area.
const DOCUMENT: &str = "document";

/// The address of the source `id`.
fn source_href(id: &str) -> String {
    format!("{SOURCES}/{id}")
}

/// What a source form posts.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SourcePost {
    /// The name, as typed.
    pub name: String,
    /// The kind, as chosen.
    pub kind: String,
    /// Whether the enabled box was ticked.
    pub enabled: bool,
    /// The document, as typed.
    pub document: String,
    /// The `ETag` the page showed, on a change or a delete.
    pub etag: String,
    /// The submit button's `decision`: `save` or `delete`.
    pub decision: String,
}

impl SourcePost {
    /// The post the form body `body` carried; a CRLF the text area submits
    /// is read back as LF (HTML Living Standard §4.10.11).
    #[must_use]
    pub fn from_form(body: &str) -> Self {
        Self {
            name: field(body, "name").trim().to_owned(),
            kind: field(body, "kind"),
            enabled: field(body, "enabled") == "on",
            document: field(body, DOCUMENT).replace("\r\n", "\n"),
            etag: field(body, "etag"),
            decision: field(body, "decision"),
        }
    }

    /// The post a source stands at, for the edit form.
    fn of(source: &Source) -> Self {
        Self {
            name: source.name.clone(),
            kind: String::from(source.kind.as_str()),
            enabled: source.enabled,
            document: serde_json::to_string_pretty(&source.document)
                .unwrap_or_else(|_| source.document.to_string()),
            etag: String::new(),
            decision: String::new(),
        }
    }

    /// The write this post asks for, or the message of why it asks for
    /// none.
    fn write(&self) -> Result<Write, Msg> {
        let kind = kind_of(&self.kind).ok_or(Msg::SourceKindInvalid)?;
        if self.name.is_empty() {
            return Err(Msg::SourceNameMissing);
        }
        let document: serde_json::Value =
            serde_json::from_str(&self.document).map_err(|_unread| Msg::SourceDocumentInvalid)?;
        Ok(Write {
            name: self.name.clone(),
            kind,
            enabled: self.enabled,
            document,
        })
    }
}

/// What the bridge answered a post of a source form.
#[derive(Debug, Clone, PartialEq)]
pub enum SourceAnswer {
    /// The post asks for no write, for the reason the message names; the
    /// bridge was not asked.
    Invalid(Msg),
    /// The bridge saved the source, or refused.
    Saved(Loaded<Tagged>),
    /// The bridge refused the delete.
    Deleted(Loaded<()>),
}

/// The kind `text` names.
fn kind_of(text: &str) -> Option<Kind> {
    [Kind::Webhook, Kind::FhirSubscription, Kind::Mllp]
        .into_iter()
        .find(|kind| kind.as_str() == text)
}

/// The words of `kind`.
const fn kind_words(kind: Kind) -> Msg {
    match kind {
        Kind::Webhook => Msg::SourceKindWebhook,
        Kind::FhirSubscription => Msg::SourceKindFhirSubscription,
        Kind::Mllp => Msg::SourceKindMllp,
    }
}

/// The state of a source that is or is not served.
fn served(l: &Locale, enabled: bool) -> Html {
    if enabled {
        state(Mark::Ok, tr(l, Msg::SourceEnabled), None)
    } else {
        state(Mark::Neutral, tr(l, Msg::SourceDisabled), None)
    }
}

/// The list of every source.
pub async fn list_page(visit: &Visit) -> Drawn {
    let sources = match visit.reads.signed_in() {
        Ok((state, token)) => loaded(state.bridge().sources(token).await),
        Err(fault) => Err(fault),
    };
    Drawn::shell(visit.tr(Msg::Sources), listed(&visit.locale, &sources))
}

/// The list page over `sources`.
fn listed(l: &Locale, sources: &Loaded<Sources>) -> Html {
    let body = match sources {
        Ok(Outcome::Shown(sources)) if sources.sources.is_empty() => {
            list_empty(tr(l, Msg::SourcesNone), None)
        }
        Ok(Outcome::Shown(sources)) => Container::PanelSections.wrap(section(
            l,
            tr(l, Msg::Sources),
            Some(u64::try_from(sources.sources.len()).unwrap_or(u64::MAX)),
            table(l, &sources.sources),
        )),
        Ok(Outcome::Refused(refusal)) if refusal.not_served() => {
            list_empty(tr(l, Msg::SourcesUnavailable), None)
        }
        Ok(Outcome::Refused(refusal)) => refused_operator(l, refusal),
        Err(fault) => faulted(l, fault, SOURCES),
    };
    detail_page(
        l,
        DetailPage {
            header: Header {
                title: tr(l, Msg::Sources).into(),
                subtitle: Some(tr(l, Msg::SourcesIntro).into()),
                subtitle_view: None,
                crumbs: Vec::new(),
                mono: false,
                badge: None,
            },
            actions: ActionGroup::primary(Action::new(
                Some(Glyph::PLUS),
                tr(l, Msg::SourceNew),
                Act::Link(String::from(NEW)),
            )),
            stepper: None,
            tabs: None,
            body,
        },
    )
}

/// The table of `sources`.
fn table(l: &Locale, sources: &[Source]) -> Html {
    let columns = [
        Column::text("name", tr(l, Msg::ColName)).role(Role::Title),
        Column::text("state", tr(l, Msg::ColState)),
        Column::text("version", tr(l, Msg::ColVersion)).numeric(),
        Column::text("by", tr(l, Msg::ColUpdatedBy)),
        Column::text("at", tr(l, Msg::ColUpdatedAt)),
    ];
    let [_, served_column, version, by, at] = &columns;
    let rows = sources
        .iter()
        .map(|source| {
            h(format!(
                "<tr>{}{}{}{}{}</tr>",
                title_cell(
                    None,
                    Some(&source_href(&source.id)),
                    code(&source.name),
                    Some(t(tr(l, kind_words(source.kind)))),
                ),
                cell(served_column, served(l, source.enabled)),
                cell(
                    version,
                    t(tr_with(
                        l,
                        Msg::ValueCount,
                        &[("count", source.version.into())]
                    ))
                ),
                cell(by, code(&source.updated_by)),
                cell(at, code(&source.updated_at)),
            ))
        })
        .collect::<Html>();
    data_table(DataTable {
        shape: Shape::Stacked,
        caption: &tr(l, Msg::Sources),
        columns: &columns,
        sorting: Sorting::default(),
        body: Body::Rows(rows),
    })
}

/// The form of a new source, with the answer to the post `posted` when it
/// holds one.
pub fn new_page(visit: &Visit, posted: Option<(SourcePost, SourceAnswer)>) -> Drawn {
    let l = &visit.locale;
    let (post, answer) = posted.map_or_else(
        || {
            (
                SourcePost {
                    kind: String::from(Kind::Webhook.as_str()),
                    enabled: true,
                    document: String::from("{}"),
                    ..SourcePost::default()
                },
                None,
            )
        },
        |(post, answer)| (post, Some(answer)),
    );
    let shown = answer.as_ref().map(|answer| answered(l, answer));
    let body = Container::PanelSections.wrap(section(
        l,
        tr(l, Msg::SourceNew),
        None,
        form(l, NEW, &post, false),
    ));
    Drawn::shell(
        visit.tr(Msg::SourceNew),
        detail_page(
            l,
            DetailPage {
                header: Header {
                    title: tr(l, Msg::SourceNew).into(),
                    subtitle: None,
                    subtitle_view: None,
                    crumbs: vec![Crumb::new(tr(l, Msg::Sources), String::from(SOURCES))],
                    mono: false,
                    badge: None,
                },
                actions: ActionGroup::default(),
                stepper: None,
                tabs: None,
                body: shown.unwrap_or_else(Html::empty).then(body),
            },
        ),
    )
}

/// Creates the source the form body `body` describes for the operator of
/// `visit`: `303` to the new source, or the form again with the refusal.
pub async fn posted_new(visit: &Visit, body: &str) -> Response {
    let post = SourcePost::from_form(body);
    let answer = match post.write() {
        Err(said) => SourceAnswer::Invalid(said),
        Ok(write) => match visit.reads.signed_in() {
            Ok((state, token)) => {
                SourceAnswer::Saved(loaded(state.bridge().create_source(&write, token).await))
            }
            Err(fault) => SourceAnswer::Saved(Err(fault)),
        },
    };
    if let SourceAnswer::Saved(Ok(Outcome::Shown(created))) = &answer {
        return Redirect::to(&source_href(&created.source.id)).into_response();
    }
    crate::app::render(visit, new_page(visit, Some((post, answer)))).await
}

/// The source the route names, with its versions and its edit form.
pub async fn source_page(visit: &Visit, posted: Option<(SourcePost, SourceAnswer)>) -> Drawn {
    let Some(id) = visit.segment(1).map(str::to_owned) else {
        return crate::pages::errors::not_found(visit);
    };
    let (source, versions) = match visit.reads.signed_in() {
        Ok((state, token)) => (
            loaded(state.bridge().source(&id, token).await),
            loaded(state.bridge().source_versions(&id, token).await),
        ),
        Err(fault) => (Err(fault.clone()), Err(fault)),
    };
    if let Ok(Outcome::Refused(refusal)) = &source
        && refusal.not_served()
    {
        return crate::pages::errors::not_found(visit);
    }
    let title = match &source {
        Ok(Outcome::Shown(tagged)) => tagged.source.name.clone(),
        _ => id.clone(),
    };
    Drawn::shell(
        title.clone(),
        shown(&visit.locale, &id, &title, (&source, &versions), posted),
    )
}

/// The detail page of the source `id`, titled `title`.
fn shown(
    l: &Locale,
    id: &str,
    title: &str,
    (source, versions): (&Loaded<Tagged>, &Loaded<Versions>),
    posted: Option<(SourcePost, SourceAnswer)>,
) -> Html {
    let answer = posted.as_ref().map(|(_, answer)| answered(l, answer));
    let body = match source {
        Ok(Outcome::Shown(tagged)) => {
            let kept = posted
                .filter(|(_, answer)| !matches!(answer, SourceAnswer::Saved(Ok(Outcome::Shown(_)))))
                .map(|(post, _)| post);
            let mut post = kept.unwrap_or_else(|| SourcePost::of(&tagged.source));
            post.etag.clone_from(&tagged.etag);
            Container::PanelSections.wrap(
                section(l, tr(l, Msg::SourceFacts), None, facts(l, &tagged.source))
                    .then(section(
                        l,
                        tr(l, Msg::SourceEdit),
                        None,
                        form(l, &source_href(id), &post, true),
                    ))
                    .then(history(l, versions)),
            )
        }
        Ok(Outcome::Refused(refusal)) => refused(l, refusal),
        Err(fault) => faulted(l, fault, &source_href(id)),
    };
    let badge = match source {
        Ok(Outcome::Shown(tagged)) => Some(served(l, tagged.source.enabled)),
        _ => None,
    };
    detail_page(
        l,
        DetailPage {
            header: Header {
                title: title.to_owned().into(),
                subtitle: Some(tr(l, Msg::Source).into()),
                subtitle_view: None,
                crumbs: vec![Crumb::new(tr(l, Msg::Sources), String::from(SOURCES))],
                mono: true,
                badge,
            },
            actions: ActionGroup::default(),
            stepper: None,
            tabs: None,
            body: answer.unwrap_or_else(Html::empty).then(body),
        },
    )
}

/// The facts of `source`.
fn facts(l: &Locale, source: &Source) -> Html {
    definitions(vec![
        (tr(l, Msg::FactKind), t(tr(l, kind_words(source.kind)))),
        (
            tr(l, Msg::ColVersion),
            t(tr_with(
                l,
                Msg::ValueCount,
                &[("count", source.version.into())],
            )),
        ),
        (tr(l, Msg::ColUpdatedBy), code(&source.updated_by)),
        (tr(l, Msg::ColUpdatedAt), code(&source.updated_at)),
        (tr(l, Msg::FactId), code(&source.id)),
    ])
}

/// The section of every version of a source.
fn history(l: &Locale, versions: &Loaded<Versions>) -> Html {
    let body = match versions {
        Ok(Outcome::Shown(versions)) => version_table(l, &versions.versions),
        Ok(Outcome::Refused(refusal)) => refused(l, refusal),
        Err(fault) => faulted(l, fault, SOURCES),
    };
    let count = match versions {
        Ok(Outcome::Shown(versions)) => u64::try_from(versions.versions.len()).ok(),
        _ => None,
    };
    section(l, tr(l, Msg::SourceVersions), count, body)
}

/// The table of `versions`, newest first.
fn version_table(l: &Locale, versions: &[SourceVersion]) -> Html {
    let columns = [
        Column::text("version", tr(l, Msg::ColVersion)).role(Role::Title),
        Column::text("state", tr(l, Msg::ColState)),
        Column::text("by", tr(l, Msg::ColCommitter)),
        Column::text("at", tr(l, Msg::ColCommittedAt)),
    ];
    let [_, served_column, by, at] = &columns;
    let rows = versions
        .iter()
        .rev()
        .map(|version| {
            let shown = if version.deleted {
                state(Mark::Warn, tr(l, Msg::SourceDeleted), None)
            } else {
                served(l, version.enabled)
            };
            h(format!(
                "<tr>{}{}{}{}</tr>",
                title_cell(
                    None,
                    None,
                    t(tr_with(
                        l,
                        Msg::ValueCount,
                        &[("count", version.version.into())]
                    )),
                    Some(code(&version.name)),
                ),
                cell(served_column, shown),
                cell(by, code(&version.committer)),
                cell(at, code(&version.committed_at)),
            ))
        })
        .collect::<Html>();
    data_table(DataTable {
        shape: Shape::Stacked,
        caption: &tr(l, Msg::SourceVersions),
        columns: &columns,
        sorting: Sorting::default(),
        body: Body::Rows(rows),
    })
}

/// The form that posts `post` to `action`; with `existing`, the kind is
/// fixed, the `ETag` travels and the form offers Delete.
fn form(l: &Locale, action: &str, post: &SourcePost, existing: bool) -> Html {
    let name = Field::new("source-name", tr(l, Msg::ColName)).view(h(format!(
        r#"<input id="source-name" name="name" class="fui-input fui-mono" value="{}" required maxlength="128" spellcheck="false" autocomplete="off">"#,
        t(&post.name)
    )));
    let kinds = [Kind::Webhook, Kind::FhirSubscription, Kind::Mllp]
        .into_iter()
        .filter(|kind| !existing || kind.as_str() == post.kind)
        .map(|kind| {
            h(format!(
                r#"<option value="{value}"{selected}>{words}</option>"#,
                value = kind.as_str(),
                selected = if kind.as_str() == post.kind {
                    " selected"
                } else {
                    ""
                },
                words = t(tr(l, kind_words(kind))),
            ))
        })
        .collect::<Html>();
    let kind = Field::new("source-kind", tr(l, Msg::FactKind)).view(h(format!(
        r#"<select id="source-kind" name="kind" class="fui-select">{kinds}</select>"#
    )));
    let enabled = check(
        "source-enabled",
        tr(l, Msg::SourceEnabled),
        h(format!(
            r#"<input id="source-enabled" name="enabled" type="checkbox"{}>"#,
            if post.enabled { " checked" } else { "" },
        )),
    );
    let editor = code_editor(&Editor {
        id: DOCUMENT.into(),
        language: Language::Json,
        text: post.document.clone(),
        marks: Vec::new(),
        editable: true,
    });
    let document = h(format!(
        r#"<label for="{DOCUMENT}" class="fui-label">{words}</label><p class="fui-help">{help}</p>{editor}"#,
        words = t(tr(l, Msg::SourceDocument)),
        help = t(tr(l, Msg::SourceDocumentHelp)),
    ));
    let etag = if existing {
        h(format!(
            r#"<input type="hidden" name="etag" value="{}">"#,
            t(&post.etag)
        ))
    } else {
        Html::empty()
    };
    let delete = if existing {
        h(format!(
            r#"<button type="submit" name="decision" value="delete" class="{}" formnovalidate>{}{}</button>"#,
            Variant::Danger.class(),
            ferrohealth_ui::icon::icon(Glyph::TRASH, "16"),
            t(tr(l, Msg::SourceDelete)),
        ))
    } else {
        Html::empty()
    };
    let save = h(format!(
        r#"<button type="submit" name="decision" value="save" class="{}">{}{}</button>"#,
        Variant::Primary.class(),
        ferrohealth_ui::icon::icon(Glyph::SAVE, "16"),
        t(tr(l, Msg::SourceSave)),
    ));
    h(format!(
        r#"<form method="post" action="{action}" class="fui-stack">{etag}{name}{kind}{enabled}{document}<div class="fui-actions">{save}{delete}</div></form>"#,
        action = t(action),
    ))
}

/// The answer `answer` draws above the form it came from.
fn answered(l: &Locale, answer: &SourceAnswer) -> Html {
    match answer {
        SourceAnswer::Invalid(said) => notice_text(Tone::Bad, Announce::Alert, tr(l, *said)),
        SourceAnswer::Saved(Ok(Outcome::Shown(_))) => {
            notice_text(Tone::Ok, Announce::Status, tr(l, Msg::SourceSaved))
        }
        SourceAnswer::Saved(Ok(Outcome::Refused(refusal)))
        | SourceAnswer::Deleted(Ok(Outcome::Refused(refusal))) => refused(l, refusal),
        SourceAnswer::Deleted(Ok(Outcome::Shown(()))) => {
            notice_text(Tone::Ok, Announce::Status, tr(l, Msg::SourceDeletedNotice))
        }
        SourceAnswer::Saved(Err(fault)) | SourceAnswer::Deleted(Err(fault)) => {
            faulted(l, fault, SOURCES)
        }
    }
}

/// Saves or deletes the source the route names as the form body `body`
/// asks, for the operator of `visit`: the page again with the answer, or
/// `303` to the list once deleted.
pub async fn posted_source(visit: &Visit, body: &str) -> Response {
    let Some(id) = visit.segment(1).map(str::to_owned) else {
        return crate::app::render(visit, crate::pages::errors::not_found(visit)).await;
    };
    let post = SourcePost::from_form(body);
    let answer = match (post.decision.as_str(), visit.reads.signed_in()) {
        (_, Err(fault)) => SourceAnswer::Saved(Err(fault)),
        ("delete", Ok((state, token))) => SourceAnswer::Deleted(loaded(
            state.bridge().delete_source(&id, &post.etag, token).await,
        )),
        (_, Ok((state, token))) => match post.write() {
            Err(said) => SourceAnswer::Invalid(said),
            Ok(write) => SourceAnswer::Saved(loaded(
                state
                    .bridge()
                    .change_source(&id, &post.etag, &write, token)
                    .await,
            )),
        },
    };
    if let SourceAnswer::Deleted(Ok(Outcome::Shown(()))) = &answer {
        return Redirect::to(SOURCES).into_response();
    }
    let drawn = source_page(visit, Some((post, answer))).await;
    crate::app::render(visit, drawn).await
}
