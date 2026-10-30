// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The Mapping set: what the bridge runs and the files it runs from, a list
//! page (T1) with binder tabs over one card.
//!
//! Its title row holds New file and More (Reload the set…), and the load
//! report (B4) under it says how the running set compiled, or how the
//! reload the operator just asked for went: Reload posts to the page, which
//! draws the reload's report in its place. The tabs are the contexts, the
//! OMOCL files, the templates, every file (with its find field, its
//! filters and its pager) and the running load's diagnostics, each a route.

use axum::response::Response;
use ferrobridge_operator_api::operator::{EngineState, Listed, Owner, Program, Template};
use ferrohealth_ui::html::Html;
use ferrohealth_ui::i18n::Locale;
use ferrohealth_ui::icon::Glyph;
use ferrohealth_ui::query::Query;
use ferrohealth_ui::table::view::{
    Body, Column, DataTable, Role, Sorting, cell, data_table, menu_cell, title_cell,
};
use ferrohealth_ui::view::action::ActionGroup;
use ferrohealth_ui::view::action_menu::{Act, Action, Item, Menu, row_menu};
use ferrohealth_ui::view::button::Variant;
use ferrohealth_ui::view::diagnostics::diagnostics_by_file;
use ferrohealth_ui::view::dialog::{Confirmation, confirm_dialog};
use ferrohealth_ui::view::load_report;
use ferrohealth_ui::view::load_report::{Arrival, Count, LoadReport};
use ferrohealth_ui::view::surface::{Header, page_heading};
use ferrohealth_ui::view::tabs::{Tab, TabRow};
use ferrohealth_ui::view::template::list::{ListCard, list_card};

use crate::actions::Reloaded;
use crate::app::{Drawn, MAPPINGS, NEW_FILE, PLAYGROUND, TEMPLATES};
use crate::locale::messages::Msg;
use crate::locale::{tr, tr_with};
use crate::markup::{code, h, t};
use crate::model::{Loaded, Outcome};
use crate::pages::mappings::files::files_body;
use crate::pages::parts::{
    by_file, diagnostics_state, edit_href, faulted, file_href, owner_tag, quiet, refused_operator,
    short, tally, wire_path,
};
use crate::visit::Visit;

mod files;

/// The id of the reload's confirmation dialog.
pub const RELOAD_DIALOG: &str = "reload-set";

/// The tabs of the mapping set, each its key and its words.
const TABS: [(&str, Msg); 5] = [
    ("contexts", Msg::TabContexts),
    ("omocl", Msg::TabOmocl),
    ("templates", Msg::TabTemplates),
    ("files", Msg::TabFiles),
    ("diagnostics", Msg::TabDiagnostics),
];

/// The page of the tab `key`.
pub(super) fn tab_href(key: &str) -> String {
    if key == "contexts" {
        String::from(MAPPINGS)
    } else {
        format!("{MAPPINGS}/{key}")
    }
}

/// The Mapping set, on the tab the route names, with the report of the
/// reload the operator just asked for when `reloaded` holds one.
pub async fn page(visit: &Visit, reloaded: Option<Reloaded>) -> Drawn {
    let tab = visit.segment(1).unwrap_or("contexts").to_owned();
    if !TABS.iter().any(|(key, _)| *key == tab) {
        return crate::pages::errors::not_found(visit);
    }
    let state = visit.reads.running().await;
    let listing = visit.reads.listing().await;
    let body = drawn(
        &visit.locale,
        &tab,
        &visit.query,
        reloaded.as_ref(),
        &state,
        &listing,
    );
    Drawn::shell(visit.tr(Msg::MappingSet), body)
}

/// Reloads the set for the operator of `visit` and draws the page with its
/// report.
pub async fn reload(visit: &Visit) -> Response {
    let reloaded = match visit.reads.signed_in() {
        Ok((state, token)) => crate::posts::reloaded(state, token).await,
        Err(fault) => Reloaded(Err(fault)),
    };
    let drawn = page(visit, Some(reloaded)).await;
    crate::app::render(visit, drawn).await
}

/// The title row: New file and More with Reload the set…, whose dialog's
/// form posts to the page.
fn heading(l: &Locale, here: &str) -> Html {
    let more = Menu::new(
        "set-more",
        tr(l, Msg::SetMoreLabel),
        vec![
            Item::new(Action::new(
                Some(Glyph::REFRESH_CW),
                tr(l, Msg::ReloadSet),
                Act::Dialog(String::from(RELOAD_DIALOG)),
            ))
            .note(tr(l, Msg::ReloadSetNote)),
        ],
    );
    let actions = ActionGroup::primary(Action::new(
        Some(Glyph::FILE_PLUS),
        tr(l, Msg::NewFile),
        Act::Link(String::from(NEW_FILE)),
    ))
    .more(more);
    let dialog = confirm_dialog(
        l,
        Confirmation {
            id: RELOAD_DIALOG.into(),
            title: tr(l, Msg::ReloadDialogTitle).into(),
            message: tr(l, Msg::ReloadDialogBody).into(),
        },
        h(format!(
            r#"<form method="post" action="{here}" class="fui-action-form"><button type="submit" class="{class}">{glyph}{words}</button></form>"#,
            here = t(here),
            class = Variant::Primary.class(),
            glyph = ferrohealth_ui::icon::icon(Glyph::REFRESH_CW, "16"),
            words = t(tr(l, Msg::ReloadSet)),
        )),
    );
    page_heading(
        l,
        Header {
            title: tr(l, Msg::MappingSet).into(),
            subtitle: Some(tr(l, Msg::MappingSetSubtitle).into()),
            ..Header::default()
        },
        actions,
    )
    .then(dialog)
}

/// The counts of a load report for `errors` and `warnings`.
fn counts(l: &Locale, errors: usize, warnings: usize) -> Vec<Count> {
    vec![
        Count {
            value: errors.to_string(),
            noun: tr_with(l, Msg::NounErrors, &[("count", errors.into())]),
        },
        Count {
            value: warnings.to_string(),
            noun: tr_with(l, Msg::NounWarnings, &[("count", warnings.into())]),
        },
    ]
}

/// The load report of the running set `state`.
fn running_report(l: &Locale, state: &EngineState) -> Html {
    let (errors, warnings) = tally(&state.diagnostics);
    let (outcome, title) = if errors > 0 {
        (load_report::Outcome::Bad, Msg::RunningNotCompiles)
    } else if warnings > 0 {
        (load_report::Outcome::Warn, Msg::RunningCompilesWarnings)
    } else {
        (load_report::Outcome::Ok, Msg::RunningCompiles)
    };
    load_report::load_report(LoadReport {
        outcome,
        arrival: Arrival::WithPage,
        title: t(tr(l, title)),
        counts: counts(l, errors, warnings),
        action: None,
    })
}

/// The load report of the reload the operator just asked for, with its
/// diagnostics when it was refused.
fn reload_report(l: &Locale, reloaded: &Reloaded) -> (Html, Option<Html>) {
    match &reloaded.0 {
        Ok(Outcome::Shown(report)) => {
            let (errors, warnings) = tally(&report.diagnostics);
            let (outcome, title) = if report.clean {
                (load_report::Outcome::Ok, Msg::Reloaded)
            } else {
                (load_report::Outcome::Bad, Msg::ReloadRefused)
            };
            let shown = load_report::load_report(LoadReport {
                outcome,
                arrival: Arrival::AfterAction,
                title: t(tr(l, title)),
                counts: counts(l, errors, warnings),
                action: None,
            });
            let findings = (!report.clean).then(|| {
                diagnostics_by_file(
                    l,
                    &tr(l, Msg::ReloadDiagnostics),
                    &by_file(&report.diagnostics),
                )
            });
            (shown, findings)
        }
        Ok(Outcome::Refused(refusal)) => (crate::pages::parts::refused(l, refusal), None),
        Err(fault) => (faulted(l, fault, MAPPINGS), None),
    }
}

/// The page over the running set and its files.
fn drawn(
    l: &Locale,
    tab: &str,
    query: &Query,
    reloaded: Option<&Reloaded>,
    state: &Loaded<EngineState>,
    listing: &Loaded<Vec<Listed>>,
) -> Html {
    let here = tab_href(tab);
    let heading = heading(l, &here);
    let (state, listing) = match (state, listing) {
        (Ok(Outcome::Shown(state)), Ok(Outcome::Shown(listing))) => (state, listing),
        (Err(fault), _) | (_, Err(fault)) => {
            return heading.then(faulted(l, fault, &here));
        }
        (Ok(Outcome::Refused(refusal)), _) | (_, Ok(Outcome::Refused(refusal))) => {
            return heading.then(refused_operator(l, refusal));
        }
    };
    let (report, reload_findings) = match reloaded {
        Some(reloaded) => reload_report(l, reloaded),
        None => (running_report(l, state), None),
    };
    if listing.is_empty() {
        let empty = quiet(
            tr(l, Msg::SetEmpty),
            Some((tr(l, Msg::WriteFirstFile), String::from(NEW_FILE))),
        );
        return heading.then(h(format!(
            r#"<div class="fui-list-page">{report}{empty}</div>"#
        )));
    }
    let card = match reload_findings {
        Some(findings) => list_card(
            l,
            ListCard {
                title: tr(l, Msg::ReloadDiagnostics),
                count: None,
                order: None,
                tabs: Some(TabRow {
                    label: tr(l, Msg::ReloadDiagnostics),
                    tabs: vec![
                        Tab::new("reload", MAPPINGS, tr(l, Msg::ReloadDiagnostics)),
                        Tab::new("contexts", MAPPINGS, tr(l, Msg::RunningContexts))
                            .count(count(state.contexts.len()), tr(l, Msg::TabContexts)),
                    ],
                    current: String::from("reload"),
                }),
                actions: None,
                body: findings,
                pager: None,
            },
        ),
        None => tab_card(l, tab, query, state, listing),
    };
    heading.then(h(format!(
        r#"<div class="fui-list-page">{report}{card}</div>"#
    )))
}

/// A count as the tabs carry it.
pub(super) fn count(n: usize) -> u64 {
    u64::try_from(n).unwrap_or(u64::MAX)
}

/// The binder tabs of the set, the one keyed `current` the page.
fn tab_row(l: &Locale, current: &str, state: &EngineState, listing: &[Listed]) -> TabRow {
    let counts = [
        state.contexts.len(),
        state.omocl.files.len(),
        state.templates.len(),
        listing.len(),
        state.diagnostics.len(),
    ];
    TabRow {
        label: tr(l, Msg::MappingSet),
        tabs: TABS
            .iter()
            .zip(counts)
            .map(|((key, words), number)| {
                Tab::new(*key, tab_href(key), tr(l, *words)).count(count(number), tr(l, *words))
            })
            .collect(),
        current: current.to_owned(),
    }
}

/// The card of the tab `tab`: its table, list or diagnostics.
fn tab_card(l: &Locale, tab: &str, query: &Query, state: &EngineState, listing: &[Listed]) -> Html {
    let (body, pager) = match tab {
        "omocl" => (omocl_table(l, state, listing), None),
        "templates" => (templates_table(l, state), None),
        "files" => files_body(l, query, listing),
        "diagnostics" => (
            if state.diagnostics.is_empty() {
                quiet(tr(l, Msg::NoDiagnostics), None)
            } else {
                diagnostics_by_file(l, &tr(l, Msg::TabDiagnostics), &by_file(&state.diagnostics))
            },
            None,
        ),
        _ => (contexts_table(l, state, listing), None),
    };
    list_card(
        l,
        ListCard {
            title: tr(l, Msg::MappingSet),
            count: None,
            order: None,
            tabs: Some(tab_row(l, tab, state, listing)),
            actions: None,
            body,
            pager,
        },
    )
}

/// The listed file at `path`, when the listing holds it.
fn listed<'a>(listing: &'a [Listed], path: &str) -> Option<&'a Listed> {
    listing.iter().find(|file| file.path == path)
}

/// The errors and warnings the running load raised about `path`.
fn file_tally(state: &EngineState, path: &str) -> (usize, usize) {
    tally(
        state
            .diagnostics
            .iter()
            .filter(|diagnostic| wire_path(diagnostic) == path),
    )
}

/// The name a context is listed by: its mapping name.
fn context_name(program: &Program) -> String {
    program.context.clone()
}

/// The page that opens a context: its file's Program tab.
#[must_use]
pub fn program_href(program: &Program) -> Option<String> {
    program
        .file
        .as_deref()
        .map(|file| format!("{}?tab=program", file_href(file)))
}

/// The row menu of a context: its program, the playground, and Edit or
/// Check a draft by the file's owner.
fn context_menu(l: &Locale, program: &Program, owner: Option<Owner>) -> Html {
    let mut items = Vec::new();
    if let Some(href) = program_href(program) {
        items.push(Item::new(Action::new(
            Some(Glyph::LAYERS),
            tr(l, Msg::OpenProgram),
            Act::Link(href),
        )));
    }
    items.push(Item::new(Action::new(
        Some(Glyph::FLASK_CONICAL),
        tr(l, Msg::TryInPlayground),
        Act::Link(String::from(PLAYGROUND)),
    )));
    if let Some(file) = program.file.as_deref() {
        let (glyph, words) = match owner {
            Some(Owner::Console) => (Glyph::PENCIL, Msg::Edit),
            _ => (Glyph::LIST_CHECKS, Msg::CheckDraft),
        };
        items.push(Item::new(Action::new(
            Some(glyph),
            tr(l, words),
            Act::Link(edit_href(file)),
        )));
    }
    let label = tr_with(
        l,
        Msg::ActionsFor,
        &[("name", context_name(program).into())],
    );
    row_menu(
        l,
        &Menu::new(format!("menu-{}", program.context), label, items),
    )
}

/// The contexts of the running set.
fn contexts_table(l: &Locale, state: &EngineState, listing: &[Listed]) -> Html {
    let columns = [
        Column::text("context", tr(l, Msg::ColContext)).role(Role::Title),
        Column::text("resource", tr(l, Msg::ColResourceType)).role(Role::Sub),
        Column::text("template", tr(l, Msg::ColTemplate)).role(Role::Wide),
        Column::text("models", tr(l, Msg::ColModels))
            .numeric()
            .role(Role::Quiet),
        Column::text("search", tr(l, Msg::ColSearch)).role(Role::Wide),
        Column::text("owner", tr(l, Msg::ColOwner)).role(Role::Wide),
        Column::text("state", tr(l, Msg::ColState)),
        Column::text("menu", tr(l, Msg::ColActions)).role(Role::Menu),
    ];
    let [_, resource, template, models, search, owner, word, _] = &columns;
    if state.contexts.is_empty() {
        return quiet(
            tr(l, Msg::NoContexts),
            Some((tr(l, Msg::WriteFirstFile), String::from(NEW_FILE))),
        );
    }
    let mut programs = state.contexts.iter().collect::<Vec<_>>();
    programs.sort_by(|a, b| a.context.cmp(&b.context));
    let rows = programs
        .into_iter()
        .map(|program| {
            let file_owner = program
                .file
                .as_deref()
                .and_then(|file| listed(listing, file))
                .map(|file| file.owner);
            let (errors, warnings) = program
                .file
                .as_deref()
                .map_or((0, 0), |file| file_tally(state, file));
            let href = program_href(program);
            h(format!(
                "<tr>{}{}{}{}{}{}{}{}</tr>",
                title_cell(href.as_deref(), t(context_name(program)), None),
                cell(resource, t(&program.resource_type)),
                cell(template, code(&program.template)),
                cell(
                    models,
                    t(tr_with(
                        l,
                        Msg::ModelsCount,
                        &[("count", program.models.len().into())]
                    ))
                ),
                cell(
                    search,
                    t(if program.search.is_some() {
                        tr(l, Msg::SearchDeclared)
                    } else {
                        String::from("–")
                    })
                ),
                cell(
                    owner,
                    file_owner
                        .map(|owner| owner_tag(l, owner))
                        .unwrap_or_default()
                ),
                cell(
                    word,
                    diagnostics_state(l, errors, warnings, Msg::StateCompiles)
                ),
                menu_cell(context_menu(l, program, file_owner)),
            ))
        })
        .collect::<Html>();
    data_table(DataTable {
        caption: &tr(l, Msg::TabContexts),
        columns: &columns,
        sorting: Sorting::default(),
        body: Body::Rows(rows),
    })
}

/// The OMOCL files of the running set.
fn omocl_table(l: &Locale, state: &EngineState, listing: &[Listed]) -> Html {
    if state.omocl.files.is_empty() {
        return quiet(tr(l, Msg::NoOmocl), None);
    }
    let columns = [
        Column::text("file", tr(l, Msg::ColFile)).role(Role::Title),
        Column::text("owner", tr(l, Msg::ColOwner)).role(Role::Wide),
        Column::text("state", tr(l, Msg::ColState)),
    ];
    let [_, owner, word] = &columns;
    let rows = state
        .omocl
        .files
        .iter()
        .map(|path| {
            let (errors, warnings) = file_tally(state, path);
            h(format!(
                "<tr>{}{}{}</tr>",
                title_cell(Some(&file_href(path)), t(short(path)), None),
                cell(
                    owner,
                    listed(listing, path)
                        .map(|file| owner_tag(l, file.owner))
                        .unwrap_or_default()
                ),
                cell(
                    word,
                    diagnostics_state(l, errors, warnings, Msg::StateCompiles)
                ),
            ))
        })
        .collect::<Html>();
    data_table(DataTable {
        caption: &tr(l, Msg::TabOmocl),
        columns: &columns,
        sorting: Sorting::default(),
        body: Body::Rows(rows),
    })
}

/// The id of a template, from either source.
#[must_use]
pub fn template_id(template: &Template) -> &str {
    match template {
        Template::Directory { id, .. } | Template::Cdr { id } => id,
    }
}

/// The templates the running set compiled against.
fn templates_table(l: &Locale, state: &EngineState) -> Html {
    if state.templates.is_empty() {
        return quiet(tr(l, Msg::NoTemplates), None);
    }
    let columns = [
        Column::text("template", tr(l, Msg::ColTemplate))
            .mono()
            .role(Role::Title),
        Column::text("source", tr(l, Msg::ColSource)).role(Role::Sub),
        Column::text("contexts", tr(l, Msg::ColContexts))
            .numeric()
            .role(Role::Quiet),
    ];
    let [_, source, contexts] = &columns;
    let rows = state
        .templates
        .iter()
        .map(|template| {
            let id = template_id(template).to_owned();
            let named = state
                .contexts
                .iter()
                .filter(|program| program.template == id)
                .count();
            let from = tr(
                l,
                match template {
                    Template::Directory { .. } => Msg::SourceDirectory,
                    Template::Cdr { .. } => Msg::SourceCdr,
                },
            );
            let href = format!("{TEMPLATES}/{id}");
            h(format!(
                "<tr>{}{}{}</tr>",
                title_cell(Some(&href), code(&id), None),
                cell(source, t(from)),
                cell(contexts, t(named.to_string())),
            ))
        })
        .collect::<Html>();
    data_table(DataTable {
        caption: &tr(l, Msg::TabTemplates),
        columns: &columns,
        sorting: Sorting::default(),
        body: Body::Rows(rows),
    })
}
