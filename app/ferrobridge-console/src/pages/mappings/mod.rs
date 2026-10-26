// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The Mapping set: what the bridge runs and the files it runs from, a list
//! page (T1) with binder tabs over one card.
//!
//! Its title row holds New file and More (Reload the set…), and the load
//! report (B4) under it says how the running set compiled, or how the
//! reload the operator just asked for went. The tabs are the contexts, the
//! OMOCL files, the templates, every file (with its find field, its
//! filters and its pager) and the running load's diagnostics, each a route.

use ferrobridge_operator_api::operator::{EngineState, Listed, Owner, Program, Template};
use ferrohealth_ui::icon::Glyph;
use ferrohealth_ui::table::view::{
    Body, Column, DataTable, Role, Sorting, cell, data_table, menu_cell, title_cell,
};
use ferrohealth_ui::view::action::ActionGroup;
use ferrohealth_ui::view::button::Variant;
use ferrohealth_ui::view::diagnostics::diagnostics_by_file;
use ferrohealth_ui::view::dialog::{Confirmation, confirm_dialog};
use ferrohealth_ui::view::load_report;
use ferrohealth_ui::view::load_report::{Arrival, Count, LoadReport};
use ferrohealth_ui::view::surface::{Header, page_heading};
use ferrohealth_ui::view::tabs::{Tab, TabRow};
use ferrohealth_ui::view::template::list::{ListCard, list_card};
use ferrohealth_ui::widget::action_menu::{Act, Action, Item, Menu, row_menu};
use leptos::prelude::*;
use leptos_router::hooks::{use_params_map, use_query_map};

use crate::actions::{ReloadSet, Reloaded};
use crate::app::{MAPPINGS, NEW_FILE, PLAYGROUND, TEMPLATES, page_title};
use crate::locale::messages::Msg;
use crate::locale::{tr, tr_with};
use crate::model::{Loaded, Outcome};
use crate::pages::mappings::files::files_body;
use crate::pages::parts::{
    by_file, diagnostics_state, edit_href, faulted, file_href, owner_tag, quiet, refused_operator,
    short, tally, wire_path,
};

mod files;

/// The id of the reload's confirmation dialog.
pub const RELOAD_DIALOG: &str = "reload-set";

/// The route the reload posts to: the Mapping set itself, which the post
/// draws again with the reload's outcome.
pub const RELOAD: &str = MAPPINGS;

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

/// The Mapping set, on the tab the route names.
#[must_use]
pub fn page() -> impl IntoView {
    let tab = use_params_map().with_untracked(|params| {
        params
            .get("tab")
            .unwrap_or_else(|| String::from("contexts"))
    });
    if !TABS.iter().any(|(key, _)| *key == tab) {
        return crate::pages::errors::not_found().into_any();
    }
    let query = use_query_map().get_untracked();
    let posted = (*SharedValue::new(use_context::<Reloaded>)).clone();
    let reload = ServerAction::<ReloadSet>::new();
    crate::load::follow(reload);
    let state = Resource::new(crate::load::changes, |_| crate::load::state_now());
    let listing = Resource::new(crate::load::changes, |_| crate::load::listing_now());
    view! {
        {page_title(tr(Msg::MappingSet))}
        <Suspense>
            {move || {
                let tab = tab.clone();
                let query = query.clone();
                let reloaded = reload
                    .value()
                    .get()
                    .map(|answer| answer.unwrap_or_else(|fault| Reloaded(Err(fault))))
                    .or_else(|| posted.clone());
                Suspend::new(async move {
                    let state = state.await;
                    let listing = listing.await;
                    drawn(&tab, &query, reloaded.as_ref(), &state, &listing, reload)
                })
            }}
        </Suspense>
    }
    .into_any()
}

/// The title row: New file and More with Reload the set…, which posts
/// through `reload` once the page has hydrated.
fn heading(reload: ServerAction<ReloadSet>) -> AnyView {
    let more = Menu::new(
        "set-more",
        tr(Msg::SetMoreLabel),
        vec![
            Item::new(Action::new(
                Some(Glyph::REFRESH_CW),
                tr(Msg::ReloadSet),
                Act::Dialog(String::from(RELOAD_DIALOG)),
            ))
            .note(tr(Msg::ReloadSetNote)),
        ],
    );
    let actions = ActionGroup::primary(Action::new(
        Some(Glyph::FILE_PLUS),
        tr(Msg::NewFile),
        Act::Link(String::from(NEW_FILE)),
    ))
    .more(more);
    let dialog = confirm_dialog(
        Confirmation {
            id: RELOAD_DIALOG.into(),
            title: tr(Msg::ReloadDialogTitle).into(),
            message: tr(Msg::ReloadDialogBody).into(),
        },
        view! {
            <form
                method="post"
                action=RELOAD
                class="fui-action-form"
                on:submit=move |event| {
                    event.prevent_default();
                    reload.dispatch(ReloadSet {});
                }
            >
                <button type="submit" class=Variant::Primary.class()>
                    {ferrohealth_ui::icon::icon(Glyph::REFRESH_CW, "16")}
                    {tr(Msg::ReloadSet)}
                </button>
            </form>
        },
    )
    .into_any();
    view! {
        {page_heading(
            Header {
                title: tr(Msg::MappingSet).into(),
                subtitle: Some(tr(Msg::MappingSetSubtitle).into()),
                ..Header::default()
            },
            actions,
        )}
        {dialog}
    }
    .into_any()
}

/// The counts of a load report for `errors` and `warnings`.
fn counts(errors: usize, warnings: usize) -> Vec<Count> {
    vec![
        Count {
            value: errors.to_string(),
            noun: tr_with(Msg::NounErrors, &[("count", errors.into())]),
        },
        Count {
            value: warnings.to_string(),
            noun: tr_with(Msg::NounWarnings, &[("count", warnings.into())]),
        },
    ]
}

/// The load report of the running set `state`.
fn running_report(state: &EngineState) -> AnyView {
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
        title: tr(title).into_any(),
        counts: counts(errors, warnings),
        action: None,
    })
}

/// The load report of the reload the operator just asked for, with its
/// diagnostics when it was refused.
fn reload_report(reloaded: &Reloaded) -> (AnyView, Option<AnyView>) {
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
                title: tr(title).into_any(),
                counts: counts(errors, warnings),
                action: None,
            });
            let findings = (!report.clean).then(|| {
                diagnostics_by_file(&tr(Msg::ReloadDiagnostics), &by_file(&report.diagnostics))
            });
            (shown, findings)
        }
        Ok(Outcome::Refused(refusal)) => (crate::pages::parts::refused(refusal), None),
        Err(fault) => (faulted(fault, MAPPINGS), None),
    }
}

/// The page over the running set and its files.
fn drawn(
    tab: &str,
    query: &leptos_router::params::ParamsMap,
    reloaded: Option<&Reloaded>,
    state: &Loaded<EngineState>,
    listing: &Loaded<Vec<Listed>>,
    reload: ServerAction<ReloadSet>,
) -> AnyView {
    let (state, listing) = match (state, listing) {
        (Ok(Outcome::Shown(state)), Ok(Outcome::Shown(listing))) => (state, listing),
        (Err(fault), _) | (_, Err(fault)) => {
            return view! { {heading(reload)} {faulted(fault, &tab_href(tab))} }.into_any();
        }
        (Ok(Outcome::Refused(refusal)), _) | (_, Ok(Outcome::Refused(refusal))) => {
            return view! { {heading(reload)} {refused_operator(refusal)} }.into_any();
        }
    };
    let (report, reload_findings) = match reloaded {
        Some(reloaded) => reload_report(reloaded),
        None => (running_report(state), None),
    };
    if listing.is_empty() {
        let empty = quiet(
            tr(Msg::SetEmpty),
            Some((tr(Msg::WriteFirstFile), String::from(NEW_FILE))),
        );
        return view! {
            {heading(reload)}
            <div class="fui-list-page">{report} {empty}</div>
        }
        .into_any();
    }
    let card = match reload_findings {
        Some(findings) => list_card(ListCard {
            title: tr(Msg::ReloadDiagnostics),
            count: None,
            order: None,
            tabs: Some(TabRow {
                label: tr(Msg::ReloadDiagnostics),
                tabs: vec![
                    Tab::new("reload", MAPPINGS, tr(Msg::ReloadDiagnostics)),
                    Tab::new("contexts", MAPPINGS, tr(Msg::RunningContexts))
                        .count(count(state.contexts.len()), tr(Msg::TabContexts)),
                ],
                current: String::from("reload"),
            }),
            actions: None,
            body: findings,
            pager: None,
        }),
        None => tab_card(tab, query, state, listing),
    };
    view! {
        {heading(reload)}
        <div class="fui-list-page">{report} {card}</div>
    }
    .into_any()
}

/// A count as the tabs carry it.
pub(super) fn count(n: usize) -> u64 {
    u64::try_from(n).unwrap_or(u64::MAX)
}

/// The binder tabs of the set, the one keyed `current` the page.
fn tab_row(current: &str, state: &EngineState, listing: &[Listed]) -> TabRow {
    let counts = [
        state.contexts.len(),
        state.omocl.files.len(),
        state.templates.len(),
        listing.len(),
        state.diagnostics.len(),
    ];
    TabRow {
        label: tr(Msg::MappingSet),
        tabs: TABS
            .iter()
            .zip(counts)
            .map(|((key, words), number)| {
                Tab::new(*key, tab_href(key), tr(*words)).count(count(number), tr(*words))
            })
            .collect(),
        current: current.to_owned(),
    }
}

/// The card of the tab `tab`: its table, list or diagnostics.
fn tab_card(
    tab: &str,
    query: &leptos_router::params::ParamsMap,
    state: &EngineState,
    listing: &[Listed],
) -> AnyView {
    let (body, pager) = match tab {
        "omocl" => (omocl_table(state, listing), None),
        "templates" => (templates_table(state), None),
        "files" => files_body(query, listing),
        "diagnostics" => (
            if state.diagnostics.is_empty() {
                quiet(tr(Msg::NoDiagnostics), None)
            } else {
                diagnostics_by_file(&tr(Msg::TabDiagnostics), &by_file(&state.diagnostics))
            },
            None,
        ),
        _ => (contexts_table(state, listing), None),
    };
    list_card(ListCard {
        title: tr(Msg::MappingSet),
        count: None,
        order: None,
        tabs: Some(tab_row(tab, state, listing)),
        actions: None,
        body,
        pager,
    })
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
fn context_menu(program: &Program, owner: Option<Owner>) -> AnyView {
    let mut items = Vec::new();
    if let Some(href) = program_href(program) {
        items.push(Item::new(Action::new(
            Some(Glyph::LAYERS),
            tr(Msg::OpenProgram),
            Act::Link(href),
        )));
    }
    items.push(Item::new(Action::new(
        Some(Glyph::FLASK_CONICAL),
        tr(Msg::TryInPlayground),
        Act::Link(String::from(PLAYGROUND)),
    )));
    if let Some(file) = program.file.as_deref() {
        let (glyph, words) = match owner {
            Some(Owner::Console) => (Glyph::PENCIL, Msg::Edit),
            _ => (Glyph::LIST_CHECKS, Msg::CheckDraft),
        };
        items.push(Item::new(Action::new(
            Some(glyph),
            tr(words),
            Act::Link(edit_href(file)),
        )));
    }
    let label = tr_with(Msg::ActionsFor, &[("name", context_name(program).into())]);
    row_menu(Menu::new(format!("menu-{}", program.context), label, items))
}

/// The contexts of the running set.
fn contexts_table(state: &EngineState, listing: &[Listed]) -> AnyView {
    let columns = [
        Column::text("context", tr(Msg::ColContext)).role(Role::Title),
        Column::text("resource", tr(Msg::ColResourceType)).role(Role::Sub),
        Column::text("template", tr(Msg::ColTemplate)).role(Role::Wide),
        Column::text("models", tr(Msg::ColModels))
            .numeric()
            .role(Role::Quiet),
        Column::text("search", tr(Msg::ColSearch)).role(Role::Wide),
        Column::text("owner", tr(Msg::ColOwner)).role(Role::Wide),
        Column::text("state", tr(Msg::ColState)),
        Column::text("menu", tr(Msg::ColActions)).role(Role::Menu),
    ];
    let [_, resource, template, models, search, owner, word, _] = &columns;
    if state.contexts.is_empty() {
        return quiet(
            tr(Msg::NoContexts),
            Some((tr(Msg::WriteFirstFile), String::from(NEW_FILE))),
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
            view! {
                <tr>
                    {title_cell(href.as_deref(), context_name(program), None)}
                    {cell(resource, program.resource_type.clone())}
                    {cell(template, view! { <code>{program.template.clone()}</code> })}
                    {cell(models, tr_with(Msg::ModelsCount, &[("count", program.models.len().into())]))}
                    {cell(search, if program.search.is_some() { tr(Msg::SearchDeclared) } else { String::from("–") })}
                    {cell(owner, file_owner.map(owner_tag))}
                    {cell(word, diagnostics_state(errors, warnings, Msg::StateCompiles))}
                    {menu_cell(context_menu(program, file_owner))}
                </tr>
            }
        })
        .collect_view()
        .into_any();
    data_table(DataTable {
        caption: &tr(Msg::TabContexts),
        columns: &columns,
        sorting: Sorting::default(),
        body: Body::Rows(rows),
    })
    .into_any()
}

/// The OMOCL files of the running set.
fn omocl_table(state: &EngineState, listing: &[Listed]) -> AnyView {
    if state.omocl.files.is_empty() {
        return quiet(tr(Msg::NoOmocl), None);
    }
    let columns = [
        Column::text("file", tr(Msg::ColFile)).role(Role::Title),
        Column::text("owner", tr(Msg::ColOwner)).role(Role::Wide),
        Column::text("state", tr(Msg::ColState)),
    ];
    let [_, owner, word] = &columns;
    let rows = state
        .omocl
        .files
        .iter()
        .map(|path| {
            let (errors, warnings) = file_tally(state, path);
            view! {
                <tr>
                    {title_cell(Some(&file_href(path)), short(path).to_owned(), None)}
                    {cell(owner, listed(listing, path).map(|file| owner_tag(file.owner)))}
                    {cell(word, diagnostics_state(errors, warnings, Msg::StateCompiles))}
                </tr>
            }
        })
        .collect_view()
        .into_any();
    data_table(DataTable {
        caption: &tr(Msg::TabOmocl),
        columns: &columns,
        sorting: Sorting::default(),
        body: Body::Rows(rows),
    })
    .into_any()
}

/// The id of a template, from either source.
#[must_use]
pub fn template_id(template: &Template) -> &str {
    match template {
        Template::Directory { id, .. } | Template::Cdr { id } => id,
    }
}

/// The templates the running set compiled against.
fn templates_table(state: &EngineState) -> AnyView {
    if state.templates.is_empty() {
        return quiet(tr(Msg::NoTemplates), None);
    }
    let columns = [
        Column::text("template", tr(Msg::ColTemplate))
            .mono()
            .role(Role::Title),
        Column::text("source", tr(Msg::ColSource)).role(Role::Sub),
        Column::text("contexts", tr(Msg::ColContexts))
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
            let from = tr(match template {
                Template::Directory { .. } => Msg::SourceDirectory,
                Template::Cdr { .. } => Msg::SourceCdr,
            });
            let href = format!("{TEMPLATES}/{id}");
            view! {
                <tr>
                    {title_cell(Some(&href), view! { <code>{id.clone()}</code> }, None)}
                    {cell(source, from)}
                    {cell(contexts, named.to_string())}
                </tr>
            }
        })
        .collect_view()
        .into_any();
    data_table(DataTable {
        caption: &tr(Msg::TabTemplates),
        columns: &columns,
        sorting: Sorting::default(),
        body: Body::Rows(rows),
    })
    .into_any()
}
