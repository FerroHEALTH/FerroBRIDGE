// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The About page: the console, its version, its maker and its licence, and
//! for a signed-in operator the bridge it is a client of.

use leptos::prelude::*;
use leptos_meta::Title;

use crate::app::{MANUFACTURER, PRODUCT, VERSION};

/// The page that says what the console is and who makes it.
#[component]
#[expect(
    clippy::must_use_candidate,
    reason = "the #[component] macro drops a #[must_use] from the function it writes"
)]
pub fn AboutPage(
    /// The bridge's base URL, shown to a signed-in operator; empty for a
    /// visitor.
    bridge: String,
) -> impl IntoView {
    let bridge = (!bridge.is_empty()).then(|| {
        view! {
            <dt>"Bridge"</dt>
            <dd>
                <code>{bridge}</code>
            </dd>
        }
    });
    view! {
        <Title text=format!("About · {PRODUCT}") />
        {ferrohealth_ui::view::surface::page_header(
            "About",
            Some(format!("{PRODUCT} is the operator console of FerroBRIDGE, a bridge between openEHR, HL7 FHIR and the OMOP Common Data Model.")),
            (),
        )}
        <section class="fui-card">
        <h2 class="fui-card-title">{PRODUCT}</h2>
        <dl class="fui-facts">
            <dt>"Version"</dt>
            <dd>{VERSION}</dd>
            <dt>"Made by"</dt>
            <dd>
                {MANUFACTURER} " ("
                <a href="https://www.cadasto.com/contact/">"www.cadasto.com/contact"</a> ")"
            </dd>
            <dt>"Licence"</dt>
            <dd>"Business Source License 1.1"</dd>
            {bridge}
        </dl>
        </section>
    }
}
