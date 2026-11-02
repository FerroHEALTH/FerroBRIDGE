// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The events lane: a detail page (T2) of the change-feed consumers and the
//! AMQP topology.
//!
//! The page reads `GET /operator/events` and draws each consumer's source,
//! reader and position, its health, what it published since the bridge
//! started and its last failure with the cause, then the exchange, the
//! routing-key grammar and the bound queues. It changes nothing: the
//! configuration is where the topology changes. A bridge whose lane is off
//! says so in one quiet line.

use ferrobridge_operator_api::events::{Consumer, Events, Position, Source, State, Topology};
use ferrohealth_ui::html::Html;
use ferrohealth_ui::i18n::Locale;
use ferrohealth_ui::table::view::{
    Body, Column, DataTable, Role, Sorting, cell, data_table, title_cell,
};
use ferrohealth_ui::tone::Tone;
use ferrohealth_ui::view::action::ActionGroup;
use ferrohealth_ui::view::state::{Mark, state};
use ferrohealth_ui::view::status::badge;
use ferrohealth_ui::view::surface::{Header, definitions, panel_sections};
use ferrohealth_ui::view::template::detail::{DetailPage, detail_page};

use crate::app::{CONFIGURATION, Drawn, EVENTS};
use crate::locale::messages::Msg;
use crate::locale::{tr, tr_with};
use crate::markup::{code, h, t};
use crate::model::{Loaded, Outcome};
use crate::pages::parts::{faulted, quiet, refused_operator, section};
use crate::visit::Visit;

/// The events page.
pub async fn page(visit: &Visit) -> Drawn {
    let events = visit.reads.events().await;
    Drawn::shell(visit.tr(Msg::Events), drawn(&visit.locale, &events))
}

/// The page over the document `events`.
fn drawn(l: &Locale, events: &Loaded<Events>) -> Html {
    let events = match events {
        Ok(Outcome::Shown(events)) => events,
        Ok(Outcome::Refused(refusal)) => return page_with(l, None, refused_operator(l, refusal)),
        Err(fault) => return page_with(l, None, faulted(l, fault, EVENTS)),
    };
    let Some(topology) = events
        .topology
        .as_ref()
        .filter(|_| !events.consumers.is_empty())
    else {
        return page_with(
            l,
            Some(&[]),
            quiet(
                tr(l, Msg::EventsOff),
                Some((tr(l, Msg::EventsOffLink), String::from(CONFIGURATION))),
            ),
        );
    };
    let consumers = &events.consumers;
    let body = section(
        l,
        tr(l, Msg::EventsConsumers),
        Some(u64::try_from(consumers.len()).unwrap_or(u64::MAX)),
        consumer_table(l, consumers),
    )
    .then(section(
        l,
        tr(l, Msg::EventsFailures),
        None,
        failures(l, consumers),
    ))
    .then(section(
        l,
        tr(l, Msg::EventsTopology),
        None,
        definitions(vec![
            (tr(l, Msg::FactExchange), code(&topology.exchange)),
            (tr(l, Msg::FactRoutingKey), code(&topology.routing_key)),
        ]),
    ))
    .then(section(
        l,
        tr(l, Msg::EventsQueues),
        Some(u64::try_from(topology.queues.len()).unwrap_or(u64::MAX)),
        queues(l, topology),
    ));
    page_with(l, Some(consumers), panel_sections(body))
}

/// The detail page around `body`, its badge from `consumers`: none known
/// when `None`, the lane off when empty.
fn page_with(l: &Locale, consumers: Option<&[Consumer]>, body: Html) -> Html {
    let on = consumers.is_some_and(|consumers| !consumers.is_empty());
    let badge_of = consumers.map(|consumers| {
        if consumers.is_empty() {
            badge(Tone::Neutral, tr(l, Msg::StateOff))
        } else if consumers
            .iter()
            .any(|consumer| consumer.state == State::Stalled)
        {
            state(Mark::Warn, tr(l, Msg::StateStalled), None)
        } else if consumers
            .iter()
            .any(|consumer| consumer.state == State::Starting)
        {
            state(Mark::Neutral, tr(l, Msg::StateStarting), None)
        } else {
            state(Mark::Ok, tr(l, Msg::StateFollowing), None)
        }
    });
    detail_page(
        l,
        DetailPage {
            header: Header {
                title: tr(l, Msg::Events).into(),
                subtitle: Some(
                    tr(
                        l,
                        if on {
                            Msg::EventsOnSubtitle
                        } else {
                            Msg::EventsOffSubtitle
                        },
                    )
                    .into(),
                ),
                subtitle_view: None,
                crumbs: Vec::new(),
                mono: false,
                badge: badge_of,
            },
            actions: ActionGroup::none(),
            stepper: None,
            tabs: None,
            body,
        },
    )
}

/// The table of `consumers`: each one's reader, source, position, state and
/// what it published.
fn consumer_table(l: &Locale, consumers: &[Consumer]) -> Html {
    let columns = [
        Column::text("consumer", tr(l, Msg::ColConsumer)).role(Role::Title),
        Column::text("source", tr(l, Msg::ColSource)).role(Role::Sub),
        Column::text("position", tr(l, Msg::ColPosition)),
        Column::text("state", tr(l, Msg::ColState)),
        Column::text("published", tr(l, Msg::ColPublished)).numeric(),
        Column::text("last", tr(l, Msg::ColLastPublished)),
    ];
    let [
        _,
        source_column,
        position_column,
        state_column,
        published,
        last,
    ] = &columns;
    let rows = consumers
        .iter()
        .map(|consumer| {
            h(format!(
                "<tr>{}{}{}{}{}{}</tr>",
                title_cell(None, t(&consumer.name), Some(code(&consumer.reader))),
                cell(source_column, t(source_words(l, consumer.source))),
                cell(position_column, positions(l, &consumer.positions)),
                cell(state_column, consumer_state(l, consumer.state)),
                cell(
                    published,
                    t(tr_with(
                        l,
                        Msg::ValueCount,
                        &[("count", consumer.published.into())]
                    )),
                ),
                cell(
                    last,
                    consumer
                        .last_published
                        .as_deref()
                        .map_or_else(|| t("–"), code),
                ),
            ))
        })
        .collect::<Html>();
    data_table(DataTable {
        caption: &tr(l, Msg::EventsConsumers),
        columns: &columns,
        sorting: Sorting::default(),
        body: Body::Rows(rows),
    })
}

/// The words of `source`.
fn source_words(l: &Locale, source: Source) -> String {
    tr(
        l,
        match source {
            Source::Aql => Msg::SourceAql,
            Source::Ferroehr => Msg::SourceFerroehr,
        },
    )
}

/// The positions of one consumer, one per stream.
fn positions(l: &Locale, positions: &[Position]) -> Html {
    if positions.is_empty() {
        return t(tr(l, Msg::PositionNone));
    }
    let lines: Vec<String> = positions
        .iter()
        .map(|position| match position.versions_at_instant {
            Some(versions) => tr_with(
                l,
                Msg::PositionCommitted,
                &[
                    ("at", position.at.as_str().into()),
                    ("versions", versions.into()),
                ],
            ),
            None => tr_with(
                l,
                Msg::PositionOutbox,
                &[
                    ("stream", position.stream.as_str().into()),
                    ("at", position.at.as_str().into()),
                ],
            ),
        })
        .collect();
    t(lines.join("; "))
}

/// The state of a consumer, as a word with its mark.
fn consumer_state(l: &Locale, consumer: State) -> Html {
    match consumer {
        State::Following => state(Mark::Ok, tr(l, Msg::StateFollowing), None),
        State::Starting => state(Mark::Neutral, tr(l, Msg::StateStarting), None),
        State::Stalled => state(Mark::Warn, tr(l, Msg::StateStalled), None),
    }
}

/// The last failure of each consumer that failed, with when and why.
fn failures(l: &Locale, consumers: &[Consumer]) -> Html {
    let failed: Vec<(&Consumer, &ferrobridge_operator_api::events::Failure)> = consumers
        .iter()
        .filter_map(|consumer| {
            consumer
                .last_error
                .as_ref()
                .map(|failure| (consumer, failure))
        })
        .collect();
    if failed.is_empty() {
        return quiet(tr(l, Msg::EventsNoFailure), None);
    }
    failed
        .into_iter()
        .map(|(consumer, failure)| {
            definitions(vec![
                (tr(l, Msg::ColConsumer), t(&consumer.name)),
                (tr(l, Msg::FactFailedAt), code(&failure.at)),
                (tr(l, Msg::FactCause), code(&failure.cause)),
            ])
        })
        .collect()
}

/// The queues the lane binds, or the line that it binds none.
fn queues(l: &Locale, topology: &Topology) -> Html {
    if topology.queues.is_empty() {
        return quiet(tr(l, Msg::EventsNoQueue), None);
    }
    let columns = [
        Column::text("queue", tr(l, Msg::ColQueue)).role(Role::Title),
        Column::text("binding", tr(l, Msg::ColBinding)),
    ];
    let [_, binding] = &columns;
    let rows = topology
        .queues
        .iter()
        .map(|queue| {
            h(format!(
                "<tr>{}{}</tr>",
                title_cell(None, code(&queue.name), None),
                cell(binding, code(&queue.binding)),
            ))
        })
        .collect::<Html>();
    data_table(DataTable {
        caption: &tr(l, Msg::EventsQueues),
        columns: &columns,
        sorting: Sorting::default(),
        body: Body::Rows(rows),
    })
}
