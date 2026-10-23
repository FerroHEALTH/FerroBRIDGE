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

use std::borrow::Cow;

use ferrobridge_operator_api::operator::{EngineState, Language, Listed, Owner, Program, Template};
use ferrohealth_ui::icon::Glyph;
use ferrohealth_ui::table::filter::{Find, find_form};
use ferrohealth_ui::table::model::{Paging, StateSource, Window};
use ferrohealth_ui::table::pager::{Listing, link_pager};
use ferrohealth_ui::table::view::{
    Body, Column, DataTable, Role, Sorting, cell, data_table, menu_cell, title_cell,
};
use ferrohealth_ui::view::action::ActionGroup;
use ferrohealth_ui::view::button::Variant;
use ferrohealth_ui::view::diagnostics::diagnostics_by_file;
use ferrohealth_ui::view::dialog::{Confirmation, confirm_dialog};
use ferrohealth_ui::view::load_report::{
    Arrival, Count, LoadReport, Outcome as Verdict, load_report,
};
use ferrohealth_ui::view::surface::{Header, page_heading};
use ferrohealth_ui::view::tabs::{Tab, TabRow};
use ferrohealth_ui::view::template::list::{
    ActiveFilters, FilterChip, ListCard, active_filters, list_card,
};
use ferrohealth_ui::widget::action_menu::{
    Act, Action, Choice, Item, Listbox, Menu, Pick, listbox, row_menu,
};
use leptos::prelude::*;
use leptos_router::hooks::{use_params_map, use_query_map};

use crate::app::{MAPPINGS, NEW_FILE, PLAYGROUND, TEMPLATES, page_title};
use crate::locale::messages::Msg;
use crate::locale::{tr, tr_with};
use crate::model::{Loaded, Outcome};
use crate::pages::parts::{
    by_file, diagnostics_state, edit_href, faulted, file_href, language_name, owner_tag,
    owner_word, quiet, refused_operator, short, tally, wire_path,
};
use crate::posts::Reloaded;

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
fn tab_href(key: &str) -> String {
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
    let reloaded = use_context::<Reloaded>();
    view! {
        {page_title(tr(Msg::MappingSet))}
        <Suspense>
            {move || {
                let tab = tab.clone();
                let query = query.clone();
                let reloaded = reloaded.clone();
                Suspend::new(async move {
                    let state = crate::load::state().await;
                    let listing = crate::load::listing().await;
                    drawn(&tab, &query, reloaded.as_ref(), &state, &listing)
                })
            }}
        </Suspense>
    }
    .into_any()
}

/// The title row: New file and More with Reload the set….
fn heading() -> AnyView {
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
            id: RELOAD_DIALOG,
            title: tr(Msg::ReloadDialogTitle).into(),
            message: tr(Msg::ReloadDialogBody).into(),
        },
        view! {
            <form method="post" action=RELOAD class="fui-action-form">
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
        (Verdict::Bad, Msg::RunningNotCompiles)
    } else if warnings > 0 {
        (Verdict::Warn, Msg::RunningCompilesWarnings)
    } else {
        (Verdict::Ok, Msg::RunningCompiles)
    };
    load_report(LoadReport {
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
                (Verdict::Ok, Msg::Reloaded)
            } else {
                (Verdict::Bad, Msg::ReloadRefused)
            };
            let shown = load_report(LoadReport {
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
) -> AnyView {
    let (state, listing) = match (state, listing) {
        (Ok(Outcome::Shown(state)), Ok(Outcome::Shown(listing))) => (state, listing),
        (Err(fault), _) | (_, Err(fault)) => {
            return view! { {heading()} {faulted(fault, &tab_href(tab))} }.into_any();
        }
        (Ok(Outcome::Refused(refusal)), _) | (_, Ok(Outcome::Refused(refusal))) => {
            return view! { {heading()} {refused_operator(refusal)} }.into_any();
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
            {heading()}
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
        {heading()}
        <div class="fui-list-page">{report} {card}</div>
    }
    .into_any()
}

/// A count as the tabs carry it.
fn count(n: usize) -> u64 {
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

/// The filters of the Files tab, as its query carries them.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct FileFilters {
    /// The words of the find field.
    words: String,
    /// The language chosen.
    language: Option<Language>,
    /// The owner chosen.
    owner: Option<Owner>,
}

impl FileFilters {
    /// The filters `query` names; a value that names nothing is no filter.
    fn read(query: &leptos_router::params::ParamsMap) -> Self {
        Self {
            words: query.get("q").unwrap_or_default().trim().to_owned(),
            language: match query.get("language").as_deref() {
                Some("fhirconnect") => Some(Language::Fhirconnect),
                Some("omocl") => Some(Language::Omocl),
                _ => None,
            },
            owner: match query.get("owner").as_deref() {
                Some("provisioned") => Some(Owner::Provisioned),
                Some("console") => Some(Owner::Console),
                _ => None,
            },
        }
    }

    /// Whether `file` passes every filter.
    fn keeps(&self, file: &Listed) -> bool {
        self.language
            .is_none_or(|language| file.language == language)
            && self.owner.is_none_or(|owner| file.owner == owner)
            && (self.words.is_empty()
                || file
                    .path
                    .to_lowercase()
                    .contains(&self.words.to_lowercase()))
    }

    /// The query of these filters, `without` one of them, with `paging`.
    fn href(&self, without: Option<&str>, paging: Option<Paging>) -> String {
        let mut pairs = Vec::new();
        if !self.words.is_empty() && without != Some("q") {
            pairs.push(format!("q={}", encoded(&self.words)));
        }
        if let Some(language) = self.language.filter(|_| without != Some("language")) {
            pairs.push(format!("language={}", language_key(language)));
        }
        if let Some(owner) = self.owner.filter(|_| without != Some("owner")) {
            pairs.push(format!("owner={}", owner_key(owner)));
        }
        if let Some(paging) = paging {
            pairs.push(format!("page={}&size={}", paging.page, paging.size));
        }
        let base = tab_href("files");
        if pairs.is_empty() {
            base
        } else {
            format!("{base}?{}", pairs.join("&"))
        }
    }
}

/// The query value of `language`.
const fn language_key(language: Language) -> &'static str {
    match language {
        Language::Fhirconnect => "fhirconnect",
        Language::Omocl => "omocl",
    }
}

/// The query value of `owner`.
const fn owner_key(owner: Owner) -> &'static str {
    match owner {
        Owner::Provisioned => "provisioned",
        Owner::Console => "console",
    }
}

/// `words` encoded for a URL query (RFC 3986 §3.4).
fn encoded(words: &str) -> String {
    url::form_urlencoded::byte_serialize(words.as_bytes()).collect()
}

/// The filter menu of one filter: a select button whose choices are links.
fn filter_menu(
    id: &str,
    label: String,
    any: String,
    current: Option<String>,
    choices: Vec<(String, String, bool)>,
    clear: String,
) -> AnyView {
    let mut options = vec![Choice {
        label: any.clone().into(),
        note: None,
        lang: None,
        selected: current.is_none(),
        pick: Pick::Link(clear),
    }];
    options.extend(choices.into_iter().map(|(words, href, selected)| Choice {
        label: words.into(),
        note: None,
        lang: None,
        selected,
        pick: Pick::Link(href),
    }));
    listbox(Listbox {
        id: id.to_owned(),
        label,
        value: current.unwrap_or(any).into(),
        choices: options,
    })
}

/// The Files tab's toolbar: the find field and the Language and Owner
/// filters, each carrying the others.
fn files_toolbar(filters: &FileFilters) -> AnyView {
    let language_menu = filter_menu(
        "filter-language",
        tr(Msg::ColLanguage),
        tr(Msg::FilterAnyLanguage),
        filters
            .language
            .map(|language| language_name(language).to_owned()),
        [Language::Fhirconnect, Language::Omocl]
            .into_iter()
            .map(|language| {
                let chosen = FileFilters {
                    language: Some(language),
                    ..filters.clone()
                };
                (
                    language_name(language).to_owned(),
                    chosen.href(None, None),
                    filters.language == Some(language),
                )
            })
            .collect(),
        filters.href(Some("language"), None),
    );
    let owner_menu = filter_menu(
        "filter-owner",
        tr(Msg::ColOwner),
        tr(Msg::FilterAnyOwner),
        filters.owner.map(owner_word),
        [Owner::Provisioned, Owner::Console]
            .into_iter()
            .map(|owner| {
                let chosen = FileFilters {
                    owner: Some(owner),
                    ..filters.clone()
                };
                (
                    owner_word(owner),
                    chosen.href(None, None),
                    filters.owner == Some(owner),
                )
            })
            .collect(),
        filters.href(Some("owner"), None),
    );
    let carried: Vec<(String, String)> = [
        filters
            .language
            .map(|language| (String::from("language"), language_key(language).to_owned())),
        filters
            .owner
            .map(|owner| (String::from("owner"), owner_key(owner).to_owned())),
    ]
    .into_iter()
    .flatten()
    .collect();
    find_form(
        Find {
            id: "find-files",
            name: "q",
            label: &tr(Msg::FindFiles),
            value: &filters.words,
            source: StateSource::Url,
            action: None,
            fields: &carried,
        },
        view! { {language_menu} {owner_menu} },
    )
}

/// The Files tab's active filter chips, `total` of `all` files shown, or
/// nothing while no filter is set.
fn files_chips(filters: &FileFilters, total: u32, all: usize) -> Option<AnyView> {
    let mut chips = Vec::new();
    if let Some(language) = filters.language {
        chips.push(FilterChip {
            label: tr_with(
                Msg::ChipLanguage,
                &[("value", language_name(language).into())],
            ),
            remove: filters.href(Some("language"), None),
        });
    }
    if let Some(owner) = filters.owner {
        chips.push(FilterChip {
            label: tr_with(Msg::ChipOwner, &[("value", owner_word(owner).into())]),
            remove: filters.href(Some("owner"), None),
        });
    }
    if !filters.words.is_empty() {
        chips.push(FilterChip {
            label: tr_with(Msg::ChipPath, &[("value", filters.words.clone().into())]),
            remove: filters.href(Some("q"), None),
        });
    }
    (!chips.is_empty()).then(|| {
        active_filters(
            ActiveFilters {
                chips,
                clear: tab_href("files"),
                shown: Some(u64::from(total)),
            },
            Some(count(all)),
        )
    })
}

/// The Files tab: the find field and the filters, the files on the page,
/// and the pager.
fn files_body(
    query: &leptos_router::params::ParamsMap,
    listing: &[Listed],
) -> (AnyView, Option<AnyView>) {
    let filters = FileFilters::read(query);
    let mut kept = listing
        .iter()
        .filter(|file| filters.keeps(file))
        .collect::<Vec<_>>();
    kept.sort_by(|a, b| a.path.cmp(&b.path));
    let total = u32::try_from(kept.len()).unwrap_or(u32::MAX);
    let paging = Paging::from_query(query);
    let window = Window::over(total, paging);
    let shown = window.slice(&kept);
    let toolbar = files_toolbar(&filters);
    let chips = files_chips(&filters, total, listing.len());
    let table = if shown.is_empty() {
        quiet(
            tr(Msg::NoFileMatches),
            Some((tr(Msg::ClearFilters), tab_href("files"))),
        )
    } else {
        files_table(shown)
    };
    let body = view! {
        <div class="fui-panel-body fui-stack">{toolbar} {chips}</div>
        {table}
    }
    .into_any();
    let pager = (total > 0).then(|| {
        let href = |paging: Paging| filters.href(None, Some(paging));
        link_pager(Listing::new(Cow::Owned(tr(Msg::TabFiles))), window, &href)
    });
    (body, pager)
}

/// The files on the page.
fn files_table(files: &[&Listed]) -> AnyView {
    let columns = [
        Column::text("path", tr(Msg::ColPath)).role(Role::Title),
        Column::text("kind", tr(Msg::ColKind)).role(Role::Sub),
        Column::text("owner", tr(Msg::ColOwner)).role(Role::Wide),
        Column::text("diagnostics", tr(Msg::ColDiagnostics)),
    ];
    let [_, kind, owner, word] = &columns;
    let rows = files
        .iter()
        .map(|file| {
            let (errors, warnings) = tally(&file.diagnostics);
            view! {
                <tr>
                    {title_cell(Some(&file_href(&file.path)), view! { <code>{file.path.clone()}</code> }, None)}
                    {cell(kind, file.kind.clone().unwrap_or_else(|| String::from("–")))}
                    {cell(owner, owner_tag(file.owner))}
                    {cell(word, diagnostics_state(errors, warnings, Msg::StateNoDiagnostic))}
                </tr>
            }
        })
        .collect_view()
        .into_any();
    data_table(DataTable {
        caption: &tr(Msg::TabFiles),
        columns: &columns,
        sorting: Sorting::default(),
        body: Body::Rows(rows),
    })
    .into_any()
}
