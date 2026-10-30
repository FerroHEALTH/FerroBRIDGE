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
use ferrohealth_ui::html::Html;
use ferrohealth_ui::i18n::Locale;
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

use crate::app::{Drawn, FACADE, MAPPINGS, PLAYGROUND};
use crate::locale::messages::Msg;
use crate::locale::{tr, tr_with};
use crate::markup::{code, h, link, t};
use crate::model::{Loaded, Outcome};
use crate::pages::parts::{faulted, quiet, refused, section};
use crate::pages::settings::rows;
use crate::visit::Visit;

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
pub async fn page(visit: &Visit) -> Drawn {
    let tab = visit.segment(1).unwrap_or("overview").to_owned();
    if !TABS.iter().any(|(key, _)| *key == tab) {
        return crate::pages::errors::not_found(visit);
    }
    let info = visit.reads.info().await;
    let readiness = visit.reads.readiness().await;
    let capability = visit.reads.capability().await;
    let state = visit.reads.running().await;
    let smart = if tab == "security" {
        Some(visit.reads.smart().await)
    } else {
        None
    };
    let configuration = if tab == "settings" {
        Some(visit.reads.configuration().await)
    } else {
        None
    };
    let base = visit
        .settings
        .as_ref()
        .map(|settings| format!("{}fhir", settings.bridge))
        .unwrap_or_default();
    let answers = Answers {
        info: &info,
        readiness: &readiness,
        capability: &capability,
        running: &state,
        smart: smart.as_ref(),
        configuration: configuration.as_ref(),
        base: &base,
    };
    Drawn::shell(
        visit.tr(Msg::FhirFacade),
        drawn(&visit.locale, &tab, &answers),
    )
}

/// Whether the lane named `name` is on.
#[must_use]
pub fn lane_on(info: &Info, name: &str) -> bool {
    info.lanes
        .iter()
        .any(|lane| lane.name == name && lane.enabled)
}

/// What the page read from the bridge.
struct Answers<'a> {
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
    /// The facade's base address, under the bridge's.
    base: &'a str,
}

/// Returns the Settings tab over `configuration`.
fn settings_panel(l: &Locale, configuration: Option<&Loaded<Configuration>>) -> Html {
    match configuration {
        Some(configuration) => {
            crate::pages::settings::tab(l, configuration, &rows::FACADE, &tab_href("settings"))
        }
        None => Html::empty(),
    }
}

/// The page over what the bridge answered.
fn drawn(l: &Locale, tab: &str, answers: &Answers<'_>) -> Html {
    let Answers {
        info,
        readiness,
        capability,
        running,
        smart,
        configuration,
        base,
    } = *answers;
    let info = match info {
        Ok(Outcome::Shown(info)) => info,
        Ok(Outcome::Refused(refusal)) => return refused(l, refusal),
        Err(fault) => return faulted(l, fault, &tab_href(tab)),
    };
    let on = lane_on(info, "facade");
    let header = Header {
        title: tr(l, Msg::FhirFacade).into(),
        subtitle: Some(
            tr(
                l,
                if on {
                    Msg::FacadeOnSubtitle
                } else {
                    Msg::FacadeOffSubtitle
                },
            )
            .into(),
        ),
        subtitle_view: None,
        crumbs: Vec::new(),
        mono: false,
        badge: Some(if on {
            state(Mark::Ok, tr(l, Msg::StateOn), None)
        } else {
            badge(Tone::Neutral, tr(l, Msg::StateOff))
        }),
    };
    if !on {
        let body = if tab == "settings" {
            settings_panel(l, configuration)
        } else {
            quiet(
                tr(l, Msg::FacadeOff),
                Some((tr(l, Msg::SeeItsSettings), tab_href("settings"))),
            )
        };
        let tabs = TabRow {
            label: tr(l, Msg::FhirFacade),
            tabs: TABS
                .iter()
                .filter(|(key, _)| OFF_TABS.contains(key))
                .map(|(key, words)| Tab::new(*key, tab_href(key), tr(l, *words)))
                .collect(),
            current: tab.to_owned(),
        };
        return detail_page(
            l,
            DetailPage {
                header,
                actions: ActionGroup::none(),
                stepper: None,
                tabs: Some(tabs),
                body,
            },
        );
    }
    let statement = match capability {
        Ok(Outcome::Shown(statement)) => Some(statement),
        Ok(Outcome::Refused(_)) | Err(_) => None,
    };
    let body = match tab {
        "capability" => match capability {
            Ok(Outcome::Shown(statement)) => capability_panel(l, statement, running),
            Ok(Outcome::Refused(refusal)) => refused(l, refusal),
            Err(fault) => faulted(l, fault, &tab_href(tab)),
        },
        "security" => security_panel(l, smart),
        "settings" => settings_panel(l, configuration),
        _ => overview(l, base, info, readiness, statement, running),
    };
    let tabs = TabRow {
        label: tr(l, Msg::FhirFacade),
        tabs: TABS
            .iter()
            .map(|(key, words)| {
                let drawn = Tab::new(*key, tab_href(key), tr(l, *words));
                match (*key, capability) {
                    ("capability", Ok(Outcome::Shown(statement))) => drawn.count(
                        u64::try_from(resources(statement).len()).unwrap_or(u64::MAX),
                        tr(l, *words),
                    ),
                    _ => drawn,
                }
            })
            .collect(),
        current: tab.to_owned(),
    };
    detail_page(
        l,
        DetailPage {
            header,
            actions: ActionGroup::none(),
            stepper: None,
            tabs: Some(tabs),
            body,
        },
    )
}

/// The resources of `statement`'s server `rest` blocks.
fn resources(statement: &Capability) -> Vec<&ferrobridge_operator_api::capability::Resource> {
    statement.resources.iter().collect()
}

/// The resource type a resource declares.
fn type_of(resource: &ferrobridge_operator_api::capability::Resource) -> String {
    resource.r#type.clone()
}

/// The Overview tab: the facade at `base`, what it serves, its operations
/// and its identity store.
fn overview(
    l: &Locale,
    base: &str,
    info: &Info,
    readiness: &Loaded<Readiness>,
    statement: Option<&Capability>,
    state: &Loaded<EngineState>,
) -> Html {
    let types = statement.map_or(0, |statement| resources(statement).len());
    let contexts = match state {
        Ok(Outcome::Shown(state)) => link(MAPPINGS, state.contexts.len().to_string()),
        Ok(Outcome::Refused(_)) | Err(_) => t("–"),
    };
    let version = statement
        .and_then(|statement| statement.fhir_version.clone())
        .unwrap_or_else(|| String::from("–"));
    let facts = facts_row(vec![
        (tr(l, Msg::FactBase), code(base)),
        (tr(l, Msg::ColContexts), contexts),
        (
            tr(l, Msg::FactResourceTypes),
            link(format!("{FACADE}/capability"), types.to_string()),
        ),
        (tr(l, Msg::FactFhir), t(version)),
    ]);
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
            tr(l, Msg::ServesValidation),
            t(tr_with(
                l,
                Msg::ServesValidationValue,
                &[("count", validated.into())],
            )),
        ),
        (
            tr(l, Msg::ServesSearch),
            t(tr_with(
                l,
                Msg::ServesSearchValue,
                &[("count", searchable.into())],
            )),
        ),
    ]);
    let operations_on = lane_on(info, "operations");
    let ops = definitions(vec![
        (
            tr(l, Msg::FactLane),
            if operations_on {
                state_on(l)
            } else {
                badge(Tone::Neutral, tr(l, Msg::StateOff))
            },
        ),
        (tr(l, Msg::FlowOperations), t(tr(l, Msg::OperationsValue))),
        (
            tr(l, Msg::TryThem),
            link(PLAYGROUND, tr(l, Msg::InPlayground)),
        ),
    ]);
    let identity = identity(l, readiness);
    panel_sections(Html::join([
        section(l, tr(l, Msg::FhirFacade), None, facts),
        section(l, tr(l, Msg::WhatItServes), None, serves),
        section(l, tr(l, Msg::FlowOperations), None, ops),
        section(l, tr(l, Msg::Identity), None, identity),
    ]))
}

/// The state "On", as a lane's facts say it.
fn state_on(l: &Locale) -> Html {
    state(Mark::Ok, tr(l, Msg::StateOn), None)
}

/// The identity store's facts from the readiness of `readiness`.
fn identity(l: &Locale, readiness: &Loaded<Readiness>) -> Html {
    let value = match readiness {
        Ok(Outcome::Shown(readiness)) => indicator(l, readiness, "identity-store"),
        Ok(Outcome::Refused(_)) | Err(_) => t("–"),
    };
    definitions(vec![(tr(l, Msg::IndicatorIdentityStore), value)])
}

/// The state of the indicator `name`, with why when it is down.
#[must_use]
pub fn indicator(l: &Locale, readiness: &Readiness, name: &str) -> Html {
    match readiness
        .indicators
        .iter()
        .find(|indicator| indicator.name == name)
    {
        Some(found) if found.state == State::Up => state(Mark::Ok, tr(l, Msg::StateUp), None),
        Some(found) => state(
            Mark::Bad,
            tr(l, Msg::StateUnreachable),
            found.detail.clone(),
        ),
        None => state(Mark::Neutral, tr(l, Msg::StateNotChecked), None),
    }
}

/// The Capability tab: every resource type the statement declares.
fn capability_panel(l: &Locale, statement: &Capability, state: &Loaded<EngineState>) -> Html {
    let programs = match state {
        Ok(Outcome::Shown(state)) => state.contexts.clone(),
        Ok(Outcome::Refused(_)) | Err(_) => Vec::new(),
    };
    let columns = [
        Column::text("type", tr(l, Msg::ColResourceType)).role(Role::Title),
        Column::text("profiles", tr(l, Msg::ColProfiles))
            .numeric()
            .role(Role::Quiet),
        Column::text("interactions", tr(l, Msg::ColInteractions)).role(Role::Wide),
        Column::text("search", tr(l, Msg::ColSearchParams)).role(Role::Sub),
        Column::text("operations", tr(l, Msg::ColOperations)),
    ];
    let [_, profiles, interactions, search, operations] = &columns;
    let mut found = resources(statement);
    found.sort_by_key(|resource| type_of(resource));
    if found.is_empty() {
        return quiet(tr(l, Msg::NoResourceTypes), None);
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
            h(format!(
                "<tr>{}{}{}{}{}</tr>",
                title_cell(href.as_deref(), t(&kind), None),
                cell(profiles, t(resource.supported_profiles.to_string())),
                cell(interactions, t(codes)),
                cell(search, t(params)),
                cell(operations, code(ops)),
            ))
        })
        .collect::<Html>();
    let table = data_table(DataTable {
        caption: &tr(l, Msg::FactResourceTypes),
        columns: &columns,
        sorting: Sorting::default(),
        body: Body::Rows(rows),
    });
    let intro = h(format!(
        r#"<p class="fui-small fui-muted fui-section-intro">{}</p>"#,
        t(tr(l, Msg::CapabilityIntro))
    ));
    intro.then(table)
}

/// The Security tab: the SMART discovery document the facade answers.
fn security_panel(l: &Locale, smart: Option<&Loaded<serde_json::Value>>) -> Html {
    let body = match smart {
        Some(Ok(Outcome::Shown(document))) => {
            let capabilities = document
                .get("capabilities")
                .and_then(serde_json::Value::as_array)
                .map_or_else(
                    || t("–"),
                    |list| {
                        list.iter()
                            .filter_map(serde_json::Value::as_str)
                            .map(ferrohealth_ui::view::tag::tag)
                            .collect::<Html>()
                    },
                );
            let issuer = document
                .get("issuer")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("–")
                .to_owned();
            let facts = definitions(vec![
                (tr(l, Msg::FactIssuer), code(issuer)),
                (tr(l, Msg::FactCapabilities), capabilities),
            ]);
            let text = serde_json::to_string_pretty(document).unwrap_or_default();
            let viewer = code_viewer(
                l,
                None,
                &Code {
                    label: tr(l, Msg::SmartDiscovery),
                    language: Language::Json,
                    text,
                    ids: None,
                    current: None,
                    diagnostics: Vec::new(),
                },
            );
            section(l, tr(l, Msg::WhatFacadeDeclares), None, facts).then(section(
                l,
                tr(l, Msg::SmartDiscovery),
                None,
                viewer,
            ))
        }
        Some(Ok(Outcome::Refused(refusal))) if refusal.not_served() => section(
            l,
            tr(l, Msg::SmartDiscovery),
            None,
            quiet(tr(l, Msg::NoSmart), None),
        ),
        Some(Ok(Outcome::Refused(refusal))) => refused(l, refusal),
        Some(Err(fault)) => faulted(l, fault, &tab_href("security")),
        None => Html::empty(),
    };
    panel_sections(body)
}
