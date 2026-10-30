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
use ferrobridge_operator_api::health::Info;
use ferrohealth_health::document::{Readiness, State};
use ferrohealth_ui::html::Html;
use ferrohealth_ui::i18n::Locale;
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

use crate::app::{CONNECTIONS, Drawn};
use crate::locale::messages::Msg;
use crate::locale::tr;
use crate::markup::{code, h, t};
use crate::model::{Loaded, Outcome};
use crate::pages::dashboard::{UPSTREAMS, indicator_name};
use crate::pages::facade::indicator;
use crate::pages::parts::{faulted, quiet, refused, section};
use crate::pages::settings::rows::{self, Credentials};
use crate::visit::Visit;

/// The lanes that use the upstream `name`, as `GET /health/info` names
/// them, each its words.
fn users(l: &Locale, info: &Info, name: &str) -> Vec<String> {
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
            tr(
                l,
                match lane.name.as_str() {
                    "facade" => Msg::FhirFacade,
                    "operations" => Msg::Playground,
                    "hl7v2" => Msg::Hl7v2,
                    _ => Msg::OmopEtl,
                },
            )
        })
        .collect()
}

/// The state of the upstream `name` as its indicator reports it, the tone
/// and the word: the readiness carries an indicator only for an upstream
/// the bridge is configured with.
fn upstream_state(l: &Locale, readiness: &Readiness, name: &str) -> Html {
    if readiness
        .indicators
        .iter()
        .any(|indicator| indicator.name == name)
    {
        indicator(l, readiness, name)
    } else {
        state(Mark::Neutral, tr(l, Msg::StateNotConfigured), None)
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
fn credentials_words(l: &Locale, configuration: &Configuration, name: &str) -> String {
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
            l,
            match configuration.entry("cdm.url").map(|entry| &entry.value) {
                Some(Value::Secret(SecretState::File { .. })) => Msg::CredentialsUrlFile,
                Some(Value::Secret(SecretState::Set)) => Msg::CredentialsUrl,
                _ => Msg::CredentialsNone,
            },
        );
    }
    tr(
        l,
        match rows::credentials(configuration, name) {
            Credentials::Oauth => Msg::CredentialsOauth,
            Credentials::Basic => Msg::CredentialsBasic,
            Credentials::None => Msg::CredentialsNone,
        },
    )
}

/// The page that lists the upstreams.
pub async fn page(visit: &Visit) -> Drawn {
    let info = visit.reads.info().await;
    let readiness = visit.reads.readiness().await;
    let configuration = visit.reads.configuration().await;
    Drawn::shell(
        visit.tr(Msg::Connections),
        list(&visit.locale, &info, &readiness, &configuration),
    )
}

/// The list over the lanes, the readiness and the running configuration.
fn list(
    l: &Locale,
    info: &Loaded<Info>,
    readiness: &Loaded<Readiness>,
    configuration: &Loaded<Configuration>,
) -> Html {
    let header = page_heading(
        l,
        Header {
            title: tr(l, Msg::Connections).into(),
            subtitle: Some(tr(l, Msg::ConnectionsSubtitle).into()),
            ..Header::default()
        },
        ActionGroup::none(),
    );
    let (info, readiness) = match (info, readiness) {
        (Ok(Outcome::Shown(info)), Ok(Outcome::Shown(readiness))) => (info, readiness),
        (Err(fault), _) | (_, Err(fault)) => {
            return header.then(faulted(l, fault, CONNECTIONS));
        }
        (Ok(Outcome::Refused(refusal)), _) | (_, Ok(Outcome::Refused(refusal))) => {
            return header.then(refused(l, refusal));
        }
    };
    let configuration = match configuration {
        Ok(Outcome::Shown(configuration)) => Some(configuration),
        Ok(Outcome::Refused(_)) | Err(_) => None,
    };
    let columns = [
        Column::text("connection", tr(l, Msg::ColConnection)).role(Role::Title),
        Column::text("host", tr(l, Msg::ColHost)).role(Role::Sub),
        Column::text("credentials", tr(l, Msg::ColCredentials)),
        Column::text("used", tr(l, Msg::ColUsedBy)).role(Role::Sub),
        Column::text("state", tr(l, Msg::ColState)).titled(tr(l, Msg::StateFromReadiness)),
    ];
    let [_, host_column, credentials_column, used, word] = &columns;
    let rows = UPSTREAMS
        .iter()
        .map(|name| {
            let by = users(l, info, name);
            let by = if by.is_empty() {
                String::from("–")
            } else {
                by.join(", ")
            };
            let address = configuration
                .and_then(|configuration| host(configuration, name))
                .map_or_else(|| t("–"), code);
            let signs_in = configuration.map_or_else(
                || String::from("–"),
                |configuration| credentials_words(l, configuration, name),
            );
            h(format!(
                "<tr>{}{}{}{}{}</tr>",
                title_cell(
                    Some(&format!("{CONNECTIONS}/{name}")),
                    t(indicator_name(l, name)),
                    None
                ),
                cell(host_column, address),
                cell(credentials_column, t(signs_in)),
                cell(used, t(by)),
                cell(word, upstream_state(l, readiness, name)),
            ))
        })
        .collect::<Html>();
    let table = data_table(DataTable {
        caption: &tr(l, Msg::Connections),
        columns: &columns,
        sorting: Sorting::default(),
        body: Body::Rows(rows),
    });
    let card = list_card(
        l,
        ListCard {
            title: tr(l, Msg::Connections),
            count: Some(u64::try_from(UPSTREAMS.len()).unwrap_or(u64::MAX).into()),
            order: None,
            tabs: None,
            actions: None,
            body: table,
            pager: None,
        },
    );
    header.then(h(format!(r#"<div class="fui-list-page">{card}</div>"#)))
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
pub async fn connection_page(visit: &Visit) -> Drawn {
    let name = visit.segment(1).unwrap_or_default().to_owned();
    let tab = visit.segment(2).unwrap_or("overview").to_owned();
    if !UPSTREAMS.contains(&name.as_str()) || !TABS.iter().any(|(key, _)| *key == tab) {
        return crate::pages::errors::not_found(visit);
    }
    let info = visit.reads.info().await;
    let readiness = visit.reads.readiness().await;
    let configuration = if tab == "settings" {
        Some(visit.reads.configuration().await)
    } else {
        None
    };
    let l = &visit.locale;
    Drawn::shell(
        indicator_name(l, &name),
        detail(l, &name, &tab, &info, &readiness, configuration.as_ref()),
    )
}

/// What the upstream `name` is, in one sentence.
fn subtitle(l: &Locale, name: &str) -> String {
    tr(
        l,
        match name {
            "cdr" => Msg::CdrSubtitle,
            "terminology" => Msg::TerminologySubtitle,
            _ => Msg::CdmSubtitle,
        },
    )
}

/// The detail page of the upstream `name`, on the tab `tab`.
fn detail(
    l: &Locale,
    name: &str,
    tab: &str,
    info: &Loaded<Info>,
    readiness: &Loaded<Readiness>,
    configuration: Option<&Loaded<Configuration>>,
) -> Html {
    let (info, readiness) = match (info, readiness) {
        (Ok(Outcome::Shown(info)), Ok(Outcome::Shown(readiness))) => (info, readiness),
        (Err(fault), _) | (_, Err(fault)) => {
            return faulted(l, fault, &tab_href(name, tab));
        }
        (Ok(Outcome::Refused(refusal)), _) | (_, Ok(Outcome::Refused(refusal))) => {
            return refused(l, refusal);
        }
    };
    let found = readiness
        .indicators
        .iter()
        .find(|indicator| indicator.name == name);
    let badge = match found {
        Some(found) if found.state == State::Up => state(Mark::Ok, tr(l, Msg::StateUp), None),
        Some(_) => state(Mark::Bad, tr(l, Msg::StateUnreachable), None),
        None => state(Mark::Neutral, tr(l, Msg::StateNotConfigured), None),
    };
    let warning = found
        .filter(|found| found.state == State::Down)
        .map(|found| {
            notice(
                Tone::Bad,
                Announce::Status,
                None,
                t(found
                    .detail
                    .clone()
                    .unwrap_or_else(|| tr(l, Msg::NoAnswerDetail))),
            )
        });
    let by = users(l, info, name);
    let used = if by.is_empty() {
        quiet(tr(l, Msg::UsedByNoLane), None)
    } else {
        definitions(
            by.into_iter()
                .map(|lane| (lane, t(tr(l, Msg::UsesIt))))
                .collect(),
        )
    };
    let states = definitions(vec![(
        tr(l, Msg::Readiness),
        upstream_state(l, readiness, name),
    )]);
    let body = match configuration.filter(|_| tab == "settings") {
        Some(configuration) => {
            let rows = match configuration {
                Ok(Outcome::Shown(shown)) => rows::connection(shown, name),
                Ok(Outcome::Refused(_)) | Err(_) => Vec::new(),
            };
            crate::pages::settings::tab(l, configuration, &rows, &tab_href(name, tab))
        }
        None if found.is_none() => quiet(tr(l, Msg::UpstreamNotConfigured), None),
        None => panel_sections(Html::join([
            warning
                .map(|warning| ferrohealth_ui::view::surface::panel_section(l, None, warning))
                .unwrap_or_default(),
            section(l, tr(l, Msg::State), None, states),
            section(l, tr(l, Msg::ColUsedBy), None, used),
        ])),
    };
    let tabs = TabRow {
        label: indicator_name(l, name),
        tabs: TABS
            .iter()
            .map(|(key, words)| Tab::new(*key, tab_href(name, key), tr(l, *words)))
            .collect(),
        current: tab.to_owned(),
    };
    detail_page(
        l,
        DetailPage {
            header: Header {
                title: indicator_name(l, name).into(),
                subtitle: Some(subtitle(l, name).into()),
                subtitle_view: None,
                crumbs: vec![Crumb::new(tr(l, Msg::Connections), CONNECTIONS)],
                mono: false,
                badge: Some(badge),
            },
            actions: ActionGroup::none(),
            stepper: None,
            tabs: Some(tabs),
            body,
        },
    )
}
