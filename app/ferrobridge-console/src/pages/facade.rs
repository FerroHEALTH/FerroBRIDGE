// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The FHIR facade, a face of the bridge: a detail page (T2) whose title
//! row carries its state, over binder tabs of panel sections (B10).
//!
//! Overview says what the facade serves, Capability lists the resource
//! types the bridge's `CapabilityStatement` declares (FHIR R4
//! `capabilitystatement.html`), each opening the program that serves it,
//! and Security shows the SMART discovery document (SMART App Launch
//! 2.2.0, conformance). A facade that is off keeps only Overview.

use ferrobridge_operator_api::capability::Capability;
use ferrobridge_operator_api::configuration::Configuration;
use ferrobridge_operator_api::health::Info;
use ferrobridge_operator_api::operator::EngineState;
use ferrohealth_health::document::{Readiness, State};
use ferrohealth_ui::highlight::Language;
use ferrohealth_ui::table::view::{
    Body, Column, DataTable, Role, Sorting, cell, data_table, title_cell,
};
use ferrohealth_ui::tone::Tone;
use ferrohealth_ui::view::action::ActionGroup;
use ferrohealth_ui::view::code::{Code, code_viewer};
use ferrohealth_ui::view::state::{Mark, state};
use ferrohealth_ui::view::status::badge;
use ferrohealth_ui::view::surface::{Header, definitions, facts_row, panel_sections};
use ferrohealth_ui::view::tabs::{Tab, TabRow};
use ferrohealth_ui::view::template::detail::{DetailPage, detail_page};
use leptos::prelude::*;
use leptos_router::hooks::use_params_map;

use crate::app::{FACADE, MAPPINGS, PLAYGROUND, page_title};
use crate::locale::messages::Msg;
use crate::locale::{tr, tr_with};
use crate::model::{Loaded, Outcome};
use crate::pages::parts::{faulted, quiet, refused, section};
use crate::pages::settings::rows;

/// The tabs of a facade that is on, each its key and its words.
const TABS: [(&str, Msg); 4] = [
    ("overview", Msg::TabOverview),
    ("capability", Msg::TabCapability),
    ("security", Msg::TabSecurity),
    ("settings", Msg::TabSettings),
];

/// The tabs of a facade that is off.
const OFF_TABS: [&str; 2] = ["overview", "settings"];

/// The page of the tab `key`.
fn tab_href(key: &str) -> String {
    if key == "overview" {
        String::from(FACADE)
    } else {
        format!("{FACADE}/{key}")
    }
}

/// The FHIR facade, on the tab the route names.
#[must_use]
pub fn page() -> impl IntoView {
    let tab = use_params_map().with_untracked(|params| {
        params
            .get("tab")
            .unwrap_or_else(|| String::from("overview"))
    });
    if !TABS.iter().any(|(key, _)| *key == tab) {
        return crate::pages::errors::not_found().into_any();
    }
    let info = Resource::new(crate::load::changes, |_| crate::load::info_now());
    let readiness = Resource::new(crate::load::changes, |_| crate::load::readiness_now());
    let capability = Resource::new(crate::load::changes, |_| crate::load::capability_now());
    let state = Resource::new(crate::load::changes, |_| crate::load::state_now());
    let smart = (tab == "security")
        .then(|| Resource::new(crate::load::changes, |_| crate::load::smart_now()));
    let configuration = (tab == "settings")
        .then(|| Resource::new(crate::load::changes, |_| crate::load::configuration_now()));
    view! {
        {page_title(tr(Msg::FhirFacade))}
        <Suspense>
            {move || {
                let tab = tab.clone();
                Suspend::new(async move {
                    let info = info.await;
                    let readiness = readiness.await;
                    let capability = capability.await;
                    let state = state.await;
                    let smart = match smart {
                        Some(smart) => Some(smart.await),
                        None => None,
                    };
                    let configuration = match configuration {
                        Some(configuration) => Some(configuration.await),
                        None => None,
                    };
                    let reads = Reads {
                        info: &info,
                        readiness: &readiness,
                        capability: &capability,
                        running: &state,
                        smart: smart.as_ref(),
                        configuration: configuration.as_ref(),
                    };
                    drawn(&tab, &reads)
                })
            }}
        </Suspense>
    }
    .into_any()
}

/// Whether the lane named `name` is on.
#[must_use]
pub fn lane_on(info: &Info, name: &str) -> bool {
    info.lanes
        .iter()
        .any(|lane| lane.name == name && lane.enabled)
}

/// What the page read from the bridge.
struct Reads<'a> {
    /// The lanes and pins.
    info: &'a Loaded<Info>,
    /// The readiness of every indicator.
    readiness: &'a Loaded<Readiness>,
    /// The `CapabilityStatement`.
    capability: &'a Loaded<Capability>,
    /// The running load.
    running: &'a Loaded<EngineState>,
    /// The SMART discovery document, on the Security tab.
    smart: Option<&'a Loaded<serde_json::Value>>,
    /// The running configuration, on the Settings tab.
    configuration: Option<&'a Loaded<Configuration>>,
}

/// Returns the Settings tab over `configuration`.
fn settings_panel(configuration: Option<&Loaded<Configuration>>) -> AnyView {
    match configuration {
        Some(configuration) => {
            crate::pages::settings::tab(configuration, &rows::FACADE, &tab_href("settings"))
        }
        None => ().into_any(),
    }
}

/// The page over what the bridge answered.
fn drawn(tab: &str, reads: &Reads<'_>) -> AnyView {
    let Reads {
        info,
        readiness,
        capability,
        running,
        smart,
        configuration,
    } = *reads;
    let info = match info {
        Ok(Outcome::Shown(info)) => info,
        Ok(Outcome::Refused(refusal)) => return refused(refusal),
        Err(fault) => return faulted(fault, &tab_href(tab)),
    };
    let on = lane_on(info, "facade");
    let header = Header {
        title: tr(Msg::FhirFacade).into(),
        subtitle: Some(
            tr(if on {
                Msg::FacadeOnSubtitle
            } else {
                Msg::FacadeOffSubtitle
            })
            .into(),
        ),
        subtitle_view: None,
        crumbs: Vec::new(),
        mono: false,
        badge: Some(if on {
            state(Mark::Ok, tr(Msg::StateOn), None)
        } else {
            badge(Tone::Neutral, tr(Msg::StateOff)).into_any()
        }),
    };
    if !on {
        let body = if tab == "settings" {
            settings_panel(configuration)
        } else {
            quiet(
                tr(Msg::FacadeOff),
                Some((tr(Msg::SeeItsSettings), tab_href("settings"))),
            )
        };
        let tabs = TabRow {
            label: tr(Msg::FhirFacade),
            tabs: TABS
                .iter()
                .filter(|(key, _)| OFF_TABS.contains(key))
                .map(|(key, words)| Tab::new(*key, tab_href(key), tr(*words)))
                .collect(),
            current: tab.to_owned(),
        };
        return detail_page(DetailPage {
            header,
            actions: ActionGroup::none(),
            stepper: None,
            tabs: Some(tabs),
            body,
        });
    }
    let statement = match capability {
        Ok(Outcome::Shown(statement)) => Some(statement),
        Ok(Outcome::Refused(_)) | Err(_) => None,
    };
    let body = match tab {
        "capability" => match capability {
            Ok(Outcome::Shown(statement)) => capability_panel(statement, running),
            Ok(Outcome::Refused(refusal)) => refused(refusal),
            Err(fault) => faulted(fault, &tab_href(tab)),
        },
        "security" => security_panel(smart),
        "settings" => settings_panel(configuration),
        _ => overview(info, readiness, statement, running),
    };
    let tabs = TabRow {
        label: tr(Msg::FhirFacade),
        tabs: TABS
            .iter()
            .map(|(key, words)| {
                let drawn = Tab::new(*key, tab_href(key), tr(*words));
                match (*key, capability) {
                    ("capability", Ok(Outcome::Shown(statement))) => drawn.count(
                        u64::try_from(resources(statement).len()).unwrap_or(u64::MAX),
                        tr(*words),
                    ),
                    _ => drawn,
                }
            })
            .collect(),
        current: tab.to_owned(),
    };
    detail_page(DetailPage {
        header,
        actions: ActionGroup::none(),
        stepper: None,
        tabs: Some(tabs),
        body,
    })
}

/// The resources of `statement`'s server `rest` blocks.
fn resources(statement: &Capability) -> Vec<&ferrobridge_operator_api::capability::Resource> {
    statement.resources.iter().collect()
}

/// The resource type a resource declares.
fn type_of(resource: &ferrobridge_operator_api::capability::Resource) -> String {
    resource.r#type.clone()
}

/// The Overview tab: the facade, what it serves, its operations and its
/// identity store.
fn overview(
    info: &Info,
    readiness: &Loaded<Readiness>,
    statement: Option<&Capability>,
    state: &Loaded<EngineState>,
) -> AnyView {
    let base = crate::visit::visit()
        .settings
        .map(|settings| format!("{}fhir", settings.bridge))
        .unwrap_or_default();
    let types = statement.map_or(0, |statement| resources(statement).len());
    let contexts = match state {
        Ok(Outcome::Shown(state)) => {
            view! { <a href=MAPPINGS>{state.contexts.len().to_string()}</a> }.into_any()
        }
        Ok(Outcome::Refused(_)) | Err(_) => String::from("–").into_any(),
    };
    let version = statement
        .and_then(|statement| statement.fhir_version.clone())
        .unwrap_or_else(|| String::from("–"));
    let facts = facts_row(vec![
        (tr(Msg::FactBase), view! { <code>{base}</code> }.into_any()),
        (tr(Msg::ColContexts), contexts),
        (
            tr(Msg::FactResourceTypes),
            view! { <a href=format!("{FACADE}/capability")>{types.to_string()}</a> }.into_any(),
        ),
        (tr(Msg::FactFhir), version.into_any()),
    ])
    .into_any();
    let searchable = statement.map_or(0, |statement| {
        resources(statement)
            .iter()
            .filter(|resource| !resource.search_params.is_empty())
            .count()
    });
    let validated = statement.map_or(0, |statement| {
        resources(statement)
            .iter()
            .filter(|resource| {
                resource
                    .operations
                    .iter()
                    .any(|operation| operation == "validate")
            })
            .count()
    });
    let serves = definitions(vec![
        (
            tr(Msg::ServesValidation),
            tr_with(Msg::ServesValidationValue, &[("count", validated.into())]).into_any(),
        ),
        (
            tr(Msg::ServesSearch),
            tr_with(Msg::ServesSearchValue, &[("count", searchable.into())]).into_any(),
        ),
    ])
    .into_any();
    let operations_on = lane_on(info, "operations");
    let ops = definitions(vec![
        (
            tr(Msg::FactLane),
            if operations_on {
                state_on()
            } else {
                badge(Tone::Neutral, tr(Msg::StateOff)).into_any()
            },
        ),
        (tr(Msg::FlowOperations), tr(Msg::OperationsValue).into_any()),
        (
            tr(Msg::TryThem),
            view! { <a href=PLAYGROUND>{tr(Msg::InPlayground)}</a> }.into_any(),
        ),
    ])
    .into_any();
    let identity = identity(readiness);
    panel_sections(view! {
        {section(tr(Msg::FhirFacade), None, facts)}
        {section(tr(Msg::WhatItServes), None, serves)}
        {section(tr(Msg::FlowOperations), None, ops)}
        {section(tr(Msg::Identity), None, identity)}
    })
}

/// The state "On", as a lane's facts say it.
fn state_on() -> AnyView {
    state(Mark::Ok, tr(Msg::StateOn), None)
}

/// The identity store's facts from the readiness of `readiness`.
fn identity(readiness: &Loaded<Readiness>) -> AnyView {
    let value = match readiness {
        Ok(Outcome::Shown(readiness)) => indicator(readiness, "identity-store"),
        Ok(Outcome::Refused(_)) | Err(_) => String::from("–").into_any(),
    };
    definitions(vec![(tr(Msg::IndicatorIdentityStore), value)]).into_any()
}

/// The state of the indicator `name`, with why when it is down.
#[must_use]
pub fn indicator(readiness: &Readiness, name: &str) -> AnyView {
    match readiness
        .indicators
        .iter()
        .find(|indicator| indicator.name == name)
    {
        Some(found) if found.state == State::Up => state(Mark::Ok, tr(Msg::StateUp), None),
        Some(found) => state(Mark::Bad, tr(Msg::StateUnreachable), found.detail.clone()),
        None => state(Mark::Neutral, tr(Msg::StateNotChecked), None),
    }
}

/// The Capability tab: every resource type the statement declares.
fn capability_panel(statement: &Capability, state: &Loaded<EngineState>) -> AnyView {
    let programs = match state {
        Ok(Outcome::Shown(state)) => state.contexts.clone(),
        Ok(Outcome::Refused(_)) | Err(_) => Vec::new(),
    };
    let columns = [
        Column::text("type", tr(Msg::ColResourceType)).role(Role::Title),
        Column::text("profiles", tr(Msg::ColProfiles))
            .numeric()
            .role(Role::Quiet),
        Column::text("interactions", tr(Msg::ColInteractions)).role(Role::Wide),
        Column::text("search", tr(Msg::ColSearchParams)).role(Role::Sub),
        Column::text("operations", tr(Msg::ColOperations)),
    ];
    let [_, profiles, interactions, search, operations] = &columns;
    let mut found = resources(statement);
    found.sort_by_key(|resource| type_of(resource));
    if found.is_empty() {
        return quiet(tr(Msg::NoResourceTypes), None);
    }
    let rows = found
        .iter()
        .map(|resource| {
            let kind = type_of(resource);
            let href = programs
                .iter()
                .find(|program| program.resource_type == kind)
                .and_then(crate::pages::mappings::program_href);
            let joined = |words: Vec<String>| {
                if words.is_empty() {
                    String::from("–")
                } else {
                    words.join(", ")
                }
            };
            let codes = joined(resource.interactions.clone());
            let params = joined(resource.search_params.clone());
            let ops = joined(
                resource
                    .operations
                    .iter()
                    .map(|name| format!("${name}"))
                    .collect(),
            );
            view! {
                <tr>
                    {title_cell(href.as_deref(), kind.clone(), None)}
                    {cell(profiles, resource.supported_profiles.to_string())}
                    {cell(interactions, codes)}
                    {cell(search, params)}
                    {cell(operations, view! { <code>{ops}</code> })}
                </tr>
            }
        })
        .collect_view()
        .into_any();
    let table = data_table(DataTable {
        caption: &tr(Msg::FactResourceTypes),
        columns: &columns,
        sorting: Sorting::default(),
        body: Body::Rows(rows),
    })
    .into_any();
    let intro =
        view! { <p class="fui-small fui-muted fui-section-intro">{tr(Msg::CapabilityIntro)}</p> };
    view! { {intro} {table} }.into_any()
}

/// The Security tab: the SMART discovery document the facade answers.
fn security_panel(smart: Option<&Loaded<serde_json::Value>>) -> AnyView {
    let body = match smart {
        Some(Ok(Outcome::Shown(document))) => {
            let capabilities = document
                .get("capabilities")
                .and_then(serde_json::Value::as_array)
                .map_or_else(
                    || String::from("–").into_any(),
                    |list| {
                        list.iter()
                            .filter_map(serde_json::Value::as_str)
                            .map(ferrohealth_ui::view::tag::tag)
                            .collect_view()
                            .into_any()
                    },
                );
            let issuer = document
                .get("issuer")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("–")
                .to_owned();
            let facts = definitions(vec![
                (
                    tr(Msg::FactIssuer),
                    view! { <code>{issuer}</code> }.into_any(),
                ),
                (tr(Msg::FactCapabilities), capabilities),
            ])
            .into_any();
            let text = serde_json::to_string_pretty(document).unwrap_or_default();
            let code = code_viewer(
                None,
                &Code {
                    label: tr(Msg::SmartDiscovery),
                    language: Language::Json,
                    text,
                    ids: None,
                    current: None,
                    diagnostics: Vec::new(),
                },
            );
            view! {
                {section(tr(Msg::WhatFacadeDeclares), None, facts)}
                {section(tr(Msg::SmartDiscovery), None, code)}
            }
            .into_any()
        }
        Some(Ok(Outcome::Refused(refusal))) if refusal.not_served() => {
            section(tr(Msg::SmartDiscovery), None, quiet(tr(Msg::NoSmart), None))
        }
        Some(Ok(Outcome::Refused(refusal))) => refused(refusal),
        Some(Err(fault)) => faulted(fault, &tab_href("security")),
        None => ().into_any(),
    };
    panel_sections(body)
}
