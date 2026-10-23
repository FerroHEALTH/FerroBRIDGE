// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The HL7 v2 face: a detail page (T2) of its MLLP listener.
//!
//! It reads the lane `GET /health/info` names and the listener's readiness
//! indicator, and says how the face maps a message. A face that is off says
//! so in one quiet line.

use ferrobridge_operator_api::health::Info;
use ferrohealth_ui::tone::Tone;
use ferrohealth_ui::view::action::ActionGroup;
use ferrohealth_ui::view::status::badge;
use ferrohealth_ui::view::surface::{Header, definitions, facts_row, panel_sections_card};
use ferrohealth_ui::view::template::detail::{DetailPage, detail_page};
use leptos::prelude::*;

use crate::app::{HL7V2, page_title};
use crate::load::Ready;
use crate::locale::messages::Msg;
use crate::locale::tr;
use crate::model::{Loaded, Outcome};
use crate::pages::facade::indicator;
use crate::pages::parts::{faulted, quiet, refused, section, state};

/// The HL7 v2 face.
#[must_use]
pub fn page() -> impl IntoView {
    view! {
        {page_title(tr(Msg::Hl7v2))}
        <Suspense>
            {move || Suspend::new(async move {
                let info = crate::load::info().await;
                let readiness = crate::load::readiness().await;
                drawn(&info, &readiness)
            })}
        </Suspense>
    }
}

/// The page over the lane and the listener's readiness.
fn drawn(info: &Loaded<Info>, readiness: &Loaded<Ready>) -> AnyView {
    let info = match info {
        Ok(Outcome::Shown(info)) => info,
        Ok(Outcome::Refused(refusal)) => return refused(refusal),
        Err(fault) => return faulted(fault, HL7V2),
    };
    let lane = info
        .lanes
        .iter()
        .find(|lane| lane.name == "hl7v2" && lane.enabled);
    let header = |on: bool| Header {
        title: tr(Msg::Hl7v2).into(),
        subtitle: Some(
            tr(if on {
                Msg::Hl7v2OnSubtitle
            } else {
                Msg::Hl7v2OffSubtitle
            })
            .into(),
        ),
        crumbs: Vec::new(),
        mono: false,
        badge: Some(if on {
            state(Tone::Ok, &tr(Msg::StateOn), None)
        } else {
            badge(Tone::Neutral, tr(Msg::StateOff)).into_any()
        }),
    };
    let Some(lane) = lane else {
        return detail_page(DetailPage {
            header: header(false),
            actions: ActionGroup::none(),
            stepper: None,
            tabs: None,
            body: ferrohealth_ui::view::surface::panel(quiet(tr(Msg::Hl7v2Off), None)).into_any(),
        });
    };
    let listening = match readiness {
        Ok(Outcome::Shown((_, readiness))) => indicator(readiness, "hl7v2-listener"),
        Ok(Outcome::Refused(_)) | Err(_) => String::from("–").into_any(),
    };
    let address = lane.listen.clone().unwrap_or_else(|| String::from("–"));
    let facts = facts_row(vec![
        (
            tr(Msg::FactAddress),
            view! { <code>{address}</code> }.into_any(),
        ),
        (tr(Msg::FactListener), listening),
    ])
    .into_any();
    let mapping = definitions(vec![
        (
            tr(Msg::FactGuide),
            view! { <code>"hl7.fhir.uv.v2mappings"</code> }.into_any(),
        ),
        (
            tr(Msg::FactAcknowledgment),
            tr(Msg::AcknowledgmentValue).into_any(),
        ),
        (tr(Msg::FactCommit), tr(Msg::CommitValue).into_any()),
    ])
    .into_any();
    detail_page(DetailPage {
        header: header(true),
        actions: ActionGroup::none(),
        stepper: None,
        tabs: None,
        body: panel_sections_card(
            tr(Msg::Hl7v2),
            view! {
                {section(tr(Msg::Listener), None, facts)}
                {section(tr(Msg::Mapping), None, mapping)}
            },
        ),
    })
}
