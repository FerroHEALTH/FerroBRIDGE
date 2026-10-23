// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The OMOP ETL: a detail page (T2) of what a run does.
//!
//! It says what a run reads, maps and writes, and how runs are started. The
//! ETL is a batch of its own process
//! (`ferrobridge etl run`), so the page says how it runs rather than what
//! a run did; the run reports are planned work (#612).

use ferrobridge_operator_api::health::Info;
use ferrohealth_ui::tone::Tone;
use ferrohealth_ui::view::action::ActionGroup;
use ferrohealth_ui::view::status::badge;
use ferrohealth_ui::view::surface::{Header, definitions, panel_sections_card};
use ferrohealth_ui::view::template::detail::{DetailPage, detail_page};
use leptos::prelude::*;

use crate::app::{CONNECTIONS, MAPPINGS, OMOP, page_title};
use crate::load::Ready;
use crate::locale::messages::Msg;
use crate::locale::tr;
use crate::model::{Loaded, Outcome};
use crate::pages::facade::{indicator, lane_on};
use crate::pages::parts::{faulted, quiet, refused, section};

/// The OMOP ETL.
#[must_use]
pub fn page() -> impl IntoView {
    view! {
        {page_title(tr(Msg::OmopEtl))}
        <Suspense>
            {move || Suspend::new(async move {
                let info = crate::load::info().await;
                let readiness = crate::load::readiness().await;
                drawn(&info, &readiness)
            })}
        </Suspense>
    }
}

/// The page over the lane and the CDM database's readiness.
fn drawn(info: &Loaded<Info>, readiness: &Loaded<Ready>) -> AnyView {
    let info = match info {
        Ok(Outcome::Shown(info)) => info,
        Ok(Outcome::Refused(refusal)) => return refused(refusal),
        Err(fault) => return faulted(fault, OMOP),
    };
    let on = lane_on(info, "etl");
    let header = Header {
        title: tr(Msg::OmopEtl).into(),
        subtitle: Some(tr(Msg::OmopSubtitle).into()),
        subtitle_view: None,
        crumbs: Vec::new(),
        mono: false,
        badge: Some(
            badge(
                if on { Tone::Ok } else { Tone::Neutral },
                tr(if on {
                    Msg::StateConfigured
                } else {
                    Msg::StateNotConfigured
                }),
            )
            .into_any(),
        ),
    };
    if !on {
        return detail_page(DetailPage {
            header,
            actions: ActionGroup::none(),
            stepper: None,
            tabs: None,
            body: ferrohealth_ui::view::surface::panel(quiet(tr(Msg::OmopOff), None)).into_any(),
        });
    }
    let database = match readiness {
        Ok(Outcome::Shown((_, readiness))) => indicator(readiness, "cdm"),
        Ok(Outcome::Refused(_)) | Err(_) => String::from("–").into_any(),
    };
    let flow = definitions(vec![
        (
            tr(Msg::RunReads),
            view! { <a href=format!("{CONNECTIONS}/cdr")>{tr(Msg::RunReadsValue)}</a> }.into_any(),
        ),
        (
            tr(Msg::RunMaps),
            view! { <a href=format!("{MAPPINGS}/omocl")>{tr(Msg::RunMapsValue)}</a> }.into_any(),
        ),
        (
            tr(Msg::RunWrites),
            view! { <a href=format!("{CONNECTIONS}/cdm")>{tr(Msg::UpstreamCdm)}</a> }.into_any(),
        ),
        (tr(Msg::RunDatabase), database),
    ])
    .into_any();
    let runs = definitions(vec![
        (
            tr(Msg::HowItRuns),
            view! { <code>"ferrobridge etl run"</code> }.into_any(),
        ),
        (tr(Msg::RunReports), tr(Msg::RunReportsValue).into_any()),
    ])
    .into_any();
    detail_page(DetailPage {
        header,
        actions: ActionGroup::none(),
        stepper: None,
        tabs: None,
        body: panel_sections_card(
            tr(Msg::OmopEtl),
            view! {
                {section(tr(Msg::WhatARunDoes), None, flow)}
                {section(tr(Msg::Runs), None, runs)}
            },
        ),
    })
}
