// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The dashboard: three stat cards, Needs attention while something needs
//! it, and the bridge drawn as a flow (B9) from its callers through its
//! faces and the mapping set to its upstreams.
//!
//! The stats read the running load and the readiness indicators; the flow
//! reads the lanes `GET /health/info` names and the readiness indicators,
//! which fall back to `GET /health/readiness` while the operator API is off.

use std::borrow::Cow;

use ferrobridge_operator_api::health::{Info, Lane};
use ferrobridge_operator_api::operator::EngineState;
use ferrohealth_health::document::{Readiness, State};
use ferrohealth_ui::html::Html;
use ferrohealth_ui::i18n::Locale;
use ferrohealth_ui::icon::Glyph;
use ferrohealth_ui::table::view::{
    Body, Column, DataTable, Role, Sorting, cell, data_table, title_cell,
};
use ferrohealth_ui::tone::Tone;
use ferrohealth_ui::view::action::ActionGroup;
use ferrohealth_ui::view::button::Variant;
use ferrohealth_ui::view::flow::{
    EdgeRule, Flow, FlowCard, FlowEdge, FlowLane, FlowRule, FlowState, flow, rule_button,
    rule_dialog,
};
use ferrohealth_ui::view::state::{Mark, state};
use ferrohealth_ui::view::status::{badge, pill};
use ferrohealth_ui::view::surface::{
    Header, Stat, page_heading, stack, stat_card_marked, stat_grid, titled_panel,
};

use crate::app::{CONNECTIONS, Drawn, FACADE, HL7V2, MAPPINGS, OMOP};
use crate::locale::messages::Msg;
use crate::locale::{tr, tr_with};
use crate::markup::{h, t};
use crate::model::{Loaded, Outcome};
use crate::pages::parts::{faulted, operator_off, tally};
use crate::visit::Visit;

/// The names of the upstreams the readiness report checks, in the order
/// the flow draws them.
pub const UPSTREAMS: [&str; 3] = ["cdr", "terminology", "cdm"];

/// The dashboard.
pub async fn page(visit: &Visit) -> Drawn {
    let info = visit.reads.info().await;
    let readiness = visit.reads.readiness().await;
    let state = visit.reads.running().await;
    let bridge = visit
        .settings
        .as_ref()
        .map(|settings| settings.bridge.clone())
        .unwrap_or_default();
    Drawn::shell(
        visit.tr(Msg::Dashboard),
        drawn(&visit.locale, &bridge, &info, &readiness, &state),
    )
}

/// The dashboard of the bridge at `bridge` over what it answered.
fn drawn(
    l: &Locale,
    bridge: &str,
    info: &Loaded<Info>,
    readiness: &Loaded<Readiness>,
    state: &Loaded<EngineState>,
) -> Html {
    let header = page_heading(
        l,
        Header {
            title: tr(l, Msg::Dashboard).into(),
            subtitle: Some(
                tr_with(
                    l,
                    Msg::DashboardSubtitle,
                    &[("bridge", bridge.to_owned().into())],
                )
                .into(),
            ),
            ..Header::default()
        },
        ActionGroup::none(),
    );
    let (Ok(Outcome::Shown(info)), Ok(Outcome::Shown(readiness))) = (info, readiness) else {
        let fault = match (info, readiness) {
            (Err(fault), _) | (_, Err(fault)) => faulted(l, fault, crate::app::DASHBOARD),
            (Ok(Outcome::Refused(refusal)), _) | (_, Ok(Outcome::Refused(refusal))) => {
                crate::pages::parts::refused(l, refusal)
            }
            (Ok(Outcome::Shown(_)), Ok(Outcome::Shown(_))) => Html::empty(),
        };
        return header.then(fault);
    };
    let running = match state {
        Ok(Outcome::Shown(state)) => Some(state),
        Ok(Outcome::Refused(_)) | Err(_) => None,
    };
    let off = matches!(state, Ok(Outcome::Refused(refusal)) if refusal.not_served());
    let cards = stats(l, running, readiness, off);
    let attention = attention(l, info, readiness).unwrap_or_default();
    let notice = if off { operator_off(l) } else { Html::empty() };
    let flow = flow_panel(l, info, readiness, running);
    header.then(stack(Html::join([cards, notice, attention, flow])))
}

/// The three stat cards: the contexts loaded, the running load's errors and
/// the upstreams that answer.
fn stats(l: &Locale, state: Option<&EngineState>, readiness: &Readiness, off: bool) -> Html {
    let (contexts, load) = match state {
        Some(state) => running_cards(l, state),
        None => unknown_cards(l, off),
    };
    let upstreams = upstreams_card(l, readiness);
    stat_grid(Html::join([contexts, load, upstreams]))
}

/// The contexts and running load cards over the running load `state`.
fn running_cards(l: &Locale, state: &EngineState) -> (Html, Html) {
    let (errors, warnings) = tally(&state.diagnostics);
    let contexts = stat_card_marked(
        Stat {
            glyph: Glyph::LAYERS,
            value: tr_with(
                l,
                Msg::StatContextsValue,
                &[
                    ("programs", state.counts.programs.into()),
                    ("contexts", state.counts.contexts.into()),
                ],
            ),
            label: Cow::Owned(tr(l, Msg::StatContexts)),
            href: Some(String::from(MAPPINGS)),
        },
        badge(
            Tone::Neutral,
            tr_with(
                l,
                Msg::StatOmocl,
                &[("count", state.counts.omocl_files.into())],
            ),
        ),
    );
    let mark = if warnings > 0 {
        badge(
            Tone::Warn,
            tr_with(l, Msg::CountWarnings, &[("count", warnings.into())]),
        )
    } else {
        pill(Tone::Ok, &tr(l, Msg::StatClean))
    };
    let load = stat_card_marked(
        Stat {
            glyph: if errors > 0 {
                Glyph::CIRCLE_X
            } else {
                Glyph::CIRCLE_CHECK
            },
            value: tr_with(l, Msg::CountErrors, &[("count", errors.into())]),
            label: Cow::Owned(tr(l, Msg::StatRunningLoad)),
            href: Some(format!("{MAPPINGS}/diagnostics")),
        },
        mark,
    );
    (contexts, load)
}

/// The contexts and running load cards while the running load is not
/// known, saying why: the operator API is `off`, or the bridge did not say.
fn unknown_cards(l: &Locale, off: bool) -> (Html, Html) {
    let why = tr(
        l,
        if off {
            Msg::StatOperatorOff
        } else {
            Msg::StatNotKnown
        },
    );
    let card = |glyph: Glyph, label: Msg, why: String| {
        stat_card_marked(
            Stat {
                glyph,
                value: String::from("–"),
                label: Cow::Owned(tr(l, label)),
                href: Some(String::from(MAPPINGS)),
            },
            badge(Tone::Neutral, why),
        )
    };
    (
        card(Glyph::LAYERS, Msg::StatContexts, why.clone()),
        card(Glyph::CIRCLE_CHECK, Msg::StatRunningLoad, why),
    )
}

/// The upstreams card: how many of the upstreams the bridge checks answer.
fn upstreams_card(l: &Locale, readiness: &Readiness) -> Html {
    let checked = UPSTREAMS
        .iter()
        .filter_map(|name| {
            readiness
                .indicators
                .iter()
                .find(|indicator| indicator.name == *name)
        })
        .collect::<Vec<_>>();
    let up = checked
        .iter()
        .filter(|indicator| indicator.state == State::Up)
        .count();
    let down = checked.len().saturating_sub(up);
    let mark = if down == 0 {
        pill(Tone::Ok, &tr(l, Msg::StatAllUp))
    } else {
        badge(
            Tone::Bad,
            tr_with(l, Msg::StatUnreachable, &[("count", down.into())]),
        )
    };
    stat_card_marked(
        Stat {
            glyph: Glyph::PLUG,
            value: tr_with(
                l,
                Msg::StatUpstreamsValue,
                &[("up", up.into()), ("total", checked.len().into())],
            ),
            label: Cow::Owned(tr(l, Msg::StatUpstreams)),
            href: Some(String::from(CONNECTIONS)),
        },
        mark,
    )
}

/// The page of an upstream or a probe named `name`.
fn indicator_href(name: &str) -> String {
    match name {
        "hl7v2-listener" => String::from(HL7V2),
        "identity-store" => String::from(FACADE),
        other => format!("{CONNECTIONS}/{other}"),
    }
}

/// The words of an indicator named `name`.
#[must_use]
pub fn indicator_name(l: &Locale, name: &str) -> String {
    match name {
        "cdr" => tr(l, Msg::UpstreamCdr),
        "terminology" => tr(l, Msg::UpstreamTerminology),
        "cdm" => tr(l, Msg::UpstreamCdm),
        "hl7v2-listener" => tr(l, Msg::IndicatorHl7v2),
        "identity-store" => tr(l, Msg::IndicatorIdentityStore),
        other => other.to_owned(),
    }
}

/// Needs attention: every indicator that does not answer, with why; nothing
/// while every one answers.
fn attention(l: &Locale, _info: &Info, readiness: &Readiness) -> Option<Html> {
    let down = readiness
        .indicators
        .iter()
        .filter(|indicator| indicator.state == State::Down)
        .collect::<Vec<_>>();
    if down.is_empty() {
        return None;
    }
    let columns = [
        Column::text("what", tr(l, Msg::AttentionWhat)).role(Role::Title),
        Column::text("kind", tr(l, Msg::AttentionKind)).role(Role::Sub),
        Column::text("state", tr(l, Msg::AttentionState)),
        Column::text("why", tr(l, Msg::AttentionWhy)),
    ];
    let [_, kind, word, why] = &columns;
    let rows = down
        .iter()
        .map(|indicator| {
            let name = &indicator.name;
            let kind_word = if UPSTREAMS.contains(&name.as_str()) {
                tr(l, Msg::KindConnection)
            } else {
                tr(l, Msg::KindProbe)
            };
            let detail = indicator.detail.clone().unwrap_or_default();
            h(format!(
                "<tr>{}{}{}{}</tr>",
                title_cell(
                    Some(&indicator_href(name)),
                    t(indicator_name(l, name)),
                    None
                ),
                cell(kind, t(kind_word)),
                cell(word, state(Mark::Bad, tr(l, Msg::StateUnreachable), None)),
                cell(why, t(detail)),
            ))
        })
        .collect::<Html>();
    let count = u64::try_from(down.len()).unwrap_or(u64::MAX);
    let table = data_table(DataTable {
        caption: &tr(l, Msg::AttentionCaption),
        columns: &columns,
        sorting: Sorting::default(),
        body: Body::Rows(rows),
    });
    let title = format!("{} · {count}", tr(l, Msg::Attention));
    Some(titled_panel(title, Html::empty(), table))
}

/// The lane named `name` of `info`.
fn lane<'a>(info: &'a Info, name: &str) -> Option<&'a Lane> {
    info.lanes.iter().find(|lane| lane.name == name)
}

/// The state and line of the face lane named `name`.
fn face(l: &Locale, info: &Info, name: &str, on: Msg) -> (FlowState, String) {
    match lane(info, name) {
        Some(lane) if lane.enabled => match lane.up {
            Some(false) => (FlowState::Unreachable, tr(l, Msg::FlowListenerDown)),
            _ => match &lane.listen {
                Some(listen) => (
                    FlowState::Up,
                    tr_with(l, Msg::FlowListening, &[("listen", listen.clone().into())]),
                ),
                None => (FlowState::Up, tr(l, on)),
            },
        },
        _ => (FlowState::Off, tr(l, Msg::FlowOff)),
    }
}

/// The state and line of the upstream named `name`.
fn upstream(l: &Locale, readiness: &Readiness, name: &str) -> (FlowState, String, Option<String>) {
    match readiness
        .indicators
        .iter()
        .find(|indicator| indicator.name == name)
    {
        Some(indicator) if indicator.state == State::Up => {
            (FlowState::Up, tr(l, Msg::FlowUpstreamUp), None)
        }
        Some(_) => (
            FlowState::Unreachable,
            tr(l, Msg::FlowNoAnswer),
            Some(tr(l, Msg::FlowNoAnswerLabel)),
        ),
        None => (FlowState::NotChecked, tr(l, Msg::FlowNotConfigured), None),
    }
}

/// A card of the flow.
fn card(
    (id, glyph, name, line): (&str, Glyph, String, String),
    state: FlowState,
    href: String,
    path: &[&str],
) -> FlowCard {
    FlowCard {
        id: id.to_owned(),
        glyph,
        name,
        line,
        state,
        href,
        path: path.iter().map(|each| (*each).to_owned()).collect(),
        hub: id == "hub",
    }
}

/// An edge of the flow.
fn edge(from: &str, to: &str, state: FlowState, label: Option<String>) -> FlowEdge {
    FlowEdge {
        from: from.to_owned(),
        to: to.to_owned(),
        state,
        label,
    }
}

/// The states and lines of every card of the flow, read once.
struct Reading {
    /// The FHIR facade.
    facade: (FlowState, String),
    /// The operations.
    ops: (FlowState, String),
    /// The HL7 v2 face.
    hl7: (FlowState, String),
    /// The OMOP ETL.
    etl: (FlowState, String),
    /// The openEHR CDR, with its edge's label.
    record: (FlowState, String, Option<String>),
    /// The terminology server, with its edge's label.
    codes: (FlowState, String, Option<String>),
    /// The CDM database, with its edge's label.
    rows: (FlowState, String, Option<String>),
    /// The mapping set's line.
    hub: String,
}

impl Reading {
    /// The reading of `info`, `readiness` and the running load `state`.
    fn of(l: &Locale, info: &Info, readiness: &Readiness, state: Option<&EngineState>) -> Self {
        Self {
            facade: face(l, info, "facade", Msg::FlowFacadeOn),
            ops: face(l, info, "operations", Msg::FlowOperationsOn),
            hl7: face(l, info, "hl7v2", Msg::FlowHl7On),
            etl: face(l, info, "etl", Msg::FlowEtlOn),
            record: upstream(l, readiness, "cdr"),
            codes: upstream(l, readiness, "terminology"),
            rows: upstream(l, readiness, "cdm"),
            hub: state.map_or_else(
                || tr(l, Msg::FlowHubUnknown),
                |state| {
                    tr_with(
                        l,
                        Msg::FlowHubLine,
                        &[
                            ("contexts", state.counts.contexts.into()),
                            ("omocl", state.counts.omocl_files.into()),
                        ],
                    )
                },
            ),
        }
    }
}

/// The path from the FHIR clients through the facade to the CDR.
const TO_CDR: [&str; 4] = ["clients", "facade", "hub", "cdr"];

/// The path through the operations to the CDR.
const OPS_PATH: [&str; 4] = ["clients", "ops", "hub", "cdr"];

/// The path from the HL7 v2 senders to the CDR.
const HL7_PATH: [&str; 4] = ["senders", "hl7", "hub", "cdr"];

/// The path from an ETL run to the CDM database.
const ETL_PATH: [&str; 4] = ["run", "etl", "hub", "cdm"];

/// The path through the facade to the terminology server.
const TERM_PATH: [&str; 4] = ["clients", "facade", "hub", "term"];

/// The callers' lane.
fn callers(l: &Locale, reading: &Reading) -> FlowLane {
    FlowLane {
        title: tr(l, Msg::FlowCallers),
        cards: vec![
            card(
                (
                    "clients",
                    Glyph::USERS,
                    tr(l, Msg::FlowClients),
                    tr(l, Msg::FlowClientsLine),
                ),
                reading.facade.0,
                format!("{FACADE}/security"),
                &TO_CDR,
            ),
            card(
                (
                    "senders",
                    Glyph::SEND,
                    tr(l, Msg::FlowSenders),
                    tr(l, Msg::FlowSendersLine),
                ),
                reading.hl7.0,
                String::from(HL7V2),
                &HL7_PATH,
            ),
            card(
                (
                    "run",
                    Glyph::CLOCK,
                    tr(l, Msg::FlowRun),
                    tr(l, Msg::FlowRunLine),
                ),
                reading.etl.0,
                String::from(OMOP),
                &ETL_PATH,
            ),
        ],
        fold: None,
    }
}

/// The faces' lane.
fn faces(l: &Locale, reading: &Reading) -> FlowLane {
    FlowLane {
        title: tr(l, Msg::FlowFaces),
        cards: vec![
            card(
                (
                    "facade",
                    Glyph::FLAME,
                    tr(l, Msg::FhirFacade),
                    reading.facade.1.clone(),
                ),
                reading.facade.0,
                String::from(FACADE),
                &TO_CDR,
            ),
            card(
                (
                    "ops",
                    Glyph::FLASK_CONICAL,
                    tr(l, Msg::FlowOperations),
                    reading.ops.1.clone(),
                ),
                reading.ops.0,
                format!("{FACADE}/capability"),
                &OPS_PATH,
            ),
            card(
                (
                    "hl7",
                    Glyph::MESSAGE_SQUARE,
                    tr(l, Msg::Hl7v2),
                    reading.hl7.1.clone(),
                ),
                reading.hl7.0,
                String::from(HL7V2),
                &HL7_PATH,
            ),
            card(
                (
                    "etl",
                    Glyph::TABLE_2,
                    tr(l, Msg::OmopEtl),
                    reading.etl.1.clone(),
                ),
                reading.etl.0,
                String::from(OMOP),
                &ETL_PATH,
            ),
        ],
        fold: None,
    }
}

/// The upstreams' lane.
fn upstreams(l: &Locale, reading: &Reading) -> FlowLane {
    FlowLane {
        title: tr(l, Msg::FlowUpstreams),
        cards: vec![
            card(
                (
                    "cdr",
                    Glyph::DATABASE,
                    tr(l, Msg::UpstreamCdr),
                    reading.record.1.clone(),
                ),
                reading.record.0,
                format!("{CONNECTIONS}/cdr"),
                &TO_CDR,
            ),
            card(
                (
                    "term",
                    Glyph::BOOK_OPEN,
                    tr(l, Msg::UpstreamTerminology),
                    reading.codes.1.clone(),
                ),
                reading.codes.0,
                format!("{CONNECTIONS}/terminology"),
                &TERM_PATH,
            ),
            card(
                (
                    "cdm",
                    Glyph::TABLE_2,
                    tr(l, Msg::UpstreamCdm),
                    reading.rows.1.clone(),
                ),
                reading.rows.0,
                format!("{CONNECTIONS}/cdm"),
                &ETL_PATH,
            ),
        ],
        fold: None,
    }
}

/// The edges of the flow, each in the state of the card it reaches.
fn edges(reading: &Reading) -> Vec<FlowEdge> {
    vec![
        edge("clients", "facade", reading.facade.0, None),
        edge("clients", "ops", reading.ops.0, None),
        edge("senders", "hl7", reading.hl7.0, None),
        edge("run", "etl", reading.etl.0, None),
        edge("facade", "hub", reading.facade.0, None),
        edge("ops", "hub", reading.ops.0, None),
        edge("hl7", "hub", reading.hl7.0, None),
        edge("etl", "hub", reading.etl.0, None),
        edge("hub", "cdr", reading.record.0, reading.record.2.clone()),
        edge("hub", "term", reading.codes.0, reading.codes.2.clone()),
        edge("hub", "cdm", reading.rows.0, reading.rows.2.clone()),
    ]
}

/// The flow's foot line: the faces on and the upstreams that answer.
fn foot(l: &Locale, reading: &Reading) -> String {
    let faces = [reading.facade.0, reading.ops.0, reading.hl7.0]
        .iter()
        .filter(|face| **face != FlowState::Off)
        .count();
    let upstreams = [reading.record.0, reading.codes.0, reading.rows.0];
    let checked = upstreams
        .iter()
        .filter(|state| **state != FlowState::NotChecked)
        .count();
    let up = upstreams
        .iter()
        .filter(|state| **state == FlowState::Up)
        .count();
    tr_with(
        l,
        Msg::FlowFoot,
        &[
            ("faces", faces.into()),
            ("up", up.into()),
            ("total", checked.into()),
        ],
    )
}

/// The flow's panel: the bridge's callers, faces, mapping set and upstreams.
fn flow_panel(l: &Locale, info: &Info, readiness: &Readiness, state: Option<&EngineState>) -> Html {
    let reading = Reading::of(l, info, readiness, state);
    let hub = FlowLane {
        title: tr(l, Msg::FlowMappingSet),
        cards: vec![card(
            (
                "hub",
                Glyph::LAYERS,
                String::from("FerroBRIDGE"),
                reading.hub.clone(),
            ),
            FlowState::Up,
            String::from(MAPPINGS),
            &[],
        )],
        fold: None,
    };
    let drawn = flow(
        l,
        &Flow {
            id: String::from("bridge-flow"),
            label: tr(l, Msg::FlowLabel),
            list_label: tr(l, Msg::FlowListLabel),
            lanes: vec![
                callers(l, &reading),
                faces(l, &reading),
                hub,
                upstreams(l, &reading),
            ],
            edges: edges(&reading),
            foot: foot(l, &reading),
        },
    );
    let action = rule_button(l, FLOW_RULE).then(h(format!(
        r#"<a href="{href}" class="{class}">{words}{glyph}</a>"#,
        href = t(CONNECTIONS),
        class = Variant::Secondary.small(),
        words = t(tr(l, Msg::Connections)),
        glyph = ferrohealth_ui::icon::icon(Glyph::CHEVRON_RIGHT, "16"),
    )));
    let panel = titled_panel(tr(l, Msg::Flow), action, drawn);
    panel.then(flow_rule(l))
}

/// The id of the flow's "How the flow is drawn" dialog.
const FLOW_RULE: &str = "flow-rule";

/// "How the flow is drawn": each state the flow draws, its line and what it
/// means on this page.
fn flow_rule(l: &Locale) -> Html {
    let rule = |state, meaning| EdgeRule {
        state,
        meaning: tr(l, meaning),
    };
    rule_dialog(
        l,
        FlowRule {
            id: Cow::Borrowed(FLOW_RULE),
            rules: vec![
                rule(FlowState::Up, Msg::FlowRuleUp),
                rule(FlowState::Unreachable, Msg::FlowRuleUnreachable),
                rule(FlowState::NotChecked, Msg::FlowRuleNotChecked),
                rule(FlowState::Off, Msg::FlowRuleOff),
            ],
            note: tr(l, Msg::FlowRuleNote),
        },
    )
}
