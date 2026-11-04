// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The change feed: a detail page (T2) of every change-feed consumer, the
//! events they parked and the AMQP topology.
//!
//! The page reads `GET /operator/events` and draws each consumer's source,
//! reader and position, its health, what it published since the bridge
//! started and its last failure with the cause, then the events each one
//! parked in the dead-letter store with a Replay form, then the exchange,
//! the routing-key grammar and the bound queues. Replay posts to the page,
//! which asks the bridge to mark the event and draws the page again with
//! the answer. A bridge with no consumer says so in one quiet line.

use axum::response::Response;
use ferrobridge_operator_api::events::{
    Consumer, Events, Parked, Position, Source, State, Topology,
};
use ferrohealth_ui::html::Html;
use ferrohealth_ui::i18n::Locale;
use ferrohealth_ui::icon::Glyph;
use ferrohealth_ui::table::view::{
    Body, Column, DataTable, Role, Sorting, cell, data_table, title_cell,
};
use ferrohealth_ui::tone::Tone;
use ferrohealth_ui::view::action::ActionGroup;
use ferrohealth_ui::view::button::Variant;
use ferrohealth_ui::view::notice::{Announce, notice_text};
use ferrohealth_ui::view::state::{Mark, state};
use ferrohealth_ui::view::status::badge;
use ferrohealth_ui::view::surface::{Header, definitions, panel_sections};
use ferrohealth_ui::view::template::detail::{DetailPage, detail_page};

use crate::actions::{ReplayAnswer, ReplayPost};
use crate::app::{CONFIGURATION, Drawn, EVENTS};
use crate::locale::messages::Msg;
use crate::locale::{tr, tr_with};
use crate::markup::{code, h, t};
use crate::model::{Loaded, Outcome};
use crate::pages::parts::{faulted, refused, refused_operator, section};
use crate::visit::Visit;
use ferrohealth_ui::view::template::list::list_empty;

/// The events page, with the answer to the replay the operator just asked
/// for when `replayed` holds one.
pub async fn page(visit: &Visit, replayed: Option<ReplayAnswer>) -> Drawn {
    let events = visit.reads.events().await;
    Drawn::shell(
        visit.tr(Msg::Events),
        drawn(&visit.locale, &events, replayed.as_ref()),
    )
}

/// Asks the bridge to replay the parked event the form body `body` names for
/// the operator of `visit`, and draws the page with its answer.
pub async fn posted(visit: &Visit, body: &str) -> Response {
    let post = ReplayPost::from_form(body);
    let answer = match visit.reads.signed_in() {
        Ok((state, token)) => crate::posts::replayed(state, token, &post).await,
        Err(fault) => ReplayAnswer::Answered(Err(fault)),
    };
    let drawn = page(visit, Some(answer)).await;
    crate::app::render(visit, drawn).await
}

/// The answer to a replay, in the place it was asked from.
fn answer(l: &Locale, replayed: &ReplayAnswer) -> Html {
    match replayed {
        ReplayAnswer::Invalid => {
            notice_text(Tone::Bad, Announce::Alert, tr(l, Msg::EventsReplayInvalid))
        }
        ReplayAnswer::Answered(Ok(Outcome::Shown(()))) => {
            notice_text(Tone::Ok, Announce::Status, tr(l, Msg::EventsReplayed))
        }
        ReplayAnswer::Answered(Ok(Outcome::Refused(refusal))) => refused(l, refusal),
        ReplayAnswer::Answered(Err(fault)) => faulted(l, fault, EVENTS),
    }
}

/// The page over the document `events`, with the answer `replayed`.
fn drawn(l: &Locale, events: &Loaded<Events>, replayed: Option<&ReplayAnswer>) -> Html {
    let shown = replayed.map(|replayed| answer(l, replayed));
    let events = match events {
        Ok(Outcome::Shown(events)) => events,
        Ok(Outcome::Refused(refusal)) => {
            return page_with(l, None, with_answer(shown, refused_operator(l, refusal)));
        }
        Err(fault) => return page_with(l, None, with_answer(shown, faulted(l, fault, EVENTS))),
    };
    if events.consumers.is_empty() {
        return page_with(
            l,
            Some(&[]),
            with_answer(
                shown,
                list_empty(
                    tr(l, Msg::EventsOff),
                    Some((tr(l, Msg::EventsOffLink), String::from(CONFIGURATION))),
                ),
            ),
        );
    }
    let consumers = &events.consumers;
    let parked_count: usize = consumers.iter().map(|consumer| consumer.parked.len()).sum();
    let mut body = section(
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
        tr(l, Msg::EventsParked),
        Some(u64::try_from(parked_count).unwrap_or(u64::MAX)),
        parked_table(l, consumers),
    ));
    if let Some(topology) = events.topology.as_ref() {
        body = body
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
    }
    page_with(l, Some(consumers), with_answer(shown, panel_sections(body)))
}

/// `body` after the answer `shown`, when there is one.
fn with_answer(shown: Option<Html>, body: Html) -> Html {
    match shown {
        Some(shown) => shown.then(body),
        None => body,
    }
}

/// The events every consumer parked, each with its Replay form, or the line
/// that none is parked.
fn parked_table(l: &Locale, consumers: &[Consumer]) -> Html {
    let parked: Vec<(&Consumer, &Parked)> = consumers
        .iter()
        .flat_map(|consumer| consumer.parked.iter().map(move |entry| (consumer, entry)))
        .collect();
    if parked.is_empty() {
        return list_empty(tr(l, Msg::EventsNoParked), None);
    }
    let columns = [
        Column::text("consumer", tr(l, Msg::ColConsumer)).role(Role::Title),
        Column::text("position", tr(l, Msg::ColPosition)),
        Column::text("contribution", tr(l, Msg::ColContribution)),
        Column::text("cause", tr(l, Msg::FactCause)),
        Column::text("attempts", tr(l, Msg::ColAttempts)).numeric(),
        Column::text("parked", tr(l, Msg::ColParkedAt)),
        Column::text("actions", tr(l, Msg::ColActions)),
    ];
    let [
        _,
        position_column,
        contribution,
        cause,
        attempts,
        at,
        actions,
    ] = &columns;
    let rows = parked
        .into_iter()
        .map(|(consumer, entry)| {
            h(format!(
                "<tr>{}{}{}{}{}{}{}</tr>",
                title_cell(None, t(&consumer.name), Some(code(&consumer.reader))),
                cell(
                    position_column,
                    positions(l, std::slice::from_ref(&entry.position))
                ),
                cell(
                    contribution,
                    entry
                        .contribution_id
                        .as_deref()
                        .map_or_else(|| t("–"), code),
                ),
                cell(cause, t(&entry.cause)),
                cell(
                    attempts,
                    t(tr_with(
                        l,
                        Msg::ValueCount,
                        &[("count", entry.attempts.into())]
                    )),
                ),
                cell(at, code(&entry.at)),
                cell(actions, replay(l, consumer, entry)),
            ))
        })
        .collect::<Html>();
    data_table(DataTable {
        caption: &tr(l, Msg::EventsParked),
        columns: &columns,
        sorting: Sorting::default(),
        body: Body::Rows(rows),
    })
}

/// The Replay form of `entry`, or the state of a replay already requested.
fn replay(l: &Locale, consumer: &Consumer, entry: &Parked) -> Html {
    if entry.replay_requested {
        return state(Mark::Neutral, tr(l, Msg::EventsReplayRequested), None);
    }
    h(format!(
        r#"<form method="post" action="{EVENTS}" class="fui-action-form"><input type="hidden" name="consumer" value="{consumer}"><input type="hidden" name="id" value="{id}"><button type="submit" class="{class}" aria-label="{label}">{glyph}{words}</button></form>"#,
        consumer = t(&consumer.name),
        id = entry.id,
        class = Variant::Secondary.class(),
        label = t(tr_with(
            l,
            Msg::EventsReplayLabel,
            &[
                ("id", entry.id.into()),
                ("consumer", consumer.name.as_str().into()),
            ],
        )),
        glyph = ferrohealth_ui::icon::icon(Glyph::ROTATE_CCW, "16"),
        words = t(tr(l, Msg::EventsReplay)),
    ))
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
        return list_empty(tr(l, Msg::EventsNoFailure), None);
    }
    failed
        .into_iter()
        .map(|(consumer, failure)| {
            definitions(vec![
                (tr(l, Msg::ColConsumer), t(&consumer.name)),
                (tr(l, Msg::FactFailedAt), code(&failure.at)),
                (tr(l, Msg::FactCause), t(&failure.cause)),
            ])
        })
        .collect()
}

/// The queues the lane binds, or the line that it binds none.
fn queues(l: &Locale, topology: &Topology) -> Html {
    if topology.queues.is_empty() {
        return list_empty(tr(l, Msg::EventsNoQueue), None);
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
