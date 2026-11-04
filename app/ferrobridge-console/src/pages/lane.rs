// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! What the lane pages share.
//!
//! The FHIR facade, the HL7 v2 face and the OMOP ETL are each a detail page
//! (T2) over one lane, with its tabs, a header that says whether the lane is
//! on, and the reads every one of them opens with.

use ferrobridge_operator_api::configuration::Configuration;
use ferrobridge_operator_api::health::Info;
use ferrohealth_health::document::Readiness;
use ferrohealth_ui::html::Html;
use ferrohealth_ui::i18n::Locale;
use ferrohealth_ui::tone::Tone;
use ferrohealth_ui::view::action::ActionGroup;
use ferrohealth_ui::view::state::{Mark, state};
use ferrohealth_ui::view::status::badge;
use ferrohealth_ui::view::surface::Header;
use ferrohealth_ui::view::tabs::{Tab, TabRow};
use ferrohealth_ui::view::template::detail::{DetailPage, detail_page};

use crate::locale::messages::Msg;
use crate::locale::tr;
use crate::model::{Loaded, Outcome};
use crate::pages::parts::{faulted, refused};
use crate::visit::Visit;

/// One lane page: where it is served, its title, its tabs and the subtitle
/// its header carries when the lane is on and when it is off.
#[derive(Debug, Clone, Copy)]
pub struct Lane {
    /// The page's path; each tab but Overview sits under it.
    pub path: &'static str,
    /// The page's title, which also labels its tab row.
    pub title: Msg,
    /// The tabs, each its key and its words.
    pub tabs: &'static [(&'static str, Msg)],
    /// The header's subtitle while the lane is on.
    pub on: Msg,
    /// The header's subtitle while the lane is off.
    pub off: Msg,
}

impl Lane {
    /// Returns the tab the route of `visit` names, `None` for one the page
    /// does not have.
    #[must_use]
    pub fn tab(&self, visit: &Visit) -> Option<String> {
        let tab = visit.segment(1).unwrap_or("overview");
        self.tabs
            .iter()
            .any(|(key, _)| *key == tab)
            .then(|| tab.to_owned())
    }

    /// Returns what a page on the tab the route of `visit` names opens
    /// with, `None` for a tab the page does not have.
    pub async fn open(&self, visit: &Visit) -> Option<Opened> {
        let tab = self.tab(visit)?;
        let (info, readiness, configuration) = opening(visit, &tab).await;
        Some((tab, info, readiness, configuration))
    }

    /// Returns the page of the tab `key`.
    #[must_use]
    pub fn href(&self, key: &str) -> String {
        if key == "overview" {
            String::from(self.path)
        } else {
            format!("{}/{key}", self.path)
        }
    }

    /// Returns the tab row with `tab` current, each tab `drawn` passes.
    #[must_use]
    pub fn tab_row(
        &self,
        l: &Locale,
        tab: &str,
        drawn: impl Fn(&str, Tab) -> Option<Tab>,
    ) -> TabRow {
        TabRow {
            label: tr(l, self.title),
            tabs: self
                .tabs
                .iter()
                .filter_map(|(key, words)| {
                    drawn(key, Tab::new(*key, self.href(key), tr(l, *words)))
                })
                .collect(),
            current: tab.to_owned(),
        }
    }

    /// Returns the page's header, for a lane that is `on` or off.
    #[must_use]
    pub fn header(&self, l: &Locale, on: bool) -> Header {
        Header {
            title: tr(l, self.title).into(),
            subtitle: Some(tr(l, if on { self.on } else { self.off }).into()),
            subtitle_view: None,
            crumbs: Vec::new(),
            mono: false,
            badge: Some(if on {
                state(Mark::Ok, tr(l, Msg::StateOn), None)
            } else {
                badge(Tone::Neutral, tr(l, Msg::StateOff))
            }),
        }
    }

    /// Returns the detail page of the tab `tab`, every tab shown, under
    /// `header` around `body`.
    #[must_use]
    pub fn framed(&self, l: &Locale, tab: &str, header: Header, body: Html) -> Html {
        Self::page(
            l,
            header,
            self.tab_row(l, tab, |_, drawn| Some(drawn)),
            body,
        )
    }

    /// Returns the detail page of `tabs` under `header` around `body`.
    #[must_use]
    pub fn page(l: &Locale, header: Header, tabs: TabRow, body: Html) -> Html {
        detail_page(
            l,
            DetailPage {
                header,
                actions: ActionGroup::none(),
                stepper: None,
                tabs: Some(tabs),
                body,
            },
        )
    }

    /// Returns the lanes and pins `info` answered, or the page that says
    /// why the bridge answered none, with a retry at the tab `tab`.
    ///
    /// # Errors
    ///
    /// The refusal or the fault page, drawn.
    pub fn shown<'a>(
        &self,
        l: &Locale,
        info: &'a Loaded<Info>,
        tab: &str,
    ) -> Result<&'a Info, Html> {
        match info {
            Ok(Outcome::Shown(info)) => Ok(info),
            Ok(Outcome::Refused(refusal)) => Err(refused(l, refusal)),
            Err(fault) => Err(faulted(l, fault, &self.href(tab))),
        }
    }
}

/// What every lane page opens with: the tab its route names, the bridge's
/// lanes and pins, its readiness, and on the Settings tab its running
/// configuration.
pub type Opened = (
    String,
    Loaded<Info>,
    Loaded<Readiness>,
    Option<Loaded<Configuration>>,
);

/// Reads what a page on the tab `tab` opens with besides the tab: the
/// bridge's lanes and pins, its readiness, and on the Settings tab its
/// running configuration.
pub async fn opening(
    visit: &Visit,
    tab: &str,
) -> (
    Loaded<Info>,
    Loaded<Readiness>,
    Option<Loaded<Configuration>>,
) {
    let info = visit.reads.info().await;
    let readiness = visit.reads.readiness().await;
    let configuration = if tab == "settings" {
        Some(visit.reads.configuration().await)
    } else {
        None
    };
    (info, readiness, configuration)
}
