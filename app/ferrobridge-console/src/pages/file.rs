// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! One mapping file, its one home: a detail page (T2) with the file as the
//! bridge serves it, byte for byte (B1), beside its diagnostics (B3).
//!
//! A context file has two tabs: Source, and Program, what the context
//! compiled. A model lists the contexts that use it under its source. A
//! provisioned file says it is read-only here and offers Check a draft; a
//! console file offers Edit. A file that is not UTF-8 is not shown, and the
//! page says why.

use ferrobridge_operator_api::operator::{EngineState, Listed, Owner, Program};
use ferrohealth_ui::highlight::Language;
use ferrohealth_ui::i18n::Phrase;
use ferrohealth_ui::icon::Glyph;
use ferrohealth_ui::table::view::{
    Body, Column, DataTable, Role, Sorting, cell, data_table, title_cell,
};
use ferrohealth_ui::tone::Tone;
use ferrohealth_ui::view::action::ActionGroup;
use ferrohealth_ui::view::code::{Code, CodeHead, Digest, code_viewer};
use ferrohealth_ui::view::diagnostics::{FileFindings, diagnostics};
use ferrohealth_ui::view::notice::{Announce, notice};
use ferrohealth_ui::view::state::Mark;
use ferrohealth_ui::view::surface::{
    Crumb, Header, definitions, facts_row, page_heading, panel_sections, panel_sections_card,
    source_split,
};
use ferrohealth_ui::view::tabs::{Tab, binder};
use ferrohealth_ui::widget::action_menu::{Act, Action};
use leptos::prelude::*;
use leptos_router::hooks::{use_params_map, use_query_map};
use openehr_mapping_core::diagnostic::Diagnostic;

use crate::app::{FACADE, FILES, TEMPLATES, page_title};
use crate::locale::messages::Msg;
use crate::locale::{tr, tr_with};
use crate::model::{Loaded, OpenedFile, Outcome};
use crate::pages::parts::{
    diagnostics_state, edit_href, faulted, file_href, findings, language_name, line_diagnostics,
    quiet, refused_operator, section, short, tally, wire_path, with_markup,
};

/// The id prefix of the code viewer's lines, which a diagnostic links to.
const LINE_IDS: &str = "L";

/// The id of the code viewer's Copy button.
const COPY_ID: &str = "file-copy";

/// The page of the file the route names.
#[must_use]
pub fn page() -> impl IntoView {
    let path = use_params_map()
        .with_untracked(|params| params.get("path"))
        .and_then(|raw| crate::pages::parts::decoded(&raw))
        .unwrap_or_default();
    if path.is_empty() {
        return crate::pages::mappings::page().into_any();
    }
    let program_tab =
        use_query_map().with_untracked(|query| query.get("tab")) == Some(String::from("program"));
    // NOTE: RFC 3986 §3.5: a fragment never reaches the server, so the line a
    // diagnostic links to is reached by the browser alone, unmarked.
    let current = None;
    let file = Resource::new(
        {
            let path = path.clone();
            move || (crate::load::changes(), path.clone())
        },
        |(_, path)| crate::load::file_now(path),
    );
    let listing = Resource::new(crate::load::changes, |_| crate::load::listing_now());
    let state = Resource::new(crate::load::changes, |_| crate::load::state_now());
    view! {
        {page_title(short(&path).to_owned())}
        <Suspense>
            {move || {
                let path = path.clone();
                Suspend::new(async move {
                    let file = file.await;
                    let listing = listing.await;
                    let state = state.await;
                    drawn(&path, program_tab, current, &file, &listing, &state)
                })
            }}
        </Suspense>
    }
    .into_any()
}

/// What kind of mapping file a path holds, which decides its sections.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    /// A FHIRconnect context: Source and Program.
    Context,
    /// A FHIRconnect model or extension: Source, and the contexts using it.
    Model,
    /// An OMOCL file or a search declaration: Source alone.
    Other,
}

/// The kind of the file at `path` the listing describes as `listed`.
fn kind_of(listed: Option<&Listed>, state: &EngineState, path: &str) -> Kind {
    if state
        .contexts
        .iter()
        .any(|program| program.file.as_deref() == Some(path))
    {
        return Kind::Context;
    }
    match listed.and_then(|file| file.kind.as_deref()) {
        Some("context") => Kind::Context,
        Some("model" | "extension") => Kind::Model,
        _ => Kind::Other,
    }
}

/// The page over the file and what the running load says of it.
fn drawn(
    path: &str,
    program_tab: bool,
    current: Option<u32>,
    file: &Loaded<OpenedFile>,
    listing: &Loaded<Vec<Listed>>,
    state: &Loaded<EngineState>,
) -> AnyView {
    let (file, listing, state) = match (file, listing, state) {
        (Ok(Outcome::Shown(file)), Ok(Outcome::Shown(listing)), Ok(Outcome::Shown(state))) => {
            (file, listing, state)
        }
        (Err(fault), _, _) | (_, Err(fault), _) | (_, _, Err(fault)) => {
            return faulted(fault, &file_href(path));
        }
        (Ok(Outcome::Refused(refusal)), _, _)
        | (_, Ok(Outcome::Refused(refusal)), _)
        | (_, _, Ok(Outcome::Refused(refusal))) => {
            if refusal.not_served() && matches!(file, Ok(Outcome::Refused(_))) {
                return crate::pages::errors::not_found().into_any();
            }
            return refused_operator(refusal);
        }
    };
    let listed = listing.iter().find(|each| each.path == path);
    let owner = listed.map_or(Owner::Provisioned, |listed| listed.owner);
    let read_only = listed.is_none_or(|listed| listed.read_only);
    let kind = kind_of(listed, state, path);
    let found = state
        .diagnostics
        .iter()
        .filter(|diagnostic| wire_path(diagnostic) == path)
        .collect::<Vec<_>>();
    let (errors, warnings) = tally(found.iter().copied());
    let program = state
        .contexts
        .iter()
        .find(|program| program.file.as_deref() == Some(path));
    let text = file.text.clone();
    let badge = if text.is_none() {
        ferrohealth_ui::view::state::state(Mark::Bad, tr(Msg::StateNotUtf8), None)
    } else {
        diagnostics_state(errors, warnings, Msg::StateCompiles)
    };
    let header = Header {
        title: short(path).to_owned().into(),
        subtitle: None,
        subtitle_view: Some(subtitle(kind, program, owner, path)),
        crumbs: vec![Crumb::new(tr(Msg::MappingSet), FILES)],
        mono: false,
        badge: Some(badge),
    };
    let source = match &text {
        Some(text) => source_panel(path, text, &found, current, listed),
        None => source_split(
            quiet(
                tr(Msg::FileNotUtf8),
                Some((tr(Msg::EveryFile), String::from(FILES))),
            ),
            diagnostics_section(path, &found, current),
        ),
    };
    let lock = (read_only && text.is_some()).then(|| {
        notice(
            Tone::Info,
            Announce::Status,
            None,
            tr(Msg::ProvisionedNotice),
        )
        .into_any()
    });
    let sections = match kind {
        Kind::Context => {
            let body = if program_tab {
                program.map_or_else(
                    || quiet(tr(Msg::NoProgram), None),
                    |program| program_panel(program, listing),
                )
            } else {
                source
            };
            context_tabs(path, program_tab, body)
        }
        Kind::Model => {
            let used = used_by(path, state);
            view! { {panel_sections_card(tr(Msg::TabSource), source)} {used} }.into_any()
        }
        Kind::Other => panel_sections_card(tr(Msg::TabSource), source),
    };
    view! {
        {page_heading(header, actions_of(path, text.is_some(), read_only))}
        <div class="fui-stack">{lock} {sections}</div>
    }
    .into_any()
}

/// The title row's action of the file at `path`: Edit for a console file,
/// Check a draft for a provisioned one, and none for a file not shown.
fn actions_of(path: &str, shown: bool, read_only: bool) -> ActionGroup {
    if !shown {
        return ActionGroup::none();
    }
    let (glyph, words) = if read_only {
        (Glyph::LIST_CHECKS, Msg::CheckDraft)
    } else {
        (Glyph::PENCIL, Msg::Edit)
    };
    ActionGroup::primary(Action::new(
        Some(glyph),
        tr(words),
        Act::Link(edit_href(path)),
    ))
}

/// The binder tabs of a context file, Source and Program, joined to `body`.
fn context_tabs(path: &str, program_tab: bool, body: AnyView) -> AnyView {
    binder(
        tr_with(Msg::FileSections, &[("name", short(path).into())]),
        vec![
            Tab::new("source", file_href(path), tr(Msg::TabSource)),
            Tab::new(
                "program",
                format!("{}?tab=program", file_href(path)),
                tr(Msg::TabProgram),
            ),
        ],
        if program_tab { "program" } else { "source" },
        body,
    )
}

/// The subtitle of a file of `kind`: what it is, and who owns it, a
/// context's resource type in bold and its template a code link.
fn subtitle(kind: Kind, program: Option<&Program>, owner: Owner, path: &str) -> AnyView {
    let by = tr(match owner {
        Owner::Console => Msg::SavedByConsole,
        Owner::Provisioned => Msg::Provisioned,
    });
    let text = match (kind, program) {
        (Kind::Context, Some(program)) => {
            let text = tr_with(
                Msg::ContextSubtitle,
                &[
                    ("resource", Phrase::open("resource")),
                    ("template", Phrase::open("template")),
                    ("owned", by.into()),
                ],
            );
            let resource = program.resource_type.clone();
            let template = program.template.clone();
            return with_markup(
                &text,
                vec![
                    ("resource", view! { <strong>{resource}</strong> }.into_any()),
                    (
                        "template",
                        view! {
                            <a href=format!("{TEMPLATES}/{template}")>
                                <code>{template.clone()}</code>
                            </a>
                        }
                        .into_any(),
                    ),
                ],
            );
        }
        (Kind::Model, _) => tr_with(Msg::ModelSubtitle, &[("owned", by.into())]),
        _ if path.starts_with("omocl/") => tr_with(Msg::OmoclSubtitle, &[("owned", by.into())]),
        _ => tr_with(Msg::FileSubtitle, &[("owned", by.into())]),
    };
    view! { {text} }.into_any()
}

/// The diagnostics section beside the source: the findings of the file,
/// the line `current` marked, or a quiet line when there is none.
fn diagnostics_section(path: &str, found: &[&Diagnostic], current: Option<u32>) -> AnyView {
    let count = u64::try_from(found.len()).unwrap_or(u64::MAX);
    let body = if found.is_empty() {
        quiet(tr(Msg::NoFileDiagnostics), None)
    } else {
        diagnostics(
            &tr(Msg::TabDiagnostics),
            &FileFindings {
                file: path.to_owned(),
                href: None,
                lines: format!("#{LINE_IDS}"),
                current,
                findings: findings(found),
                all: None,
            },
        )
    };
    section(tr(Msg::TabDiagnostics), Some(count), body)
}

/// The Source tab: the file as the bridge serves it beside its diagnostics.
fn source_panel(
    path: &str,
    text: &str,
    found: &[&Diagnostic],
    current: Option<u32>,
    listed: Option<&Listed>,
) -> AnyView {
    let mut tags = Vec::new();
    if let Some(listed) = listed {
        tags.push(language_name(listed.language).to_owned());
        tags.push(crate::pages::parts::owner_word(listed.owner));
    }
    let digest = listed.map(|listed| Digest {
        text: tr_with(
            Msg::ShortDigest,
            &[("digest", short_digest(&listed.sha256).into())],
        ),
        title: tr(Msg::DigestTitle),
    });
    let viewer = code_viewer(
        Some(CodeHead {
            title: path.to_owned(),
            tags,
            digest,
            copy: Some(String::from(COPY_ID)),
            actions: None,
        }),
        &Code {
            label: path.to_owned(),
            language: Language::Yaml,
            text: text.to_owned(),
            ids: Some(String::from(LINE_IDS)),
            current,
            diagnostics: line_diagnostics(found),
        },
    );
    source_split(viewer, diagnostics_section(path, found, current))
}

/// A digest shortened to its first and last eight characters.
#[must_use]
pub fn short_digest(digest: &str) -> String {
    match (
        digest.get(..8),
        digest.get(digest.len().saturating_sub(8)..),
    ) {
        (Some(head), Some(tail)) if digest.len() > 16 => format!("{head}…{tail}"),
        _ => digest.to_owned(),
    }
}

/// The Program tab: what the context compiled, its models, its search and
/// where it runs.
fn program_panel(program: &Program, listing: &[Listed]) -> AnyView {
    let facts = facts_row(vec![
        (
            tr(Msg::ColResourceType),
            program.resource_type.clone().into_any(),
        ),
        (
            tr(Msg::FactProfile),
            view! { <code>{format!("{}|{}", program.profile, program.profile_version)}</code> }
                .into_any(),
        ),
        (
            tr(Msg::ColTemplate),
            view! {
                <a href=format!("{TEMPLATES}/{}", program.template)>
                    <code>{program.template.clone()}</code>
                </a>
            }
            .into_any(),
        ),
    ])
    .into_any();
    let columns = [
        Column::text("model", tr(Msg::ColModel)).role(Role::Title),
        Column::text("file", tr(Msg::ColFile)).role(Role::Wide),
        Column::text("owner", tr(Msg::ColOwner)),
    ];
    let [_, file, owner] = &columns;
    let rows = program
        .models
        .iter()
        .map(|model| {
            let href = model.file.as_deref().map(file_href);
            let owner_tag = model
                .file
                .as_deref()
                .and_then(|path| listing.iter().find(|each| each.path == path))
                .map(|listed| crate::pages::parts::owner_tag(listed.owner));
            view! {
                <tr>
                    {title_cell(href.as_deref(), model.name.clone(), None)}
                    {cell(file, model.file.clone().map(|path| view! { <code>{path}</code> }))}
                    {cell(owner, owner_tag)}
                </tr>
            }
        })
        .collect_view()
        .into_any();
    let models = data_table(DataTable {
        caption: &tr(Msg::ColModels),
        columns: &columns,
        sorting: Sorting::default(),
        body: Body::Rows(rows),
    })
    .into_any();
    let search = match &program.search {
        Some(search) => {
            view! { <p class="fui-panel-body"><code>{search.clone()}</code></p> }.into_any()
        }
        None => quiet(
            tr_with(
                Msg::NoSearch,
                &[("resource", program.resource_type.clone().into())],
            ),
            None,
        ),
    };
    let runs =
        definitions(vec![
        (
            tr(Msg::FhirFacade),
            view! { <a href=format!("{FACADE}/capability")>{program.resource_type.clone()}</a> }
                .into_any(),
        ),
        (tr(Msg::FlowOperations), view! { <code>"$tofhir, $toopenehr"</code> }.into_any()),
    ])
        .into_any();
    let count = u64::try_from(program.models.len()).unwrap_or(u64::MAX);
    panel_sections(view! {
        {section(tr(Msg::CompiledProgram), None, facts)}
        {section(tr(Msg::ColModels), Some(count), models)}
        {section(tr(Msg::ColSearch), None, search)}
        {section(tr(Msg::WhereItRuns), None, runs)}
    })
}

/// The contexts that read with the model at `path`.
fn used_by(path: &str, state: &EngineState) -> AnyView {
    let users = state
        .contexts
        .iter()
        .filter(|program| {
            program
                .models
                .iter()
                .any(|model| model.file.as_deref() == Some(path))
        })
        .collect::<Vec<_>>();
    let columns = [
        Column::text("context", tr(Msg::ColContext)).role(Role::Title),
        Column::text("resource", tr(Msg::ColResourceType)).role(Role::Sub),
    ];
    let [_, resource] = &columns;
    let body = if users.is_empty() {
        quiet(tr(Msg::UsedByNone), None)
    } else {
        let rows = users
            .iter()
            .map(|program| {
                let href = crate::pages::mappings::program_href(program);
                view! {
                    <tr>
                        {title_cell(href.as_deref(), program.context.clone(), None)}
                        {cell(resource, program.resource_type.clone())}
                    </tr>
                }
            })
            .collect_view()
            .into_any();
        data_table(DataTable {
            caption: &tr(Msg::UsedBy),
            columns: &columns,
            sorting: Sorting::default(),
            body: Body::Rows(rows),
        })
        .into_any()
    };
    let count = u64::try_from(users.len()).unwrap_or(u64::MAX);
    panel_sections_card(tr(Msg::UsedBy), section(tr(Msg::UsedBy), Some(count), body))
}
