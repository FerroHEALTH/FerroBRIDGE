// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The upstreams the bridge reaches: a list page (T1) of the openEHR CDR,
//! the terminology server and the OMOP CDM database, and a detail page (T2)
//! for each.
//!
//! Each upstream's state is the bridge's readiness indicator for it, and the
//! faces that use it are the lanes `GET /health/info` names. Its host and
//! its credentials, by reference alone, are the bridge's running
//! configuration (`GET /operator/configuration`), which the detail page's
//! Settings tab shows whole.

use ferrobridge_operator_api::configuration::{Configuration, SecretState, Value};
use ferrobridge_operator_api::health::{Info, Readiness, State};
use ferrohealth_ui::table::view::{
    Body, Column, DataTable, Role, Sorting, cell, data_table, title_cell,
};
use ferrohealth_ui::tone::Tone;
use ferrohealth_ui::view::action::ActionGroup;
use ferrohealth_ui::view::notice::{Announce, notice};
use ferrohealth_ui::view::state::{Mark, state};
use ferrohealth_ui::view::surface::{Crumb, Header, definitions, page_heading, panel_sections};
use ferrohealth_ui::view::tabs::{Tab, TabRow};
use ferrohealth_ui::view::template::detail::{DetailPage, detail_page};
use ferrohealth_ui::view::template::list::{ListCard, list_card};
use leptos::prelude::*;
use leptos_router::hooks::use_params_map;

use crate::app::{CONNECTIONS, page_title};
use crate::load::Ready;
use crate::locale::messages::Msg;
use crate::locale::tr;
use crate::model::{Loaded, Outcome};
use crate::pages::dashboard::{UPSTREAMS, indicator_name};
use crate::pages::facade::indicator;
use crate::pages::parts::{faulted, quiet, refused, section};
use crate::pages::settings::rows::{self, Credentials};

/// The lanes that use the upstream `name`, as `GET /health/info` names
/// them, each its words.
fn users(info: &Info, name: &str) -> Vec<String> {
    let uses = |lane: &str| match name {
        "cdr" => matches!(lane, "facade" | "operations" | "hl7v2" | "etl"),
        "terminology" => matches!(lane, "facade" | "operations" | "hl7v2"),
        "cdm" => lane == "etl",
        _ => false,
    };
    info.lanes
        .iter()
        .filter(|lane| lane.enabled && uses(&lane.name))
        .map(|lane| {
            tr(match lane.name.as_str() {
                "facade" => Msg::FhirFacade,
                "operations" => Msg::Playground,
                "hl7v2" => Msg::Hl7v2,
                _ => Msg::OmopEtl,
            })
        })
        .collect()
}

/// The state of the upstream `name` as its indicator reports it, the tone
/// and the word: the readiness carries an indicator only for an upstream
/// the bridge is configured with.
fn upstream_state(readiness: &Readiness, name: &str) -> AnyView {
    if readiness.indicators.contains_key(name) {
        indicator(readiness, name)
    } else {
        state(Mark::Neutral, tr(Msg::StateNotConfigured), None)
    }
}

/// The host of the upstream `name`, from its base URL; the CDM database's
/// URL is a secret, so its host is never read.
fn host(configuration: &Configuration, name: &str) -> Option<String> {
    let entry = configuration.entry(&format!("{name}.base_url"))?;
    let Value::String(base) = &entry.value else {
        return None;
    };
    // NOTE: no specification governs this: our own design; a base URL that
    // does not parse shows no host, and the Settings tab shows it whole.
    let url = url::Url::parse(base).ok()?;
    url.host_str().map(str::to_owned)
}

/// How the upstream `name` signs in, in words, its secrets by reference;
/// a dash for an upstream the configuration has no section for.
fn credentials_words(configuration: &Configuration, name: &str) -> String {
    let section = format!("{name}.");
    if !configuration
        .entries
        .iter()
        .any(|entry| entry.key.starts_with(&section))
    {
        return String::from("–");
    }
    if name == "cdm" {
        return tr(
            match configuration.entry("cdm.url").map(|entry| &entry.value) {
                Some(Value::Secret(SecretState::File { .. })) => Msg::CredentialsUrlFile,
                Some(Value::Secret(SecretState::Set)) => Msg::CredentialsUrl,
                _ => Msg::CredentialsNone,
            },
        );
    }
    tr(match rows::credentials(configuration, name) {
        Credentials::Oauth => Msg::CredentialsOauth,
        Credentials::Basic => Msg::CredentialsBasic,
        Credentials::None => Msg::CredentialsNone,
    })
}

/// The page that lists the upstreams.
#[must_use]
pub fn page() -> impl IntoView {
    view! {
        {page_title(tr(Msg::Connections))}
        <Suspense>
            {move || Suspend::new(async move {
                let info = crate::load::info().await;
                let readiness = crate::load::readiness().await;
                let configuration = crate::load::configuration().await;
                list(&info, &readiness, &configuration)
            })}
        </Suspense>
    }
}

/// The list over the lanes, the readiness and the running configuration.
fn list(
    info: &Loaded<Info>,
    readiness: &Loaded<Ready>,
    configuration: &Loaded<Configuration>,
) -> AnyView {
    let header = page_heading(
        Header {
            title: tr(Msg::Connections).into(),
            subtitle: Some(tr(Msg::ConnectionsSubtitle).into()),
            ..Header::default()
        },
        ActionGroup::none(),
    )
    .into_any();
    let (info, readiness) = match (info, readiness) {
        (Ok(Outcome::Shown(info)), Ok(Outcome::Shown((_, readiness)))) => (info, readiness),
        (Err(fault), _) | (_, Err(fault)) => {
            return view! { {header} {faulted(fault, CONNECTIONS)} }.into_any();
        }
        (Ok(Outcome::Refused(refusal)), _) | (_, Ok(Outcome::Refused(refusal))) => {
            return view! { {header} {refused(refusal)} }.into_any();
        }
    };
    let configuration = match configuration {
        Ok(Outcome::Shown(configuration)) => Some(configuration),
        Ok(Outcome::Refused(_)) | Err(_) => None,
    };
    let columns = [
        Column::text("connection", tr(Msg::ColConnection)).role(Role::Title),
        Column::text("host", tr(Msg::ColHost)).role(Role::Sub),
        Column::text("credentials", tr(Msg::ColCredentials)),
        Column::text("used", tr(Msg::ColUsedBy)).role(Role::Sub),
        Column::text("state", tr(Msg::ColState)).titled(tr(Msg::StateFromReadiness)),
    ];
    let [_, host_column, credentials_column, used, word] = &columns;
    let rows = UPSTREAMS
        .iter()
        .map(|name| {
            let by = users(info, name);
            let by = if by.is_empty() {
                String::from("–")
            } else {
                by.join(", ")
            };
            let address = configuration
                .and_then(|configuration| host(configuration, name))
                .map_or_else(
                    || String::from("–").into_any(),
                    |host| view! { <code>{host}</code> }.into_any(),
                );
            let signs_in = configuration.map_or_else(
                || String::from("–"),
                |configuration| credentials_words(configuration, name),
            );
            view! {
                <tr>
                    {title_cell(Some(&format!("{CONNECTIONS}/{name}")), indicator_name(name), None)}
                    {cell(host_column, address)}
                    {cell(credentials_column, signs_in)}
                    {cell(used, by)}
                    {cell(word, upstream_state(readiness, name))}
                </tr>
            }
        })
        .collect_view()
        .into_any();
    let table = data_table(DataTable {
        caption: &tr(Msg::Connections),
        columns: &columns,
        sorting: Sorting::default(),
        body: Body::Rows(rows),
    })
    .into_any();
    let card = list_card(ListCard {
        title: tr(Msg::Connections),
        count: Some(u64::try_from(UPSTREAMS.len()).unwrap_or(u64::MAX).into()),
        order: None,
        tabs: None,
        actions: None,
        body: table,
        pager: None,
    });
    view! {
        {header}
        <div class="fui-list-page">{card}</div>
    }
    .into_any()
}

/// The tabs of a connection, each its key and its words.
const TABS: [(&str, Msg); 2] = [
    ("overview", Msg::TabOverview),
    ("settings", Msg::TabSettings),
];

/// The page of the tab `key` of the upstream `name`.
fn tab_href(name: &str, key: &str) -> String {
    if key == "overview" {
        format!("{CONNECTIONS}/{name}")
    } else {
        format!("{CONNECTIONS}/{name}/{key}")
    }
}

/// The page of the upstream the route names, on the tab it names.
#[must_use]
pub fn connection_page() -> impl IntoView {
    let (name, tab) = use_params_map().with_untracked(|params| {
        (
            params.get("name").unwrap_or_default(),
            params
                .get("tab")
                .unwrap_or_else(|| String::from("overview")),
        )
    });
    if !UPSTREAMS.contains(&name.as_str()) || !TABS.iter().any(|(key, _)| *key == tab) {
        return crate::pages::errors::not_found().into_any();
    }
    view! {
        {page_title(indicator_name(&name))}
        <Suspense>
            {move || {
                let name = name.clone();
                let tab = tab.clone();
                Suspend::new(async move {
                    let info = crate::load::info().await;
                    let readiness = crate::load::readiness().await;
                    let configuration = if tab == "settings" {
                        Some(crate::load::configuration().await)
                    } else {
                        None
                    };
                    detail(&name, &tab, &info, &readiness, configuration.as_ref())
                })
            }}
        </Suspense>
    }
    .into_any()
}

/// What the upstream `name` is, in one sentence.
fn subtitle(name: &str) -> String {
    tr(match name {
        "cdr" => Msg::CdrSubtitle,
        "terminology" => Msg::TerminologySubtitle,
        _ => Msg::CdmSubtitle,
    })
}

/// The detail page of the upstream `name`, on the tab `tab`.
fn detail(
    name: &str,
    tab: &str,
    info: &Loaded<Info>,
    readiness: &Loaded<Ready>,
    configuration: Option<&Loaded<Configuration>>,
) -> AnyView {
    let (info, readiness) = match (info, readiness) {
        (Ok(Outcome::Shown(info)), Ok(Outcome::Shown((_, readiness)))) => (info, readiness),
        (Err(fault), _) | (_, Err(fault)) => {
            return faulted(fault, &tab_href(name, tab));
        }
        (Ok(Outcome::Refused(refusal)), _) | (_, Ok(Outcome::Refused(refusal))) => {
            return refused(refusal);
        }
    };
    let found = readiness.indicators.get(name);
    let badge = match found {
        Some(found) if found.state == State::Up => state(Mark::Ok, tr(Msg::StateUp), None),
        Some(_) => state(Mark::Bad, tr(Msg::StateUnreachable), None),
        None => state(Mark::Neutral, tr(Msg::StateNotConfigured), None),
    };
    let warning = found
        .filter(|found| found.state == State::Down)
        .map(|found| {
            notice(
                Tone::Bad,
                Announce::Status,
                None,
                found
                    .detail
                    .clone()
                    .unwrap_or_else(|| tr(Msg::NoAnswerDetail)),
            )
            .into_any()
        });
    let by = users(info, name);
    let used = if by.is_empty() {
        quiet(tr(Msg::UsedByNoLane), None)
    } else {
        definitions(
            by.into_iter()
                .map(|lane| (lane, tr(Msg::UsesIt).into_any()))
                .collect(),
        )
        .into_any()
    };
    let states =
        definitions(vec![(tr(Msg::Readiness), upstream_state(readiness, name))]).into_any();
    let body = match configuration.filter(|_| tab == "settings") {
        Some(configuration) => {
            let rows = match configuration {
                Ok(Outcome::Shown(shown)) => rows::connection(shown, name),
                Ok(Outcome::Refused(_)) | Err(_) => Vec::new(),
            };
            crate::pages::settings::tab(configuration, &rows, &tab_href(name, tab))
        }
        None if found.is_none() => quiet(tr(Msg::UpstreamNotConfigured), None),
        None => panel_sections(view! {
            {warning.map(|warning| ferrohealth_ui::view::surface::panel_section(None, warning))}
            {section(tr(Msg::State), None, states)}
            {section(tr(Msg::ColUsedBy), None, used)}
        }),
    };
    let tabs = TabRow {
        label: indicator_name(name),
        tabs: TABS
            .iter()
            .map(|(key, words)| Tab::new(*key, tab_href(name, key), tr(*words)))
            .collect(),
        current: tab.to_owned(),
    };
    detail_page(DetailPage {
        header: Header {
            title: indicator_name(name).into(),
            subtitle: Some(subtitle(name).into()),
            subtitle_view: None,
            crumbs: vec![Crumb::new(tr(Msg::Connections), CONNECTIONS)],
            mono: false,
            badge: Some(badge),
        },
        actions: ActionGroup::none(),
        stepper: None,
        tabs: Some(tabs),
        body,
    })
}
