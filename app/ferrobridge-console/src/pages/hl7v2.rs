// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The HL7 v2 face: a detail page (T2) of its MLLP listener.
//!
//! Overview reads the lane `GET /health/info` names and the listener's
//! readiness indicator, and says how the face maps a message; `ConceptMaps`
//! reads `GET /operator/hl7v2/concept-maps` and lists every map the face
//! runs, what it maps from and to, and the messages it served; Settings
//! shows the face's running configuration. A face that is off says so in
//! one quiet line on its Overview and its `ConceptMaps`.

use ferrobridge_operator_api::configuration::Configuration;
use ferrobridge_operator_api::health::Info;
use ferrobridge_operator_api::hl7v2::{ConceptMap, ConceptMaps, Ends, Kind, Origin};
use ferrohealth_ui::html::Html;
use ferrohealth_ui::i18n::Locale;
use ferrohealth_ui::table::view::{
    Body, Column, DataTable, Role, Sorting, cell, data_table, title_cell,
};
use ferrohealth_ui::view::action::ActionGroup;
use ferrohealth_ui::view::state::{Mark, state};
use ferrohealth_ui::view::surface::{definitions, facts_row, panel_sections};
use ferrohealth_ui::view::template::detail::{DetailPage, detail_page};

use crate::app::{Drawn, HL7V2};
use crate::locale::messages::Msg;
use crate::locale::{tr, tr_with};
use crate::markup::{code, h, t};
use crate::model::{Loaded, Outcome};
use crate::pages::facade::indicator;
use crate::pages::lane::Lane;
use crate::pages::parts::{faulted, quiet, refused_operator, section};
use crate::pages::settings::rows;
use crate::visit::Visit;
use ferrohealth_health::document::Readiness;

/// The tabs, each its key and its words.
const TABS: [(&str, Msg); 3] = [
    ("overview", Msg::TabOverview),
    ("concept-maps", Msg::TabConceptMaps),
    ("settings", Msg::TabSettings),
];

/// The page.
const LANE: Lane = Lane {
    path: HL7V2,
    title: Msg::Hl7v2,
    tabs: &TABS,
    on: Msg::Hl7v2OnSubtitle,
    off: Msg::Hl7v2OffSubtitle,
};

/// The HL7 v2 face, on the tab the route names.
pub async fn page(visit: &Visit) -> Drawn {
    let Some((tab, info, readiness, configuration)) = LANE.open(visit).await else {
        return crate::pages::errors::not_found(visit);
    };
    let directory = if tab == "concept-maps" && lane_on(&info) {
        Some(visit.reads.concept_maps().await)
    } else {
        None
    };
    Drawn::shell(
        visit.tr(Msg::Hl7v2),
        drawn(
            &visit.locale,
            &tab,
            &info,
            &readiness,
            (configuration.as_ref(), directory.as_ref()),
        ),
    )
}

/// Whether `info` names the face on.
fn lane_on(info: &Loaded<Info>) -> bool {
    matches!(info, Ok(Outcome::Shown(info)) if info.lanes.iter().any(|lane| lane.name == "hl7v2" && lane.enabled))
}

/// The page over the lane, the listener's readiness and, on Settings, the
/// running configuration, and on `ConceptMaps` the face's directory.
fn drawn(
    l: &Locale,
    tab: &str,
    info: &Loaded<Info>,
    readiness: &Loaded<Readiness>,
    (configuration, directory): (Option<&Loaded<Configuration>>, Option<&Loaded<ConceptMaps>>),
) -> Html {
    let info = match LANE.shown(l, info, tab) {
        Ok(info) => info,
        Err(page) => return page,
    };
    let lane = info
        .lanes
        .iter()
        .find(|lane| lane.name == "hl7v2" && lane.enabled);
    let header = |on: bool| LANE.header(l, on);
    if let Some(configuration) = configuration.filter(|_| tab == "settings") {
        return LANE.framed(
            l,
            tab,
            header(lane.is_some()),
            crate::pages::settings::tab(l, configuration, &rows::HL7V2, &LANE.href(tab)),
        );
    }
    let Some(lane) = lane else {
        return LANE.framed(
            l,
            tab,
            header(false),
            quiet(
                tr(l, Msg::Hl7v2Off),
                Some((tr(l, Msg::SeeItsSettings), LANE.href("settings"))),
            ),
        );
    };
    if let Some(directory) = directory.filter(|_| tab == "concept-maps") {
        return LANE.framed(
            l,
            tab,
            header(true),
            concept_maps(l, directory, &LANE.href(tab)),
        );
    }
    let listening = match readiness {
        Ok(Outcome::Shown(readiness)) => indicator(l, readiness, "hl7v2-listener"),
        Ok(Outcome::Refused(_)) | Err(_) => t("–"),
    };
    let address = lane.listen.clone().unwrap_or_else(|| String::from("–"));
    let facts = facts_row(vec![
        (tr(l, Msg::FactAddress), code(address)),
        (tr(l, Msg::FactListener), listening),
    ]);
    let mapping = definitions(vec![
        (tr(l, Msg::FactGuide), code("hl7.fhir.uv.v2mappings")),
        (
            tr(l, Msg::FactAcknowledgment),
            t(tr(l, Msg::AcknowledgmentValue)),
        ),
        (tr(l, Msg::FactCommit), t(tr(l, Msg::CommitValue))),
    ]);
    detail_page(
        l,
        DetailPage {
            header: header(true),
            actions: ActionGroup::none(),
            stepper: None,
            tabs: Some(LANE.tab_row(l, tab, |_, drawn| Some(drawn))),
            body: panel_sections(section(l, tr(l, Msg::Listener), None, facts).then(section(
                l,
                tr(l, Msg::Mapping),
                None,
                mapping,
            ))),
        },
    )
}

/// The `ConceptMaps` tab over the directory `directory`.
fn concept_maps(l: &Locale, directory: &Loaded<ConceptMaps>, href: &str) -> Html {
    let directory = match directory {
        Ok(Outcome::Shown(directory)) => directory,
        Ok(Outcome::Refused(refusal)) => return refused_operator(l, refusal),
        Err(fault) => return faulted(l, fault, href),
    };
    let supplements = if directory.supplements.is_empty() {
        t(tr(l, Msg::SupplementsNone))
    } else {
        t(directory.supplements.join(", "))
    };
    let facts = definitions(vec![
        (tr(l, Msg::FactGuideDirectory), code(&directory.directory)),
        (tr(l, Msg::RowSupplements), supplements),
        (tr(l, Msg::FactCountedSince), code(&directory.counted_since)),
    ]);
    let count = u64::try_from(directory.maps.len()).unwrap_or(u64::MAX);
    let table = if directory.maps.is_empty() {
        quiet(tr(l, Msg::ConceptMapsNone), None)
    } else {
        map_table(l, &directory.maps)
    };
    panel_sections(
        section(l, tr(l, Msg::ConceptMapDirectory), None, facts).then(section(
            l,
            tr(l, Msg::TabConceptMaps),
            Some(count),
            table,
        )),
    )
}

/// The table of `maps`: each one's title and id, kind, what it maps from
/// and to, origin, rows and the messages it served.
fn map_table(l: &Locale, maps: &[ConceptMap]) -> Html {
    let columns = [
        Column::text("map", tr(l, Msg::ColMap)).role(Role::Title),
        Column::text("kind", tr(l, Msg::ColKind)),
        Column::text("source", tr(l, Msg::ColSource)),
        Column::text("target", tr(l, Msg::ColTarget)),
        Column::text("origin", tr(l, Msg::ColOrigin)),
        Column::text("rows", tr(l, Msg::ColRows)).numeric(),
        Column::text("messages", tr(l, Msg::ColMessages)).numeric(),
    ];
    let [_, kind, source, target, origin, rows, messages] = &columns;
    let count = |value: u64| t(tr_with(l, Msg::ValueCount, &[("count", value.into())]));
    let body = maps
        .iter()
        .map(|map| {
            let ends = |pick: fn(&Ends) -> &str| {
                if map.ends.is_empty() {
                    return t("–");
                }
                map.ends.iter().map(|end| code(pick(end))).collect::<Html>()
            };
            h(format!(
                "<tr>{}{}{}{}{}{}{}</tr>",
                title_cell(
                    None,
                    t(map.title.as_deref().unwrap_or(&map.id)),
                    Some(code(&map.id))
                ),
                cell(kind, t(tr(l, kind_words(map.kind)))),
                cell(source, ends(|end| end.source.as_str())),
                cell(target, ends(|end| end.target.as_str())),
                cell(origin, origin_mark(l, map.origin)),
                cell(rows, count(map.rows)),
                cell(messages, count(map.messages)),
            ))
        })
        .collect::<Html>();
    data_table(DataTable {
        caption: &tr(l, Msg::TabConceptMaps),
        columns: &columns,
        sorting: Sorting::default(),
        body: Body::Rows(body),
    })
}

/// The words of `kind`.
const fn kind_words(kind: Kind) -> Msg {
    match kind {
        Kind::Message => Msg::MapKindMessage,
        Kind::Segment => Msg::MapKindSegment,
        Kind::Datatype => Msg::MapKindDatatype,
        Kind::Table => Msg::MapKindTable,
        Kind::Other => Msg::MapKindOther,
    }
}

/// Where a map came from, as a word with its mark.
fn origin_mark(l: &Locale, origin: Origin) -> Html {
    match origin {
        Origin::Guide => state(Mark::Neutral, tr(l, Msg::MapOriginGuide), None),
        Origin::Supplement => state(Mark::Ok, tr(l, Msg::MapOriginSupplement), None),
        Origin::Override => state(Mark::Warn, tr(l, Msg::MapOriginOverride), None),
    }
}
