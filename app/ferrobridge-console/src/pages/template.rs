// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! One operational template the running set compiled against: a detail
//! page (T2) with no tabs, its facts and the contexts that name it.

use ferrobridge_operator_api::operator::{EngineState, Listed, Template};
use ferrohealth_ui::html::Html;
use ferrohealth_ui::i18n::Locale;
use ferrohealth_ui::table::view::{
    Body, Column, DataTable, Role, Sorting, cell, data_table, title_cell,
};
use ferrohealth_ui::view::action::ActionGroup;
use ferrohealth_ui::view::surface::{Crumb, Header, facts_row, panel_sections_card};
use ferrohealth_ui::view::template::detail::{DetailPage, detail_page};

use crate::app::{Drawn, TEMPLATES};
use crate::locale::messages::Msg;
use crate::locale::tr;
use crate::markup::{code, h, t};
use crate::model::{Loaded, Outcome};
use crate::pages::mappings::{program_href, template_id};
use crate::pages::parts::{faulted, file_href, owner_tag, quiet, refused_operator, section};
use crate::visit::Visit;

/// The page of the template the route names.
pub async fn page(visit: &Visit) -> Drawn {
    let id = visit
        .segment(2)
        .and_then(crate::pages::parts::decoded)
        .unwrap_or_default();
    let state = visit.reads.running().await;
    let listing = visit.reads.listing().await;
    match drawn(&visit.locale, &id, &state, &listing) {
        Some(body) => Drawn::shell(id, body),
        None => crate::pages::errors::not_found(visit),
    }
}

/// The page over the running set's template `id`, `None` for a template
/// the running set does not hold.
fn drawn(
    l: &Locale,
    id: &str,
    state: &Loaded<EngineState>,
    listing: &Loaded<Vec<Listed>>,
) -> Option<Html> {
    let (state, listing) = match (state, listing) {
        (Ok(Outcome::Shown(state)), Ok(Outcome::Shown(listing))) => (state, listing),
        (Err(fault), _) | (_, Err(fault)) => {
            return Some(faulted(l, fault, &format!("{TEMPLATES}/{id}")));
        }
        (Ok(Outcome::Refused(refusal)), _) | (_, Ok(Outcome::Refused(refusal))) => {
            return Some(refused_operator(l, refusal));
        }
    };
    let template = state
        .templates
        .iter()
        .find(|template| template_id(template) == id)?;
    let (source, file) = match template {
        Template::Directory { file, .. } => (tr(l, Msg::SourceDirectory), Some(file.clone())),
        Template::Cdr { .. } => (tr(l, Msg::SourceCdr), None),
    };
    let contexts = state
        .contexts
        .iter()
        .filter(|program| program.template == id)
        .collect::<Vec<_>>();
    let mut facts = vec![(tr(l, Msg::ColSource), t(&source))];
    if let Some(file) = file {
        facts.push((tr(l, Msg::ColFile), code(file)));
    }
    facts.push((tr(l, Msg::ColContexts), t(contexts.len().to_string())));
    let columns = [
        Column::text("context", tr(l, Msg::ColContext)).role(Role::Title),
        Column::text("resource", tr(l, Msg::ColResourceType)).role(Role::Sub),
        Column::text("owner", tr(l, Msg::ColOwner)),
    ];
    let [_, resource, owner] = &columns;
    let table = if contexts.is_empty() {
        quiet(tr(l, Msg::TemplateNoContexts), None)
    } else {
        let rows = contexts
            .iter()
            .map(|program| {
                let owner_tag = program
                    .file
                    .as_deref()
                    .and_then(|path| listing.iter().find(|each| each.path == path))
                    .map(|listed| owner_tag(l, listed.owner))
                    .unwrap_or_default();
                let href = program_href(program).or_else(|| program.file.as_deref().map(file_href));
                h(format!(
                    "<tr>{}{}{}</tr>",
                    title_cell(href.as_deref(), t(&program.context), None),
                    cell(resource, t(&program.resource_type)),
                    cell(owner, owner_tag),
                ))
            })
            .collect::<Html>();
        data_table(DataTable {
            caption: &tr(l, Msg::TemplateContexts),
            columns: &columns,
            sorting: Sorting::default(),
            body: Body::Rows(rows),
        })
    };
    let count = u64::try_from(contexts.len()).unwrap_or(u64::MAX);
    let body = panel_sections_card(
        tr(l, Msg::ColTemplate),
        section(l, tr(l, Msg::ColTemplate), None, facts_row(facts)).then(section(
            l,
            tr(l, Msg::TemplateContexts),
            Some(count),
            table,
        )),
    );
    let subtitle = match template {
        Template::Directory { .. } => tr(l, Msg::TemplateFromDirectory),
        Template::Cdr { .. } => tr(l, Msg::TemplateFromCdr),
    };
    Some(detail_page(
        l,
        DetailPage {
            header: Header {
                title: id.to_owned().into(),
                subtitle: Some(subtitle.into()),
                subtitle_view: None,
                crumbs: vec![Crumb::new(
                    tr(l, Msg::MappingSet),
                    format!("{}/templates", crate::app::MAPPINGS),
                )],
                mono: true,
                badge: None,
            },
            actions: ActionGroup::none(),
            stepper: None,
            tabs: None,
            body,
        },
    ))
}
