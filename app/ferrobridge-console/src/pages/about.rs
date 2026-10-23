// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The About page: the Versions card with the bridge's build and pins, the
//! Manufacturer card, the console's own settings, and the bridge's
//! `GET /health/info` document as its self-description.

use ferrobridge_operator_api::health::Info;
use ferrohealth_ui::highlight::Language;
use ferrohealth_ui::view::surface::{Header, card, definitions};
use ferrohealth_ui::view::template::about::{
    AboutPage, Link, Manufacturer, SelfDescription, about_page,
};
use leptos::prelude::*;

use crate::app::{ABOUT, page_title};
use crate::locale::messages::Msg;
use crate::locale::tr;
use crate::model::{Loaded, Outcome};
use crate::pages::parts::{faulted, refused};

/// The route the self-description downloads from.
pub const INFO_JSON: &str = "/about/health-info.json";

/// The About page.
#[must_use]
pub fn page() -> impl IntoView {
    view! {
        {page_title(tr(Msg::About))}
        <Suspense>
            {move || Suspend::new(async move {
                let info = crate::load::info().await;
                drawn(&info)
            })}
        </Suspense>
    }
}

/// The page over the bridge's info.
fn drawn(info: &Loaded<Info>) -> AnyView {
    let footer = crate::shell::footer_names();
    let (bridge, locale) = crate::session::request::signed_in().map_or_else(
        |_| (String::new(), String::new()),
        |(console, _token)| {
            (
                console.settings().bridge.base.to_string(),
                console.settings().locale.tag(),
            )
        },
    );
    let (versions, document) = match info {
        Ok(Outcome::Shown(info)) => {
            let mut rows = vec![
                (
                    tr(Msg::AboutBridge),
                    format!("{} {}", info.product, info.version).into_any(),
                ),
                (
                    tr(Msg::AboutBuild),
                    view! { <code>{info.build.commit_short.clone()}</code> }.into_any(),
                ),
            ];
            rows.extend(
                info.pins
                    .iter()
                    .map(|pin| (pin.name.clone(), pin.version.clone().into_any())),
            );
            // NOTE: no specification governs this: our own design; a document with no
            // JSON form shows its rows and no self-description.
            (rows, serde_json::to_string_pretty(info).ok())
        }
        Ok(Outcome::Refused(refusal)) => (vec![(tr(Msg::AboutBridge), refused(refusal))], None),
        Err(fault) => (vec![(tr(Msg::AboutBridge), faulted(fault, ABOUT))], None),
    };
    let settings = card(
        tr(Msg::AboutConsoleSettings),
        definitions(vec![
            (
                tr(Msg::AboutBridgeAddress),
                view! { <code>{bridge}</code> }.into_any(),
            ),
            (
                tr(Msg::AboutLocale),
                view! { <code>{locale}</code> }.into_any(),
            ),
        ]),
    )
    .into_any();
    about_page(AboutPage {
        header: Header {
            title: tr(Msg::About).into(),
            subtitle: Some(tr(Msg::AboutSubtitle).into()),
            ..Header::default()
        },
        release: footer.release(),
        versions,
        manufacturer: Manufacturer {
            licence: tr(Msg::Licence),
            support: Link {
                label: String::from("www.cadasto.com/contact"),
                href: String::from("https://www.cadasto.com/contact/"),
            },
            documentation: Some(Link {
                label: String::from("ferrobridge.eu/docs"),
                href: String::from("https://ferrobridge.eu/docs/"),
            }),
        },
        cards: vec![settings],
        self_description: document.map(|text| SelfDescription {
            title: tr(Msg::AboutInfoTitle),
            line: tr(Msg::AboutInfoLine),
            language: Language::Json,
            text,
            copy_id: String::from("about-info-copy"),
            download: String::from(INFO_JSON),
        }),
        replica: None,
    })
}
