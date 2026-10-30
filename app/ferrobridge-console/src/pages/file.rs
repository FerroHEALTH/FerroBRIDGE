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
use ferrohealth_ui::html::Html;
use ferrohealth_ui::i18n::Locale;
use ferrohealth_ui::i18n::Phrase;
use ferrohealth_ui::icon::Glyph;
use ferrohealth_ui::table::view::{
    Body, Column, DataTable, Role, Sorting, cell, data_table, title_cell,
};
use ferrohealth_ui::tone::Tone;
use ferrohealth_ui::view::action::ActionGroup;
use ferrohealth_ui::view::action_menu::{Act, Action};
use ferrohealth_ui::view::code::{Code, CodeHead, Digest, code_viewer};
use ferrohealth_ui::view::diagnostics::{FileFindings, diagnostics};
use ferrohealth_ui::view::notice::{Announce, notice};
use ferrohealth_ui::view::state::Mark;
use ferrohealth_ui::view::surface::{
    Crumb, Header, definitions, facts_row, page_heading, panel_sections, panel_sections_card,
    source_split,
};
use ferrohealth_ui::view::tabs::{Tab, binder};
use openehr_mapping_core::diagnostic::Diagnostic;

use crate::app::{Drawn, FACADE, FILES, TEMPLATES};
use crate::locale::messages::Msg;
use crate::locale::{tr, tr_with};
use crate::markup::{code, h, t};
use crate::model::{Loaded, OpenedFile, Outcome};
use crate::pages::parts::{
    diagnostics_state, edit_href, faulted, file_href, findings, language_name, line_diagnostics,
    quiet, refused_operator, section, short, tally, wire_path, with_markup,
};
use crate::visit::Visit;

/// The id prefix of the code viewer's lines, which a diagnostic links to.
const LINE_IDS: &str = "L";

/// The id of the code viewer's Copy button.
const COPY_ID: &str = "file-copy";

/// The page of the file the route names.
pub async fn page(visit: &Visit) -> Drawn {
    let path = visit
        .path
        .strip_prefix(FILES)
        .and_then(|rest| rest.strip_prefix('/'))
        .and_then(crate::pages::parts::decoded)
        .unwrap_or_default();
    if path.is_empty() {
        return crate::pages::mappings::page(visit, None).await;
    }
    let program_tab = visit.param("tab") == Some("program");
    // NOTE: RFC 3986 §3.5: a fragment never reaches the server, so the line a
    // diagnostic links to is reached by the browser alone, unmarked.
    let current = None;
    let file = visit.reads.opened(&path).await;
    let listing = visit.reads.listing().await;
    let state = visit.reads.running().await;
    match drawn(
        &visit.locale,
        &path,
        program_tab,
        current,
        &file,
        &listing,
        &state,
    ) {
        Some(body) => Drawn::shell(short(&path).to_owned(), body),
        None => crate::pages::errors::not_found(visit),
    }
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

/// The page over the file and what the running load says of it, `None`
/// for a file the bridge does not hold.
fn drawn(
    l: &Locale,
    path: &str,
    program_tab: bool,
    current: Option<u32>,
    file: &Loaded<OpenedFile>,
    listing: &Loaded<Vec<Listed>>,
    state: &Loaded<EngineState>,
) -> Option<Html> {
    let (file, listing, state) = match (file, listing, state) {
        (Ok(Outcome::Shown(file)), Ok(Outcome::Shown(listing)), Ok(Outcome::Shown(state))) => {
            (file, listing, state)
        }
        (Err(fault), _, _) | (_, Err(fault), _) | (_, _, Err(fault)) => {
            return Some(faulted(l, fault, &file_href(path)));
        }
        (Ok(Outcome::Refused(refusal)), _, _)
        | (_, Ok(Outcome::Refused(refusal)), _)
        | (_, _, Ok(Outcome::Refused(refusal))) => {
            if refusal.not_served() && matches!(file, Ok(Outcome::Refused(_))) {
                return None;
            }
            return Some(refused_operator(l, refusal));
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
        ferrohealth_ui::view::state::state(Mark::Bad, tr(l, Msg::StateNotUtf8), None)
    } else {
        diagnostics_state(l, errors, warnings, Msg::StateCompiles)
    };
    let header = Header {
        title: short(path).to_owned().into(),
        subtitle: None,
        subtitle_view: Some(subtitle(l, kind, program, owner, path)),
        crumbs: vec![Crumb::new(tr(l, Msg::MappingSet), FILES)],
        mono: false,
        badge: Some(badge),
    };
    let source = match &text {
        Some(text) => source_panel(l, path, text, &found, current, listed),
        None => source_split(
            quiet(
                tr(l, Msg::FileNotUtf8),
                Some((tr(l, Msg::EveryFile), String::from(FILES))),
            ),
            diagnostics_section(l, path, &found, current),
        ),
    };
    let lock = if read_only && text.is_some() {
        notice(
            Tone::Info,
            Announce::Status,
            None,
            t(tr(l, Msg::ProvisionedNotice)),
        )
    } else {
        Html::empty()
    };
    let sections = match kind {
        Kind::Context => {
            let body = if program_tab {
                program.map_or_else(
                    || quiet(tr(l, Msg::NoProgram), None),
                    |program| program_panel(l, program, listing),
                )
            } else {
                source
            };
            context_tabs(l, path, program_tab, body)
        }
        Kind::Model => {
            let used = used_by(l, path, state);
            panel_sections_card(tr(l, Msg::TabSource), source).then(used)
        }
        Kind::Other => panel_sections_card(tr(l, Msg::TabSource), source),
    };
    Some(
        page_heading(l, header, actions_of(l, path, text.is_some(), read_only)).then(h(format!(
            r#"<div class="fui-stack">{lock}{sections}</div>"#
        ))),
    )
}

/// The title row's action of the file at `path`: Edit for a console file,
/// Check a draft for a provisioned one, and none for a file not shown.
fn actions_of(l: &Locale, path: &str, shown: bool, read_only: bool) -> ActionGroup {
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
        tr(l, words),
        Act::Link(edit_href(path)),
    ))
}

/// The binder tabs of a context file, Source and Program, joined to `body`.
fn context_tabs(l: &Locale, path: &str, program_tab: bool, body: Html) -> Html {
    binder(
        l,
        tr_with(l, Msg::FileSections, &[("name", short(path).into())]),
        vec![
            Tab::new("source", file_href(path), tr(l, Msg::TabSource)),
            Tab::new(
                "program",
                format!("{}?tab=program", file_href(path)),
                tr(l, Msg::TabProgram),
            ),
        ],
        if program_tab { "program" } else { "source" },
        body,
    )
}

/// The subtitle of a file of `kind`: what it is, and who owns it, a
/// context's resource type in bold and its template a code link.
fn subtitle(l: &Locale, kind: Kind, program: Option<&Program>, owner: Owner, path: &str) -> Html {
    let by = tr(
        l,
        match owner {
            Owner::Console => Msg::SavedByConsole,
            Owner::Provisioned => Msg::Provisioned,
        },
    );
    let text = match (kind, program) {
        (Kind::Context, Some(program)) => {
            let text = tr_with(
                l,
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
                    ("resource", h(format!("<strong>{}</strong>", t(resource)))),
                    (
                        "template",
                        h(format!(
                            r#"<a href="{}">{}</a>"#,
                            t(format!("{TEMPLATES}/{template}")),
                            code(&template)
                        )),
                    ),
                ],
            );
        }
        (Kind::Model, _) => tr_with(l, Msg::ModelSubtitle, &[("owned", by.into())]),
        _ if path.starts_with("omocl/") => tr_with(l, Msg::OmoclSubtitle, &[("owned", by.into())]),
        _ => tr_with(l, Msg::FileSubtitle, &[("owned", by.into())]),
    };
    t(text)
}

/// The diagnostics section beside the source: the findings of the file,
/// the line `current` marked, or a quiet line when there is none.
fn diagnostics_section(
    l: &Locale,
    path: &str,
    found: &[&Diagnostic],
    current: Option<u32>,
) -> Html {
    let count = u64::try_from(found.len()).unwrap_or(u64::MAX);
    let body = if found.is_empty() {
        quiet(tr(l, Msg::NoFileDiagnostics), None)
    } else {
        diagnostics(
            l,
            &tr(l, Msg::TabDiagnostics),
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
    section(l, tr(l, Msg::TabDiagnostics), Some(count), body)
}

/// The Source tab: the file as the bridge serves it beside its diagnostics.
fn source_panel(
    l: &Locale,
    path: &str,
    text: &str,
    found: &[&Diagnostic],
    current: Option<u32>,
    listed: Option<&Listed>,
) -> Html {
    let mut tags = Vec::new();
    if let Some(listed) = listed {
        tags.push(language_name(listed.language).to_owned());
        tags.push(crate::pages::parts::owner_word(l, listed.owner));
    }
    let digest = listed.map(|listed| Digest {
        text: tr_with(
            l,
            Msg::ShortDigest,
            &[("digest", short_digest(&listed.sha256).into())],
        ),
        title: tr(l, Msg::DigestTitle),
    });
    let viewer = code_viewer(
        l,
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
    source_split(viewer, diagnostics_section(l, path, found, current))
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
fn program_panel(l: &Locale, program: &Program, listing: &[Listed]) -> Html {
    let template = h(format!(
        r#"<a href="{}">{}</a>"#,
        t(format!("{TEMPLATES}/{}", program.template)),
        code(&program.template)
    ));
    let facts = facts_row(vec![
        (tr(l, Msg::ColResourceType), t(&program.resource_type)),
        (
            tr(l, Msg::FactProfile),
            code(format!("{}|{}", program.profile, program.profile_version)),
        ),
        (tr(l, Msg::ColTemplate), template),
    ]);
    let columns = [
        Column::text("model", tr(l, Msg::ColModel)).role(Role::Title),
        Column::text("file", tr(l, Msg::ColFile)).role(Role::Wide),
        Column::text("owner", tr(l, Msg::ColOwner)),
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
                .map(|listed| crate::pages::parts::owner_tag(l, listed.owner))
                .unwrap_or_default();
            h(format!(
                "<tr>{}{}{}</tr>",
                title_cell(href.as_deref(), t(&model.name), None),
                cell(file, model.file.as_deref().map(code).unwrap_or_default()),
                cell(owner, owner_tag),
            ))
        })
        .collect::<Html>();
    let models = data_table(DataTable {
        caption: &tr(l, Msg::ColModels),
        columns: &columns,
        sorting: Sorting::default(),
        body: Body::Rows(rows),
    });
    let search = match &program.search {
        Some(search) => h(format!(r#"<p class="fui-panel-body">{}</p>"#, code(search))),
        None => quiet(
            tr_with(
                l,
                Msg::NoSearch,
                &[("resource", program.resource_type.clone().into())],
            ),
            None,
        ),
    };
    let runs = definitions(vec![
        (
            tr(l, Msg::FhirFacade),
            h(format!(
                r#"<a href="{}">{}</a>"#,
                t(format!("{FACADE}/capability")),
                t(&program.resource_type)
            )),
        ),
        (tr(l, Msg::FlowOperations), code("$tofhir, $toopenehr")),
    ]);
    let count = u64::try_from(program.models.len()).unwrap_or(u64::MAX);
    panel_sections(Html::join([
        section(l, tr(l, Msg::CompiledProgram), None, facts),
        section(l, tr(l, Msg::ColModels), Some(count), models),
        section(l, tr(l, Msg::ColSearch), None, search),
        section(l, tr(l, Msg::WhereItRuns), None, runs),
    ]))
}

/// The contexts that read with the model at `path`.
fn used_by(l: &Locale, path: &str, state: &EngineState) -> Html {
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
        Column::text("context", tr(l, Msg::ColContext)).role(Role::Title),
        Column::text("resource", tr(l, Msg::ColResourceType)).role(Role::Sub),
    ];
    let [_, resource] = &columns;
    let body = if users.is_empty() {
        quiet(tr(l, Msg::UsedByNone), None)
    } else {
        let rows = users
            .iter()
            .map(|program| {
                let href = crate::pages::mappings::program_href(program);
                h(format!(
                    "<tr>{}{}</tr>",
                    title_cell(href.as_deref(), t(&program.context), None),
                    cell(resource, t(&program.resource_type)),
                ))
            })
            .collect::<Html>();
        data_table(DataTable {
            caption: &tr(l, Msg::UsedBy),
            columns: &columns,
            sorting: Sorting::default(),
            body: Body::Rows(rows),
        })
    };
    let count = u64::try_from(users.len()).unwrap_or(u64::MAX);
    panel_sections_card(
        tr(l, Msg::UsedBy),
        section(l, tr(l, Msg::UsedBy), Some(count), body),
    )
}
