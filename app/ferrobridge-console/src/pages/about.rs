// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The About page: the Versions card with the bridge's build and pins, the
//! Manufacturer card, the console's own settings, and the bridge's
//! `GET /health/info` document as its self-description.

use ferrobridge_operator_api::health::Info;
use ferrohealth_ui::highlight::Language;
use ferrohealth_ui::html::Html;
use ferrohealth_ui::view::surface::{Header, card, definitions};
use ferrohealth_ui::view::template::about::{
    AboutPage, Link, Manufacturer, SelfDescription, about_page,
};

use crate::app::{ABOUT, Drawn};
use crate::locale::messages::Msg;
use crate::markup::{code, t};
use crate::model::{Loaded, Outcome};
use crate::pages::parts::{faulted, refused};
use crate::visit::Visit;

/// The route the self-description downloads from.
pub const INFO_JSON: &str = "/about/health-info.json";

/// The About page.
pub async fn page(visit: &Visit) -> Drawn {
    let info = visit.reads.info().await;
    Drawn::shell(visit.tr(Msg::About), drawn(visit, &info))
}

/// The page over the bridge's info.
fn drawn(visit: &Visit, info: &Loaded<Info>) -> Html {
    let l = &visit.locale;
    let footer = crate::shell::footer_names();
    let crate::visit::Settings { bridge, locale } = visit.settings.clone().unwrap_or_default();
    let (versions, document) = match info {
        Ok(Outcome::Shown(info)) => {
            let mut rows = vec![
                (
                    visit.tr(Msg::AboutBridge),
                    t(format!("{} {}", info.product, info.version)),
                ),
                (visit.tr(Msg::AboutBuild), code(&info.build.commit_short)),
            ];
            rows.extend(
                info.pins
                    .iter()
                    .map(|pin| (pin.name.clone(), t(&pin.version))),
            );
            // NOTE: no specification governs this: our own design; a document with no
            // JSON form shows its rows and no self-description.
            (rows, serde_json::to_string_pretty(info).ok())
        }
        Ok(Outcome::Refused(refusal)) => (
            vec![(visit.tr(Msg::AboutBridge), refused(l, refusal))],
            None,
        ),
        Err(fault) => (
            vec![(visit.tr(Msg::AboutBridge), faulted(l, fault, ABOUT))],
            None,
        ),
    };
    let settings = card(
        visit.tr(Msg::AboutConsoleSettings),
        definitions(vec![
            (visit.tr(Msg::AboutBridgeAddress), code(bridge)),
            (visit.tr(Msg::AboutLocale), code(locale)),
        ]),
    );
    about_page(
        l,
        AboutPage {
            header: Header {
                title: visit.tr(Msg::About).into(),
                subtitle: Some(visit.tr(Msg::AboutSubtitle).into()),
                ..Header::default()
            },
            release: footer.release(),
            versions,
            manufacturer: Manufacturer {
                licence: visit.tr(Msg::Licence),
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
                title: visit.tr(Msg::AboutInfoTitle),
                line: visit.tr(Msg::AboutInfoLine),
                language: Language::Json,
                text,
                copy_id: String::from("about-info-copy"),
                download: String::from(INFO_JSON),
            }),
            replica: None,
        },
    )
}
