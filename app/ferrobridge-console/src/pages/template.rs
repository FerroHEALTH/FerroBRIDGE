// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! One operational template the running set compiled against: a detail
//! page (T2) with no tabs, its facts and the contexts that name it.

use ferrobridge_operator_api::operator::{EngineState, Listed, Template};
use ferrohealth_ui::table::view::{
    Body, Column, DataTable, Role, Sorting, cell, data_table, title_cell,
};
use ferrohealth_ui::view::action::ActionGroup;
use ferrohealth_ui::view::surface::{Crumb, Header, facts_row, panel_sections_card};
use ferrohealth_ui::view::template::detail::{DetailPage, detail_page};
use leptos::prelude::*;
use leptos_router::hooks::use_params_map;

use crate::app::{TEMPLATES, page_title};
use crate::locale::messages::Msg;
use crate::locale::tr;
use crate::model::{Loaded, Outcome};
use crate::pages::mappings::{program_href, template_id};
use crate::pages::parts::{faulted, file_href, owner_tag, quiet, refused_operator, section};

/// The page of the template the route names.
#[must_use]
pub fn page() -> impl IntoView {
    let id = use_params_map()
        .with_untracked(|params| params.get("id"))
        .and_then(|raw| crate::pages::parts::decoded(&raw))
        .unwrap_or_default();
    let state = Resource::new(crate::load::changes, |_| crate::load::state_now());
    let listing = Resource::new(crate::load::changes, |_| crate::load::listing_now());
    view! {
        {page_title(id.clone())}
        <Suspense>
            {move || {
                let id = id.clone();
                Suspend::new(async move {
                    let state = state.await;
                    let listing = listing.await;
                    drawn(&id, &state, &listing)
                })
            }}
        </Suspense>
    }
}

/// The page over the running set's template `id`.
fn drawn(id: &str, state: &Loaded<EngineState>, listing: &Loaded<Vec<Listed>>) -> AnyView {
    let (state, listing) = match (state, listing) {
        (Ok(Outcome::Shown(state)), Ok(Outcome::Shown(listing))) => (state, listing),
        (Err(fault), _) | (_, Err(fault)) => {
            return faulted(fault, &format!("{TEMPLATES}/{id}"));
        }
        (Ok(Outcome::Refused(refusal)), _) | (_, Ok(Outcome::Refused(refusal))) => {
            return refused_operator(refusal);
        }
    };
    let Some(template) = state
        .templates
        .iter()
        .find(|template| template_id(template) == id)
    else {
        return crate::pages::errors::not_found().into_any();
    };
    let (source, file) = match template {
        Template::Directory { file, .. } => (tr(Msg::SourceDirectory), Some(file.clone())),
        Template::Cdr { .. } => (tr(Msg::SourceCdr), None),
    };
    let contexts = state
        .contexts
        .iter()
        .filter(|program| program.template == id)
        .collect::<Vec<_>>();
    let mut facts = vec![(tr(Msg::ColSource), source.clone().into_any())];
    if let Some(file) = file {
        facts.push((tr(Msg::ColFile), view! { <code>{file}</code> }.into_any()));
    }
    facts.push((tr(Msg::ColContexts), contexts.len().to_string().into_any()));
    let columns = [
        Column::text("context", tr(Msg::ColContext)).role(Role::Title),
        Column::text("resource", tr(Msg::ColResourceType)).role(Role::Sub),
        Column::text("owner", tr(Msg::ColOwner)),
    ];
    let [_, resource, owner] = &columns;
    let table = if contexts.is_empty() {
        quiet(tr(Msg::TemplateNoContexts), None)
    } else {
        let rows = contexts
            .iter()
            .map(|program| {
                let owner_tag = program
                    .file
                    .as_deref()
                    .and_then(|path| listing.iter().find(|each| each.path == path))
                    .map(|listed| owner_tag(listed.owner));
                let href = program_href(program).or_else(|| program.file.as_deref().map(file_href));
                view! {
                    <tr>
                        {title_cell(href.as_deref(), program.context.clone(), None)}
                        {cell(resource, program.resource_type.clone())}
                        {cell(owner, owner_tag)}
                    </tr>
                }
            })
            .collect_view()
            .into_any();
        data_table(DataTable {
            caption: &tr(Msg::TemplateContexts),
            columns: &columns,
            sorting: Sorting::default(),
            body: Body::Rows(rows),
        })
        .into_any()
    };
    let count = u64::try_from(contexts.len()).unwrap_or(u64::MAX);
    let body = panel_sections_card(
        tr(Msg::ColTemplate),
        view! {
            {section(tr(Msg::ColTemplate), None, facts_row(facts).into_any())}
            {section(tr(Msg::TemplateContexts), Some(count), table)}
        },
    );
    let subtitle = match template {
        Template::Directory { .. } => tr(Msg::TemplateFromDirectory),
        Template::Cdr { .. } => tr(Msg::TemplateFromCdr),
    };
    detail_page(DetailPage {
        header: Header {
            title: id.to_owned().into(),
            subtitle: Some(subtitle.into()),
            subtitle_view: None,
            crumbs: vec![Crumb::new(
                tr(Msg::MappingSet),
                format!("{}/templates", crate::app::MAPPINGS),
            )],
            mono: true,
            badge: None,
        },
        actions: ActionGroup::none(),
        stepper: None,
        tabs: None,
        body,
    })
    .into_any()
}
